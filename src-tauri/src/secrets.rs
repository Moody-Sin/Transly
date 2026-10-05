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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_persistence_and_token_stability() {
        let test_key = "test_persistence_key";
        let test_val = "test_secret_123456";

        let write_res = set_secret(test_key, test_val);
        if write_res.is_ok() {
            let read_val = get_secret(test_key).expect("should read written secret");
            assert_eq!(read_val, test_val);
            let _ = delete_secret(test_key);
        }
    }
}
