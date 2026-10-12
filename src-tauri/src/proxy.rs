//! 平台代理探测与解析模块
//!
//! 支持四种代理模式：
//! 1. `auto` (默认自动): 优先检测环境变量代理，若无则自动读取系统代理（Windows 注册表等），否则直连；
//! 2. `system` (系统代理): 强制读取操作系统代理配置（Windows 注册表 Internet Settings 等）；
//! 3. `manual` (手动配置): 使用用户指定的自定义代理地址（如 `http://127.0.0.1:7890`）；
//! 4. `none` (不使用代理): 强制直连，明确忽略环境变量与系统代理。

use transly_core::{normalize_proxy_url, ProxyConfig, ProxyMode};

/// 检查环境变量代理
pub fn detect_env_proxy() -> Option<String> {
    for key in [
        "ALL_PROXY",
        "all_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Ok(val) = std::env::var(key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                if let Some(normalized) = normalize_proxy_url(trimmed) {
                    return Some(normalized);
                }
            }
        }
    }
    None
}

/// 解析 Windows 注册表中的 ProxyServer 字符串
pub fn parse_windows_proxy_server(server_str: &str) -> Option<String> {
    let s = server_str.trim();
    if s.is_empty() {
        return None;
    }

    // Windows ProxyServer 支持两种格式：
    // 1. 单一地址：`127.0.0.1:7890`
    // 2. 协议分段：`http=127.0.0.1:7890;https=127.0.0.1:7890;ftp=...` 或 `socks=127.0.0.1:1080`
    if s.contains('=') {
        // 优先匹配 https 协议分段
        for part in s.split(';') {
            let part = part.trim();
            if let Some(https) = part.strip_prefix("https=") {
                return normalize_proxy_url(https);
            }
        }
        // 其次匹配 http 协议分段
        for part in s.split(';') {
            let part = part.trim();
            if let Some(http) = part.strip_prefix("http=") {
                return normalize_proxy_url(http);
            }
        }
        // 匹配 socks 协议分段
        for part in s.split(';') {
            let part = part.trim();
            if let Some(socks) = part.strip_prefix("socks=") {
                let trimmed_socks = socks.trim();
                let with_scheme = if trimmed_socks.to_lowercase().starts_with("socks5://")
                    || trimmed_socks.to_lowercase().starts_with("socks5h://")
                {
                    trimmed_socks.to_string()
                } else {
                    format!("socks5://{trimmed_socks}")
                };
                return normalize_proxy_url(&with_scheme);
            }
        }
    }

    normalize_proxy_url(s)
}

/// 探测当前操作系统的系统代理配置
pub fn detect_system_proxy() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        detect_windows_system_proxy()
    }
    #[cfg(not(target_os = "windows"))]
    {
        detect_env_proxy()
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_system_proxy() -> Option<String> {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
        REG_SZ,
    };

    unsafe {
        let mut hkey: HKEY = std::mem::zeroed();
        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings");
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey, Some(0), KEY_READ, &mut hkey).is_err() {
            return None;
        }

        // 1. 检查 ProxyEnable 是否开启 (必须为 1)
        let mut proxy_enable: u32 = 0;
        let mut enable_type = REG_DWORD;
        let mut enable_size = std::mem::size_of::<u32>() as u32;
        let query_enable = RegQueryValueExW(
            hkey,
            w!("ProxyEnable"),
            None,
            Some(&mut enable_type),
            Some(&mut proxy_enable as *mut u32 as *mut u8),
            Some(&mut enable_size),
        );

        if query_enable.is_err() || proxy_enable != 1 {
            let _ = RegCloseKey(hkey);
            return None;
        }

        // 2. 查询 ProxyServer 字符串字节长度
        let mut server_size: u32 = 0;
        let mut server_type = REG_SZ;
        if RegQueryValueExW(
            hkey,
            w!("ProxyServer"),
            None,
            Some(&mut server_type),
            None,
            Some(&mut server_size),
        )
        .is_err()
            || server_size == 0
        {
            let _ = RegCloseKey(hkey);
            return None;
        }

        // 3. 读取 ProxyServer 实际内容
        let u16_len = (server_size as usize / 2) + 1;
        let mut buffer: Vec<u16> = vec![0; u16_len];
        if RegQueryValueExW(
            hkey,
            w!("ProxyServer"),
            None,
            Some(&mut server_type),
            Some(buffer.as_mut_ptr() as *mut u8),
            Some(&mut server_size),
        )
        .is_err()
        {
            let _ = RegCloseKey(hkey);
            return None;
        }

        let _ = RegCloseKey(hkey);

        let text = String::from_utf16_lossy(&buffer);
        let clean = text.trim_matches('\0').trim();
        parse_windows_proxy_server(clean)
    }
}

