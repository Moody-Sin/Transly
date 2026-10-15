use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub const PAIRING_SESSION_TTL_SECS: u64 = 60;
/// 最大允许同时存在的未决配对会话数，防止恶意本地客户端泛洪耗尽内存
pub const MAX_PENDING_SESSIONS: usize = 16;

/// 同一 `client_id` 两次发起配对之间的最小间隔（秒）。
///
/// `/pair/request` 位于鉴权之外（配对的本意就是「尚未持有 Token」），
/// 因此本机任意进程都能直接调用。没有节流时，恶意程序可反复请求：
/// 每次请求都会弹出桌面端配对窗口（通知轰炸），并挤占会话表。
pub const PAIRING_MIN_REQUEST_INTERVAL_SECS: u64 = 5;

/// 全局限流窗口（秒）与窗口内允许的新会话数。
///
/// 攻击者可用随机 client_id 绕过按客户端的节流；若没有全局约束，
/// 16 个并发请求即可占满会话表，令正常扩展在整个 TTL 内无法配对。
///
/// 采用「窗口内允许突发」而非固定最小间隔：正常用户可能有多个浏览器、
/// 或需要立刻重试一次，固定间隔会误伤这些场景。窗口 10 秒内允许 3 个新会话，
/// 足以覆盖正常使用；而灌满 16 个会话至少需要约 50 秒，其间早先的会话已过期，
/// 无法形成有效的拒绝服务。
pub const PAIRING_GLOBAL_WINDOW_SECS: u64 = 10;
pub const PAIRING_GLOBAL_BURST: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PairingStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

#[derive(Debug, Clone)]
pub struct PairingSession {
    pub session_id: String,
    pub client_id: String,
    pub browser_hint: Option<String>,
    pub created_at: Instant,
    pub expires_at: Instant,
    pub status: PairingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum PairingPollResult {
    Pending,
    Approved { token: String },
    Rejected,
    Expired,
}

pub struct PairingManager {
    sessions: RwLock<HashMap<String, PairingSession>>,
    /// 每个 client_id 最近一次发起配对的时间，用于节流。
    last_request_at: RwLock<HashMap<String, Instant>>,
    /// 全局限流窗口内最近的建会话时间戳（滑动窗口）。
    ///
    /// 仅有按 client_id 的节流不足以防护：攻击者换用随机 client_id 即可绕过，
    /// 只需 16 个并发请求就能占满会话表，使正常扩展在随后的 TTL（60 秒）内
    /// 全部被拒。因此在会话表写满之前先做全局入口限流。
    recent_creations: RwLock<std::collections::VecDeque<Instant>>,
}

/// `create_session` 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateSessionOutcome {
    /// 新建会话成功（session_id, 有效期秒数）
    Created(String, u64),
    /// 同一 client_id 请求过于频繁，被节流拒绝
    Throttled,
    /// 未决会话已达上限且无过期会话可回收，拒绝新请求
    ///
    /// 注意这里**不再**淘汰既有会话：旧实现按 created_at 淘汰最早的会话，
    /// 使得本机任意进程都能用 16 个请求把正常浏览器扩展正在等待的配对会话挤掉
    /// （扩展轮询立即收到 Expired），构成拒绝服务。
    AtCapacity,
}

