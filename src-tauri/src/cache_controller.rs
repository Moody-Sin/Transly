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
        let _guard = self.sync_gate.lock().unwrap();
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
        let _guard = self.sync_gate.lock().unwrap();
        self.enabled.store(false, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::SeqCst);
        cache.clear_all_cache()
    }

    /// 启用缓存并原子递增 generation
    pub fn enable(&self) {
        let _guard = self.sync_gate.lock().unwrap();
        self.enabled.store(true, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}
