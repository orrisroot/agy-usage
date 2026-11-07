use std::path::PathBuf;

pub fn get_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("agy-usage")
}

pub fn get_safe_email_name(email: &str) -> String {
    email
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '@' || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn get_account_dir(email: &str) -> PathBuf {
    get_config_dir()
        .join("accounts")
        .join(get_safe_email_name(email))
}
