use keyring::Entry;
use rand::RngCore;

const SERVICE_NAME: &str = "com.transly.app";

/// 生成 32 字节 (256-bit) 密码学安全随机令牌（纯函数，便于无副作用测试）
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let mut token = String::with_capacity(64);
    for b in bytes {
        token.push_str(&format!("{:02x}", b));
    }
    token
}

pub fn set_secret(key: &str, secret: &str) -> anyhow::Result<()> {
    let entry = Entry::new(SERVICE_NAME, key)?;
    entry.set_password(secret)?;
    Ok(())
}

pub fn get_secret(key: &str) -> anyhow::Result<String> {
    let entry = Entry::new(SERVICE_NAME, key)?;
    let password = entry.get_password()?;
    Ok(password)
}

pub fn delete_secret(key: &str) -> anyhow::Result<()> {
    let entry = Entry::new(SERVICE_NAME, key)?;
    entry.delete_credential()?;
    Ok(())
}

const SERVER_TOKEN_KEY: &str = "internal_server_token";

pub fn get_or_create_server_token() -> anyhow::Result<String> {
    if let Ok(token) = get_secret(SERVER_TOKEN_KEY) {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    let token = generate_token();
    set_secret(SERVER_TOKEN_KEY, &token)?;
    Ok(token)
}

pub fn rotate_server_token() -> anyhow::Result<String> {
    let token = generate_token();
    set_secret(SERVER_TOKEN_KEY, &token)?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token_is_256_bit_hex() {
        let token = generate_token();
        assert_eq!(token.len(), 64, "32 字节应编码为 64 位十六进制字符");
        assert!(
            token.chars().all(|c| c.is_ascii_hexdigit()),
            "令牌必须为纯十六进制字符"
        );
        assert_ne!(token, generate_token(), "连续生成不得重复（OsRng 熵源）");
    }

    /// 真实系统凭据库往返测试。
    ///
    /// 默认不执行：它会在开发者与 CI 机器的 Windows Credential Manager /
    /// macOS Keychain 中真实创建条目，污染宿主环境并引入平台 flakiness。
    /// 需显式设 `TRANSLY_KEYRING_TEST=1` 才运行。
    #[test]
    fn test_secret_persistence_roundtrip() {
        if std::env::var("TRANSLY_KEYRING_TEST").as_deref() != Ok("1") {
            eprintln!("跳过：未设 TRANSLY_KEYRING_TEST=1，避免污染真实系统凭据库");
            return;
        }

        let test_key = "test_persistence_key_v2";
        let test_val = "test_secret_654321";

        set_secret(test_key, test_val).expect("set_secret 必须成功持久化至系统安全凭据管理器");
        let read_val = get_secret(test_key).expect("get_secret 必须成功读取已保存的凭据");
        assert_eq!(read_val, test_val);

        let token1 = get_or_create_server_token().expect("首次获取 token 必须成功");
        let token2 = get_or_create_server_token().expect("二次获取 token 必须成功");
        assert_eq!(token1, token2, "Token 连续读取必须完全一致稳定");

        let _ = delete_secret(test_key);
    }
}
