use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub const PAIRING_SESSION_TTL_SECS: u64 = 60;

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
}

impl PairingManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// 创建一个有效期 60 秒的配对会话。
    /// 为抑制无序并发，同时使该 client_id 或其他过期的未决会话失效。
    pub async fn create_session(
        &self,
        client_id: String,
        browser_hint: Option<String>,
    ) -> (String, u64) {
        let session_id = crate::secrets::generate_token();
        let now = Instant::now();
        let expires_at = now + Duration::from_secs(PAIRING_SESSION_TTL_SECS);

        let mut lock = self.sessions.write().await;
        // 清理所有已过期的会话，以及同一 client_id 的旧会话
        lock.retain(|_, s| s.expires_at > now && s.client_id != client_id);

        let session = PairingSession {
            session_id: session_id.clone(),
            client_id,
            browser_hint,
            created_at: now,
            expires_at,
            status: PairingStatus::Pending,
        };
        lock.insert(session_id.clone(), session);

        (session_id, PAIRING_SESSION_TTL_SECS)
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

        let Some(session) = lock.get_mut(session_id) else {
            return PairingPollResult::Expired;
        };

        if session.client_id != client_id || now >= session.expires_at {
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
        if let Some(session) = lock.get_mut(session_id) {
            if now < session.expires_at && session.status == PairingStatus::Pending {
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
        if let Some(session) = lock.get_mut(session_id) {
            if now < session.expires_at && session.status == PairingStatus::Pending {
                session.status = PairingStatus::Rejected;
                return true;
            }
        }
        false
    }

    /// 获取当前处于 Pending 状态的会话信息（供前端回显或事件补发）
    pub async fn get_pending_session(&self, session_id: &str) -> Option<PairingSession> {
        let lock = self.sessions.read().await;
        let now = Instant::now();
        lock.get(session_id).and_then(|s| {
            if now < s.expires_at && s.status == PairingStatus::Pending {
                Some(s.clone())
            } else {
                None
            }
        })
    }
}

impl Default for PairingManager {
    fn default() -> Self {
        Self::new()
    }
}
