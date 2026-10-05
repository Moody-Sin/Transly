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

        // 2. 避免在持有写锁期间调用同步阻塞的 keyring I/O (secrets::get_secret)
        // 先在锁外安全执行阻塞凭据读取和引擎构建
        let created = self.create_engine_blocking(&id_clean).await;

        // 3. 获取写锁写入缓存（双重校验，杜绝重复写入覆盖）
        let mut map = self.cache.write().await;
        if let Some(engine) = map.get(&id_clean) {
            return engine.clone();
        }

        map.insert(id_clean, created.clone());
        created
    }

    async fn create_engine_blocking(&self, engine_id: &str) -> Arc<dyn TranslateEngine> {
        let id = engine_id.to_string();
        let fallback = self.default_engine.clone();

        tokio::task::spawn_blocking(move || match id.as_str() {
            "deepl" => {
                if let Ok(key) = crate::secrets::get_secret("deepl") {
                    if !key.trim().is_empty() {
                        let deepl = Arc::new(DeepLEngine::new(key));
                        return Arc::new(FallbackEngine::new(deepl, fallback))
                            as Arc<dyn TranslateEngine>;
                    }
                }
                fallback
            }
            "openai" | "deepseek" => {
                if let Ok(key) = crate::secrets::get_secret(&id) {
                    if !key.trim().is_empty() {
                        let llm: Arc<dyn TranslateEngine> = if id == "deepseek" {
                            Arc::new(OpenAIEngine::deepseek(key))
                        } else {
                            Arc::new(OpenAIEngine::new(key, None, None))
                        };
                        return Arc::new(FallbackEngine::new(llm, fallback))
                            as Arc<dyn TranslateEngine>;
                    }
                }
                fallback
            }
            _ => fallback,
        })
        .await
        .unwrap_or(self.default_engine.clone())
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
