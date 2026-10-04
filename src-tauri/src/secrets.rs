use keyring::Entry;

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

    let token = format!(
        "{:x}{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id()
    );

    let _ = set_secret(SERVER_TOKEN_KEY, &token);
    Ok(token)
}
