use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::{DeepLEngine, FallbackEngine, GoogleWebEngine, OpenAIEngine, TranslateEngine};

/// 引擎连接池与单例缓存管理器
#[derive(Clone)]
pub struct EnginePool {
    default_engine: Arc<dyn TranslateEngine>,
    cache: Arc<RwLock<HashMap<String, Arc<dyn TranslateEngine>>>>,
}

impl EnginePool {
    pub fn new() -> Self {
        let google = Arc::new(GoogleWebEngine::new());
        Self {
            default_engine: google,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn default_engine(&self) -> Arc<dyn TranslateEngine> {
        self.default_engine.clone()
    }

    /// 获取或按需复用创建引擎实例（已配置 Fallback 降级）
    pub async fn get_engine(&self, engine_id: &str) -> Arc<dyn TranslateEngine> {
        let id_clean = engine_id.trim().to_lowercase();
        if id_clean.is_empty() || id_clean == "google" || id_clean == "google_web" {
            return self.default_engine.clone();
        }

        // 1. 尝试读锁快速获取
        {
            let map = self.cache.read().await;
            if let Some(engine) = map.get(&id_clean) {
                return engine.clone();
            }
        }

        // 2. 写锁创建并缓存
        let mut map = self.cache.write().await;
        if let Some(engine) = map.get(&id_clean) {
            return engine.clone();
        }

        let created = self.create_engine(&id_clean);
        map.insert(id_clean, created.clone());
        created
    }

    fn create_engine(&self, engine_id: &str) -> Arc<dyn TranslateEngine> {
        let fallback = self.default_engine.clone();
        match engine_id {
            "deepl" => {
                if let Ok(key) = crate::secrets::get_secret("deepl") {
                    if !key.is_empty() {
                        let deepl = Arc::new(DeepLEngine::new(key));
                        return Arc::new(FallbackEngine::new(deepl, fallback));
                    }
                }
                fallback
            }
            "openai" | "deepseek" => {
                if let Ok(key) = crate::secrets::get_secret(engine_id) {
                    if !key.is_empty() {
                        let llm: Arc<dyn TranslateEngine> = if engine_id == "deepseek" {
                            Arc::new(OpenAIEngine::deepseek(key))
                        } else {
                            Arc::new(OpenAIEngine::new(key, None, None))
                        };
                        return Arc::new(FallbackEngine::new(llm, fallback));
                    }
                }
                fallback
            }
            _ => fallback,
        }
    }

    /// 密钥更新或删除时，使对应引擎缓存失效
    pub async fn invalidate(&self, engine_id: &str) {
        let mut map = self.cache.write().await;
        map.remove(&engine_id.to_lowercase());
    }
}

impl Default for EnginePool {
    fn default() -> Self {
        Self::new()
    }
}
