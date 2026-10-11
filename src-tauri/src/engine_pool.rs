use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::{
    DeepLEngine, FallbackEngine, GoogleWebEngine, OpenAIEngine, ProxyConfig, ProxyMode,
    TranslateEngine, TranslationCache,
};

pub type SecretGetter = Arc<dyn Fn(&str) -> anyhow::Result<String> + Send + Sync>;

/// 引擎连接池与单例缓存管理器
#[derive(Clone)]
pub struct EnginePool {
    default_engine: Arc<std::sync::RwLock<Arc<dyn TranslateEngine>>>,
    cache: Arc<RwLock<HashMap<String, Arc<dyn TranslateEngine>>>>,
    raw_cache: Arc<RwLock<HashMap<String, Arc<dyn TranslateEngine>>>>,
    secret_getter: Option<SecretGetter>,
    app_cache: Option<Arc<TranslationCache>>,
    is_custom_default: bool,
}

impl EnginePool {
    fn resolve_proxy_from_cache(cache: Option<&Arc<TranslationCache>>) -> (Option<String>, bool) {
        if let Some(c) = cache {
            let mode_str = c
                .get_setting("proxy_mode")
                .ok()
                .flatten()
                .unwrap_or_else(|| "auto".to_string());
            let mode = ProxyMode::parse_str(&mode_str);
            let url = c.get_setting("proxy_url").ok().flatten();
            let config = ProxyConfig { mode, url };
            crate::proxy::resolve_effective_proxy(&config)
        } else {
            let config = ProxyConfig::default();
            crate::proxy::resolve_effective_proxy(&config)
        }
    }

    fn create_default_google(proxy: &(Option<String>, bool)) -> Arc<dyn TranslateEngine> {
        Arc::new(GoogleWebEngine::new_with_proxy(proxy.0.as_deref(), proxy.1))
    }

    pub fn new() -> Self {
        let proxy = Self::resolve_proxy_from_cache(None);
        let google = Self::create_default_google(&proxy);
        Self {
            default_engine: Arc::new(std::sync::RwLock::new(google)),
            cache: Arc::new(RwLock::new(HashMap::new())),
            raw_cache: Arc::new(RwLock::new(HashMap::new())),
            secret_getter: None,
            app_cache: None,
            is_custom_default: false,
        }
    }

    /// 关联本地 SQLite 缓存管理器以优先读取配置，避免阻塞冷启动
    pub fn with_cache(app_cache: Arc<TranslationCache>) -> Self {
        let proxy = Self::resolve_proxy_from_cache(Some(&app_cache));
        let google = Self::create_default_google(&proxy);
        Self {
            default_engine: Arc::new(std::sync::RwLock::new(google)),
            cache: Arc::new(RwLock::new(HashMap::new())),
            raw_cache: Arc::new(RwLock::new(HashMap::new())),
            secret_getter: None,
            app_cache: Some(app_cache),
            is_custom_default: false,
        }
    }

    /// 测试专用：注入模拟凭据读取器，避免触碰开发者与 CI 的真实系统凭据库。
    #[cfg(test)]
    pub fn with_secret_getter(getter: SecretGetter) -> Self {
        let proxy = Self::resolve_proxy_from_cache(None);
        let google = Self::create_default_google(&proxy);
        Self {
            default_engine: Arc::new(std::sync::RwLock::new(google)),
            cache: Arc::new(RwLock::new(HashMap::new())),
            raw_cache: Arc::new(RwLock::new(HashMap::new())),
            secret_getter: Some(getter),
            app_cache: None,
            is_custom_default: false,
        }
    }

    /// 测试专用：注入零网络、零凭据的默认引擎。
    pub fn with_default_engine(default_engine: Arc<dyn TranslateEngine>) -> Self {
        Self {
            default_engine: Arc::new(std::sync::RwLock::new(default_engine)),
            cache: Arc::new(RwLock::new(HashMap::new())),
            raw_cache: Arc::new(RwLock::new(HashMap::new())),
            secret_getter: None,
            app_cache: None,
            is_custom_default: true,
        }
    }

