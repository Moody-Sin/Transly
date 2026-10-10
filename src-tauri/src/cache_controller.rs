use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use transly_core::{Result, TranslationCache};

/// 缓存状态协调器：采用 generation 版本号 + 同步门禁锁，彻底杜绝“检查有效与写入之间插入关闭清空”的竞态漏洞
#[derive(Clone)]
pub struct CacheController {
    enabled: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    sync_gate: Arc<Mutex<()>>,
}

impl CacheController {
    pub fn new(initial_enabled: bool) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(initial_enabled)),
            generation: Arc::new(AtomicU64::new(1)),
            sync_gate: Arc::new(Mutex::new(())),
        }
    }

    /// 获取当前状态快照：是否开启及当前 generation 版本
    pub fn snapshot(&self) -> (bool, u64) {
        (
            self.enabled.load(Ordering::Relaxed),
            self.generation.load(Ordering::Relaxed),
        )
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// 原子条件写入：在门禁锁内同时验证 generation 版本与开启状态并执行写入，杜绝任何检查与写入间的竞态窗口
    /// 返回值语义：
    /// - Ok(true): 版本有效且写入成功
    /// - Ok(false): 请求已跨越关闭/切换边界，安全拒绝写入
    /// - Err(e): 版本有效，但底层存储写入失败（便于诊断与告警，不静默吞错）
    pub fn set_if_current<F>(&self, start_gen: u64, write_fn: F) -> Result<bool>
    where
        F: FnOnce() -> Result<()>,
    {
        // 门禁锁不守护任何数据（仅 () ），因此即使上一次持锁过程 panic 导致中毒，
        // 直接取回内部值也是安全的，避免一次 panic 永久瘫痪整个缓存写入路径。
        let _guard = self.sync_gate.lock().unwrap_or_else(|e| e.into_inner());
        if self.enabled.load(Ordering::Relaxed)
            && self.generation.load(Ordering::Relaxed) == start_gen
        {
            write_fn().map(|_| true)
        } else {
            Ok(false)
        }
    }

    /// 原子关闭缓存并执行清空操作：在同一同步门禁内递增 generation、切换状态并清空磁盘缓存
    pub fn disable_and_clear(&self, cache: &TranslationCache) -> Result<usize> {
        let _guard = self.sync_gate.lock().unwrap_or_else(|e| e.into_inner());
        self.enabled.store(false, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::SeqCst);
        cache.clear_all_cache()
    }

    /// 原子清空缓存但保持启用状态：在同一同步门禁内递增 generation 后清空。
    ///
    /// 与 `disable_and_clear` 的区别是**不**关闭缓存开关——「清空历史」是用户
    /// 只想删记录、保持功能开启的操作。
    ///
    /// 必须递增 generation：否则在途请求持有的 `start_gen` 仍会通过
    /// `set_if_current` 判定为「当前代」，把用户刚清掉的记录重新写回。
    pub fn clear_only(&self, cache: &TranslationCache) -> Result<usize> {
        let _guard = self.sync_gate.lock().unwrap_or_else(|e| e.into_inner());
        self.generation.fetch_add(1, Ordering::SeqCst);
        cache.clear_all_cache()
    }

    /// 启用缓存并原子递增 generation
    pub fn enable(&self) {
        let _guard = self.sync_gate.lock().unwrap_or_else(|e| e.into_inner());
        self.enabled.store(true, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// 在阻塞线程池中执行「代际校验 + 缓存写入」。
///
/// `set_if_current` 是**同步**函数：它先抢全局 `sync_gate`，再于锁内同步执行
/// SQLite 写；写连接配置了 `PRAGMA busy_timeout = 5000`，因此一次长事务可占用
/// 一个 tokio worker 长达 5 秒。而 `sync_gate` 是全局串行门禁，任何一次长事务
/// 会让全部并发翻译的缓存写一起排队。直接从 async 上下文调用即构成阻塞 I/O，
/// 与项目「Tauri command 内禁止阻塞调用」的规程冲突。
///
/// 所有缓存写入口都必须经由此函数下沉到 `spawn_blocking`。
pub async fn write_cache_offloaded<F>(controller: Arc<CacheController>, start_gen: u64, write: F)
where
    F: FnOnce() -> Result<()> + Send + 'static,
{
    match tokio::task::spawn_blocking(move || controller.set_if_current(start_gen, write)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => eprintln!("[Cache Write Warning] 缓存写入失败: {e}"),
        Err(join_err) => {
            eprintln!("[Cache Write Warning] 缓存写入任务调度异常: {join_err}")
        }
    }
}
