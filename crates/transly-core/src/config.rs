use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub default_source_lang: String,
    pub default_target_lang: String,
    pub primary_engine: String,
    pub fallback_engine: Option<String>,
    pub local_server_port: u16,
    pub cache_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_source_lang: "auto".to_string(),
            default_target_lang: "zh-CN".to_string(),
            primary_engine: "google".to_string(),
            fallback_engine: None,
            local_server_port: 39281,
            cache_enabled: true,
        }
    }
}
