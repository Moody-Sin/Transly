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
