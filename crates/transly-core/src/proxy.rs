use serde::{Deserialize, Serialize};

/// 代理模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    /// 默认自动：优先读取环境代理，若无则嗅探系统代理，否则直连
    #[default]
    Auto,
    /// 系统代理：强制使用操作系统系统代理配置（若无则直连）
    System,
    /// 手动配置：使用指定的代理地址
    Manual,
    /// 不使用代理：强制直连，忽略环境与系统代理
    None,
}

impl ProxyMode {
    pub fn parse_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "system" => Self::System,
            "manual" => Self::Manual,
            "none" | "direct" => Self::None,
            _ => Self::Auto,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::System => "system",
            Self::Manual => "manual",
            Self::None => "none",
        }
    }
}

/// 代理配置
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    #[serde(default)]
    pub url: Option<String>,
}

/// 规整化代理地址
///
/// 若用户未显式书写协议前缀（如输入 `127.0.0.1:7890`），自动补全为 `http://127.0.0.1:7890`。
/// 过滤空白字符；若输入纯空则返回 `None`。
pub fn normalize_proxy_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("socks5://")
        || lower.starts_with("socks5h://")
    {
        Some(trimmed.to_string())
    } else {
        Some(format!("http://{trimmed}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_mode_parsing_and_serialization() {
        assert_eq!(ProxyMode::parse_str("auto"), ProxyMode::Auto);
        assert_eq!(ProxyMode::parse_str("AUTO"), ProxyMode::Auto);
        assert_eq!(ProxyMode::parse_str("system"), ProxyMode::System);
        assert_eq!(ProxyMode::parse_str("manual"), ProxyMode::Manual);
        assert_eq!(ProxyMode::parse_str("none"), ProxyMode::None);
        assert_eq!(ProxyMode::parse_str("direct"), ProxyMode::None);
        assert_eq!(ProxyMode::parse_str("invalid"), ProxyMode::Auto);

        assert_eq!(ProxyMode::Auto.as_str(), "auto");
        assert_eq!(ProxyMode::System.as_str(), "system");
        assert_eq!(ProxyMode::Manual.as_str(), "manual");
        assert_eq!(ProxyMode::None.as_str(), "none");
    }

    #[test]
    fn test_normalize_proxy_url() {
        assert_eq!(normalize_proxy_url(""), None);
        assert_eq!(normalize_proxy_url("   "), None);
        assert_eq!(
            normalize_proxy_url("127.0.0.1:7890"),
            Some("http://127.0.0.1:7890".to_string())
        );
        assert_eq!(
            normalize_proxy_url("http://127.0.0.1:7890"),
            Some("http://127.0.0.1:7890".to_string())
        );
        assert_eq!(
            normalize_proxy_url("https://proxy.example.com:8443"),
            Some("https://proxy.example.com:8443".to_string())
        );
        assert_eq!(
            normalize_proxy_url("socks5://127.0.0.1:1080"),
            Some("socks5://127.0.0.1:1080".to_string())
        );
    }
}
