use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub local_server_port: u16,
    pub cache_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            local_server_port: 39281,
            cache_enabled: true,
        }
    }
}

impl AppConfig {
    pub fn new() -> Self {
        Self::default()
    }
}
