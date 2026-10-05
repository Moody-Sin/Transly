use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// 缓存状态协调器：采用 generation 版本号机制，彻底杜绝“翻译中关闭缓存导致清空后被重新写回”的竞态漏洞
#[derive(Clone)]
pub struct CacheController {
    enabled: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}

impl CacheController {
    pub fn new(initial_enabled: bool) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(initial_enabled)),
            generation: Arc::new(AtomicU64::new(1)),
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

    /// 校验请求开始时记录的 generation 是否仍然有效且缓存保持开启
    pub fn is_valid_to_write(&self, start_gen: u64) -> bool {
        self.enabled.load(Ordering::Relaxed) && self.generation.load(Ordering::Relaxed) == start_gen
    }

    /// 切换缓存开关：若为关闭操作，原子递增 generation 宣告此前所有进行中的请求写入失效
    pub fn set_enabled(&self, enabled: bool) -> u64 {
        self.enabled.store(enabled, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }
}