    pub fn default_engine(&self) -> Arc<dyn TranslateEngine> {
        self.default_engine
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    /// 获取不带 Fallback 降级包装的原生引擎实例。
    pub async fn get_raw_engine(&self, engine_id: &str) -> Arc<dyn TranslateEngine> {
        let id_clean = engine_id.trim().to_lowercase();
        if id_clean.is_empty() || id_clean == "google" || id_clean == "google_web" {
            return self.default_engine();
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
            return self.default_engine();
        }

        // 1. 尝试读锁快速获取
        {
            let map = self.cache.read().await;
            if let Some(engine) = map.get(&id_clean) {
                return engine.clone();
            }
        }

        // 2. 先在锁外安全执行阻塞凭据读取和引擎构建
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
        let fallback = self.default_engine();
        let custom_getter = self.secret_getter.clone();
        let app_cache = self.app_cache.clone();

        tokio::task::spawn_blocking(move || {
            let (proxy_url, no_proxy) = Self::resolve_proxy_from_cache(app_cache.as_ref());

            let get_key = |key_name: &str| -> String {
                if let Some(ref getter) = custom_getter {
                    return getter(key_name).unwrap_or_default();
                }
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
                    Arc::new(DeepLEngine::new_with_proxy(
                        key,
                        proxy_url.as_deref(),
                        no_proxy,
                    )) as Arc<dyn TranslateEngine>
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
                        let mut engine = OpenAIEngine::new_with_proxy(
                            key,
                            Some(ds_url),
                            Some(ds_model.clone()),
                            proxy_url.as_deref(),
                            no_proxy,
                        )
                        .with_style(style);
                        engine.set_engine_id(format!("deepseek:{ds_model}"));
                        Arc::new(engine)
                    } else {
                        Arc::new(
                            OpenAIEngine::new_with_proxy(
                                key,
                                base_url,
                                model,
                                proxy_url.as_deref(),
                                no_proxy,
                            )
                            .with_style(style),
                        )
                    };
                    llm
                }
                _ => fallback,
            }
        })
        .await
        .unwrap_or_else(|_| self.default_engine())
    }

    async fn create_engine_blocking(&self, engine_id: &str) -> Arc<dyn TranslateEngine> {
        let raw = self.get_raw_engine(engine_id).await;
        let id_clean = engine_id.trim().to_lowercase();
        if id_clean == "deepl" || id_clean == "openai" || id_clean == "deepseek" {
            Arc::new(FallbackEngine::new(raw, self.default_engine()))
        } else {
            raw
        }
    }

    /// 代理配置发生变更时，重新加载代理策略并重置全部引擎连接池缓存
    pub async fn reload_proxy(&self) {
        if !self.is_custom_default {
            let proxy = Self::resolve_proxy_from_cache(self.app_cache.as_ref());
            let new_default = Self::create_default_google(&proxy);
            if let Ok(mut def) = self.default_engine.write() {
                *def = new_default;
            }
        }
        let mut map = self.cache.write().await;
        map.clear();
        let mut raw_map = self.raw_cache.write().await;
        raw_map.clear();
    }

    /// 密钥更新或删除时，使对应引擎缓存失效。
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
            let translated = if req.text.contains("⟦T_") {
                req.text
                    .replace("Please visit ", "请访问 ")
                    .replace(" and check ", " 并查阅 ")
                    .replace(" before proceeding.", " 后继续操作。")
                    .replace("Example Official Site", "示例官方站点")
                    .replace("Crucial Document", "重要文档")
            } else {
                format!("[E2E译文] {}", req.text)
            };

            Ok(TranslationResponse {
                text: translated,
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

    #[tokio::test]
    async fn test_reload_proxy_clears_cache_and_rebuilds_default() {
        let pool = EnginePool::new();
        let _ = pool.get_engine("deepl").await;
        assert_eq!(pool.cache.read().await.len(), 1);

        pool.reload_proxy().await;
        assert_eq!(pool.cache.read().await.len(), 0);
    }
}
