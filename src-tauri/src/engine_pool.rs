use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::{DeepLEngine, FallbackEngine, GoogleWebEngine, OpenAIEngine, TranslateEngine};

pub type SecretGetter = Arc<dyn Fn(&str) -> anyhow::Result<String> + Send + Sync>;

/// 引擎连接池与单例缓存管理器
#[derive(Clone)]
pub struct EnginePool {
    default_engine: Arc<dyn TranslateEngine>,
    cache: Arc<RwLock<HashMap<String, Arc<dyn TranslateEngine>>>>,
    raw_cache: Arc<RwLock<HashMap<String, Arc<dyn TranslateEngine>>>>,
    secret_getter: Option<SecretGetter>,
    app_cache: Option<Arc<transly_core::TranslationCache>>,
}

impl EnginePool {
    pub fn new() -> Self {
        let google = Arc::new(GoogleWebEngine::new());
        Self {
            default_engine: google,
            cache: Arc::new(RwLock::new(HashMap::new())),
            raw_cache: Arc::new(RwLock::new(HashMap::new())),
            secret_getter: None,
            app_cache: None,
        }
    }

    /// 关联本地 SQLite 缓存管理器以优先读取配置，避免阻塞冷启动
    pub fn with_cache(app_cache: Arc<transly_core::TranslationCache>) -> Self {
        let google = Arc::new(GoogleWebEngine::new());
        Self {
            default_engine: google,
            cache: Arc::new(RwLock::new(HashMap::new())),
            raw_cache: Arc::new(RwLock::new(HashMap::new())),
            secret_getter: None,
            app_cache: Some(app_cache),
        }
    }

    /// 测试专用：注入模拟凭据读取器，避免触碰开发者与 CI 的真实系统凭据库。
    ///
    /// 生产路径 `new()` 恒为 `None`（走 `crate::secrets::get_secret`），
    /// 因此不对外暴露该注入点。
    #[cfg(test)]
    pub fn with_secret_getter(getter: SecretGetter) -> Self {
        let google = Arc::new(GoogleWebEngine::new());
        Self {
            default_engine: google,
            cache: Arc::new(RwLock::new(HashMap::new())),
            raw_cache: Arc::new(RwLock::new(HashMap::new())),
            secret_getter: Some(getter),
            app_cache: None,
        }
    }

    /// 测试专用：注入零网络、零凭据的默认引擎。
    ///
    /// 生产默认引擎是 `GoogleWebEngine`，测试若直接用 `new()` 会在缓存未命中时
    /// 真实外呼 translate.googleapis.com，违反“CI 不真调外部 API”规则，
    /// 且断言恒为 200 使用例失去信号。
    #[cfg(test)]
    pub fn with_default_engine(default_engine: Arc<dyn TranslateEngine>) -> Self {
        Self {
            default_engine,
            cache: Arc::new(RwLock::new(HashMap::new())),
            raw_cache: Arc::new(RwLock::new(HashMap::new())),
            secret_getter: None,
            app_cache: None,
        }
    }

    pub fn default_engine(&self) -> Arc<dyn TranslateEngine> {
        self.default_engine.clone()
    }