/// 根据用户设置解析出最终实际生效的代理参数: (proxy_url, no_proxy)
///
/// - `proxy_url`: 待配置给 HTTP 客户端的完整代理 URL（若有）
/// - `no_proxy`: 若为 true，代表强制直连（禁用全部代理）
pub fn resolve_effective_proxy(config: &ProxyConfig) -> (Option<String>, bool) {
    match config.mode {
        ProxyMode::None => (None, true),
        ProxyMode::Manual => {
            let url = config.url.as_deref().and_then(normalize_proxy_url);
            (url, false)
        }
        ProxyMode::System => {
            let url = detect_system_proxy();
            (url, false)
        }
        ProxyMode::Auto => {
            if let Some(env_proxy) = detect_env_proxy() {
                (Some(env_proxy), false)
            } else if let Some(sys_proxy) = detect_system_proxy() {
                (Some(sys_proxy), false)
            } else {
                (None, false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_windows_proxy_server_single_host_port() {
        assert_eq!(
            parse_windows_proxy_server("127.0.0.1:7890"),
            Some("http://127.0.0.1:7890".to_string())
        );
        assert_eq!(
            parse_windows_proxy_server("http://127.0.0.1:7890"),
            Some("http://127.0.0.1:7890".to_string())
        );
    }

    #[test]
    fn test_parse_windows_proxy_server_protocol_segments() {
        let multi = "ftp=127.0.0.1:21;http=127.0.0.1:8080;https=127.0.0.1:8443";
        // 优先匹配 https
        assert_eq!(
            parse_windows_proxy_server(multi),
            Some("http://127.0.0.1:8443".to_string())
        );

        let http_only = "ftp=127.0.0.1:21;http=127.0.0.1:8080";
        assert_eq!(
            parse_windows_proxy_server(http_only),
            Some("http://127.0.0.1:8080".to_string())
        );

        let socks_only = "socks=127.0.0.1:1080";
        assert_eq!(
            parse_windows_proxy_server(socks_only),
            Some("socks5://127.0.0.1:1080".to_string())
        );
    }

    #[test]
    fn test_parse_windows_proxy_server_empty_and_whitespace() {
        assert_eq!(parse_windows_proxy_server(""), None);
        assert_eq!(parse_windows_proxy_server("   "), None);
    }

    #[test]
    fn test_resolve_effective_proxy_none_mode() {
        let config = ProxyConfig {
            mode: ProxyMode::None,
            url: Some("http://127.0.0.1:7890".to_string()),
        };
        let (url, no_proxy) = resolve_effective_proxy(&config);
        assert!(no_proxy);
        assert_eq!(url, None);
    }

    #[test]
    fn test_resolve_effective_proxy_manual_mode() {
        let config = ProxyConfig {
            mode: ProxyMode::Manual,
            url: Some("127.0.0.1:7890".to_string()),
        };
        let (url, no_proxy) = resolve_effective_proxy(&config);
        assert!(!no_proxy);
        assert_eq!(url, Some("http://127.0.0.1:7890".to_string()));

        let invalid_config = ProxyConfig {
            mode: ProxyMode::Manual,
            url: Some("".to_string()),
        };
        let (url_inv, no_proxy_inv) = resolve_effective_proxy(&invalid_config);
        assert!(!no_proxy_inv);
        assert_eq!(url_inv, None);
    }

    /// 系统代理探测的**契约测试**（环境无关）。
    ///
    /// 回归：该用例曾硬编码 `assert_eq!(sys, Some("http://127.0.0.1:7890"))`，
    /// 假设当前机器一定开着 127.0.0.1:7890 代理，导致 GitHub Actions
    /// `windows-latest` runner（无系统代理，返回 None）直接 panic 失败。
    /// 未开启代理时返回 None 是完全合法的，这里只断言返回值的**形状**。
    #[test]
    fn test_detect_system_proxy_on_current_machine() {
        #[cfg(target_os = "windows")]
        {
            // 未配置系统代理时返回 None 是合法结果（CI runner 即如此），
            // 故只在 Some 分支断言返回值的形状。
            if let Some(url) = detect_windows_system_proxy() {
                assert!(!url.is_empty(), "探测到的代理地址不得为空字符串");
                assert!(
                    url.starts_with("http://")
                        || url.starts_with("https://")
                        || url.starts_with("socks5://"),
                    "代理地址必须带受支持的协议前缀，实际 {url}"
                );
            }
        }
    }
}
