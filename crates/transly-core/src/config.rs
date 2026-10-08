use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub local_server_port: u16,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            local_server_port: 39281,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_default() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.local_server_port, 39281);
    }
}