    /// 获取不带 Fallback 降级包装的原生引擎实例。
    /// 专用于划词原地替换等对结果质量要求严苛、禁止静默隐蔽降级的场景。
    pub async fn get_raw_engine(&self, engine_id: &str) -> Arc<dyn TranslateEngine> {
        let id_clean = engine_id.trim().to_lowercase();
        if id_clean.is_empty() || id_clean == "google" || id_clean == "google_web" {
            return self.default_engine.clone();
        }

        {
            let map = self.raw_cache.read().await;
            if let Some(engine) = map.get(&id_clean) {
                return engine.clone();
            }
        }

        let created = self.create_raw_engine_blocking(&id_clean).await;

        let mut map = self.raw_cache.write().await;
        if let Some(engine) = map.get(&id_clean) {
            return engine.clone();
        }

        map.insert(id_clean, created.clone());
        created
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

    async fn create_raw_engine_blocking(&self, engine_id: &str) -> Arc<dyn TranslateEngine> {
        let id = engine_id.to_string();
        let fallback = self.default_engine.clone();
        let custom_getter = self.secret_getter.clone();
        let app_cache = self.app_cache.clone();

        tokio::task::spawn_blocking(move || {
            let get_key = |key_name: &str| -> String {
                if let Some(ref getter) = custom_getter {
                    return getter(key_name).unwrap_or_default();
                }
                // 优先从本地 SQLite 设置表读取，消除对凭据库的阻塞查询
                if let Some(ref c) = app_cache {
                    if let Ok(Some(val)) = c.get_setting(key_name) {
                        return val;
                    }
                }
                crate::secrets::get_secret(key_name).unwrap_or_default()
            };

            match id.as_str() {
                "deepl" => {
                    let key = get_key("deepl");
                    Arc::new(DeepLEngine::new(key)) as Arc<dyn TranslateEngine>
                }
                "openai" | "deepseek" => {
                    let key = get_key(&id);
                    let custom_url = get_key(&format!("{id}_base_url"));
                    let custom_model = get_key(&format!("{id}_model"));
                    let base_url = if custom_url.trim().is_empty() {
                        None
                    } else {
                        Some(custom_url)
                    };
                    let model = if custom_model.trim().is_empty() {
                        None
                    } else {
                        Some(custom_model)
                    };

                    let style_str = get_key("llm_style");
                    let style = match style_str.to_lowercase().as_str() {
                        "academic" => transly_core::engine::openai::TranslationStyle::Academic,
                        "literary" => transly_core::engine::openai::TranslationStyle::Literary,
                        "business" => transly_core::engine::openai::TranslationStyle::Business,
                        _ => transly_core::engine::openai::TranslationStyle::General,
                    };

                    let llm: Arc<dyn TranslateEngine> = if id == "deepseek" {
                        let ds_url =
                            base_url.unwrap_or_else(|| "https://api.deepseek.com".to_string());
                        let ds_model = model.unwrap_or_else(|| "deepseek-chat".to_string());
                        let mut engine =
                            OpenAIEngine::new(key, Some(ds_url), Some(ds_model.clone()))
                                .with_style(style);
                        engine.set_engine_id(format!("deepseek:{ds_model}"));
                        Arc::new(engine)
                    } else {
                        Arc::new(OpenAIEngine::new(key, base_url, model).with_style(style))
                    };
                    llm
                }
                _ => fallback,
            }
        })
        .await
        .unwrap_or(self.default_engine.clone())
    }

    async fn create_engine_blocking(&self, engine_id: &str) -> Arc<dyn TranslateEngine> {
        let raw = self.get_raw_engine(engine_id).await;
        let id_clean = engine_id.trim().to_lowercase();
        if id_clean == "deepl" || id_clean == "openai" || id_clean == "deepseek" {
            Arc::new(FallbackEngine::new(raw, self.default_engine.clone()))
        } else {
            raw
        }
    }

    /// 密钥更新或删除时，使对应引擎缓存失效。
    /// 归一化规则必须与 `get_engine` 一致，否则会漏删缓存导致旧密钥被继续复用。
    pub async fn invalidate(&self, engine_id: &str) {
        let id = engine_id.trim().to_lowercase();
        let mut map = self.cache.write().await;
        map.remove(&id);
        let mut raw_map = self.raw_cache.write().await;
        raw_map.remove(&id);
    }
}

impl Default for EnginePool {
    fn default() -> Self {
        Self::new()
    }
}

/// 测试专用零网络、零凭据的翻译引擎桩。
///
/// 仅用于验证服务端编排逻辑（缓存门禁、参数校验、术语库替换），
/// 绝不发起任何外部网络请求，也不触碰系统凭据管理器。
#[cfg(test)]
pub mod testing {
    use async_trait::async_trait;
    use transly_core::{Result, TranslateEngine, TranslationRequest, TranslationResponse};

    pub struct StubEngine;

    #[async_trait]
    impl TranslateEngine for StubEngine {
        fn id(&self) -> &str {
            "google_web"
        }

        fn name(&self) -> &str {
            "Stub 测试引擎"
        }

        async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
            Ok(TranslationResponse {
                text: format!("[stub:{}]{}", req.target_lang, req.text),
                detected_source_lang: Some("en".to_string()),
                engine: self.id().to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_raw_engine_isolation_and_fallback_contrast() {
        let pool = EnginePool::new();
        let fallback_wrapped = pool.get_engine("deepl").await;
        assert_eq!(
            fallback_wrapped.capabilities().fallback_engine,
            Some("google_web".to_string())
        );

        let raw_engine = pool.get_raw_engine("deepl").await;
        assert_eq!(raw_engine.capabilities().fallback_engine, None);
    }
}