impl PairingManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            last_request_at: RwLock::new(HashMap::new()),
            recent_creations: RwLock::new(std::collections::VecDeque::new()),
        }
    }

    /// 创建一个有效期 60 秒的配对会话。
    ///
    /// 同一 `client_id` 重复请求时**复用**其既有未决会话（而非作废旧会话），
    /// 既避免扩展重试导致自身会话失效，也让节流判断有稳定语义。
    pub async fn create_session(
        &self,
        client_id: String,
        browser_hint: Option<String>,
    ) -> CreateSessionOutcome {
        let now = Instant::now();

        // 1. 节流：同一 client_id 在窗口内重复请求直接拒绝
        {
            let mut last = self.last_request_at.write().await;
            if let Some(prev) = last.get(&client_id) {
                if now.duration_since(*prev)
                    < Duration::from_secs(PAIRING_MIN_REQUEST_INTERVAL_SECS)
                {
                    return CreateSessionOutcome::Throttled;
                }
            }
            last.insert(client_id.clone(), now);
            // 防止 last_request_at 随一次性 client_id 无界增长
            if last.len() > MAX_PENDING_SESSIONS * 4 {
                last.retain(|_, t| now.duration_since(*t) < Duration::from_secs(300));
            }
        }

        let mut lock = self.sessions.write().await;

        // 2. 清理已过期会话（真正的回收，不误伤有效会话）
        lock.retain(|_, s| s.expires_at > now);

        // 3. 同一 client_id 已有未决会话则复用，避免自我作废
        if let Some(existing) = lock.values().find(|s| s.client_id == client_id) {
            let remaining = existing
                .expires_at
                .saturating_duration_since(now)
                .as_secs()
                .max(1);
            return CreateSessionOutcome::Created(existing.session_id.clone(), remaining);
        }

        // 4. 全局限流：无论 client_id 如何变化，创建新会话都受最小间隔约束。
        //    这是对「随机 client_id 灌满会话表」的防护 —— 单靠按 client_id 节流
        //    可被轻易绕过（每次换一个新 ID 即可）。
        {
            let mut recent = self.recent_creations.write().await;
            let window = Duration::from_secs(PAIRING_GLOBAL_WINDOW_SECS);
            while recent
                .front()
                .is_some_and(|t| now.duration_since(*t) >= window)
            {
                recent.pop_front();
            }
            if recent.len() >= PAIRING_GLOBAL_BURST {
                return CreateSessionOutcome::Throttled;
            }
            recent.push_back(now);
        }

        // 5. 容量保护：已满则拒绝，绝不淘汰他人有效会话
        if lock.len() >= MAX_PENDING_SESSIONS {
            return CreateSessionOutcome::AtCapacity;
        }

        let session_id = crate::secrets::generate_token();
        let expires_at = now + Duration::from_secs(PAIRING_SESSION_TTL_SECS);

        let session = PairingSession {
            session_id: session_id.clone(),
            client_id,
            browser_hint,
            created_at: now,
            expires_at,
            status: PairingStatus::Pending,
        };
        lock.insert(session_id.clone(), session);

        CreateSessionOutcome::Created(session_id, PAIRING_SESSION_TTL_SECS)
    }

    /// 轮询会话状态。
    /// 若 status 为 Approved，则一次性返回真实的 Token 并立即删除该会话（防重放与二次读取）。
    pub async fn poll_session(
        &self,
        session_id: &str,
        client_id: &str,
        token: &str,
    ) -> PairingPollResult {
        let mut lock = self.sessions.write().await;
        let now = Instant::now();
        // 轮询时顺带执行轻量惰性过期清理
        lock.retain(|_, s| now < s.expires_at);

        let Some(session) = lock.get_mut(session_id) else {
            return PairingPollResult::Expired;
        };

        if session.client_id != client_id {
            lock.remove(session_id);
            return PairingPollResult::Expired;
        }

        match session.status {
            PairingStatus::Pending => PairingPollResult::Pending,
            PairingStatus::Rejected => {
                let res = PairingPollResult::Rejected;
                lock.remove(session_id);
                res
            }
            PairingStatus::Expired => {
                lock.remove(session_id);
                PairingPollResult::Expired
            }
            PairingStatus::Approved => {
                let res = PairingPollResult::Approved {
                    token: token.to_string(),
                };
                // 一次性消费：读取 Token 后立即销毁
                lock.remove(session_id);
                res
            }
        }
    }

    /// 桌面端用户允许配对
    pub async fn approve(&self, session_id: &str) -> bool {
        let mut lock = self.sessions.write().await;
        let now = Instant::now();
        // 顺带清理已超时的陈旧会话，防止极端探测场景下的滞留
        lock.retain(|_, s| now < s.expires_at);

        if let Some(session) = lock.get_mut(session_id) {
            if session.status == PairingStatus::Pending {
                session.status = PairingStatus::Approved;
                return true;
            }
        }
        false
    }

    /// 桌面端用户拒绝配对
    pub async fn reject(&self, session_id: &str) -> bool {
        let mut lock = self.sessions.write().await;
        let now = Instant::now();
        // 顺带清理已超时的陈旧会话，防止极端探测场景下的滞留
        lock.retain(|_, s| now < s.expires_at);

        if let Some(session) = lock.get_mut(session_id) {
            if session.status == PairingStatus::Pending {
                session.status = PairingStatus::Rejected;
                return true;
            }
        }
        false
    }
}

impl Default for PairingManager {
    fn default() -> Self {
        Self::new()
    }
}
