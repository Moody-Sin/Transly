use keyring::Entry;
use rand::RngCore;

const SERVICE_NAME: &str = "com.transly.app";

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

    // 使用系统密码学安全随机数生成 32 字节 (256-bit) 强随机令牌
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let mut token = String::with_capacity(64);
    for b in bytes {
        token.push_str(&format!("{:02x}", b));
    }

    set_secret(SERVER_TOKEN_KEY, &token)?;
    Ok(token)
}

pub fn rotate_server_token() -> anyhow::Result<String> {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let mut token = String::with_capacity(64);
    for b in bytes {
        token.push_str(&format!("{:02x}", b));
    }

    set_secret(SERVER_TOKEN_KEY, &token)?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_persistence_and_token_stability() {
        let test_key = "test_persistence_key_v2";
        let test_val = "test_secret_654321";

        // 强断言测试：写入必须成功并正确读回，不可静默吞掉异常
        set_secret(test_key, test_val).expect("set_secret 必须成功持久化至系统安全凭据管理器");
        let read_val = get_secret(test_key).expect("get_secret 必须成功读取已保存的凭据");
        assert_eq!(read_val, test_val);

        // 验证 Token 连续读取的稳定一致性
        let token1 = get_or_create_server_token().expect("首次获取 token 必须成功");
        let token2 = get_or_create_server_token().expect("二次获取 token 必须成功");
        assert_eq!(token1, token2, "Token 连续读取必须完全一致稳定");

        let _ = delete_secret(test_key);
    }
}
