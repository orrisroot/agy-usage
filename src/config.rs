use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, read_to_string, remove_dir_all};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StoredTokens {
    access_token: String,
    refresh_token: String,
    expires_at: u64, // Epoch milliseconds
    email: String,
    project_id: Option<String>,
}

impl StoredTokens {
    pub fn new(
        access_token: String,
        refresh_token: String,
        expires_at: u64,
        email: String,
        project_id: Option<String>,
    ) -> Self {
        Self {
            access_token,
            refresh_token,
            expires_at,
            email,
            project_id,
        }
    }

    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    pub fn set_access_token(&mut self, token: String) {
        self.access_token = token;
    }

    pub fn refresh_token(&self) -> &str {
        &self.refresh_token
    }

    pub fn set_refresh_token(&mut self, token: String) {
        self.refresh_token = token;
    }

    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }

    pub fn set_expires_at(&mut self, expires_at: u64) {
        self.expires_at = expires_at;
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub fn set_project_id(&mut self, id: Option<String>) {
        self.project_id = id;
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GlobalConfig {
    #[serde(rename = "activeAccount")]
    pub active_email: Option<String>,
}

pub fn get_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("agy-usage")
}

pub fn get_global_config_path() -> PathBuf {
    get_config_dir().join("config.json")
}

pub fn load_global_config() -> GlobalConfig {
    let path = get_global_config_path();
    if !path.exists() {
        return GlobalConfig::default();
    }
    match read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => GlobalConfig::default(),
    }
}

pub fn save_global_config(config: &GlobalConfig) -> Result<(), std::io::Error> {
    let path = get_global_config_path();
    let content = serde_json::to_string_pretty(config)?;
    write_atomically(&path, &content, None)?;
    Ok(())
}

fn write_atomically(
    path: &std::path::Path,
    content: &str,
    mode: Option<u32>,
) -> Result<(), std::io::Error> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Path must have a parent directory",
        )
    })?;
    create_dir_all(dir)?;

    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Path must have a file name",
        )
    })?;
    let tmp_name = format!("{}.tmp", file_name.to_string_lossy());
    let tmp_path = dir.join(tmp_name);

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        if let Some(m) = mode {
            options.mode(m);
        }
        let mut file = options.open(&tmp_path)?;
        use std::io::Write;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        use std::fs::OpenOptions;
        use std::io::Write;
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        let mut file = options.open(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }

    std::fs::rename(&tmp_path, path)?;
    Ok(())
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

pub fn load_account_tokens(email: &str) -> Option<StoredTokens> {
    let path = get_account_dir(email).join("tokens.json");
    if !path.exists() {
        return None;
    }
    let content = read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_account_tokens(email: &str, tokens: &StoredTokens) -> Result<(), std::io::Error> {
    let dir = get_account_dir(email);
    let path = dir.join("tokens.json");
    let content = serde_json::to_string_pretty(tokens)?;
    write_atomically(&path, &content, Some(0o600))?;
    Ok(())
}

pub fn get_active_account_tokens() -> Option<StoredTokens> {
    let global = load_global_config();
    let email = global.active_email?;
    load_account_tokens(&email)
}

pub fn list_accounts() -> Vec<String> {
    let accounts_dir = get_config_dir().join("accounts");
    if !accounts_dir.exists() {
        return vec![];
    }

    let mut emails = vec![];
    if let Ok(entries) = accounts_dir.read_dir() {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let tokens_path = entry.path().join("tokens.json");
                if tokens_path.exists() {
                    // Try to read email from tokens.json
                    if let Ok(content) = read_to_string(&tokens_path)
                        && let Ok(tokens) = serde_json::from_str::<StoredTokens>(&content)
                    {
                        emails.push(tokens.email().to_string());
                    }
                }
            }
        }
    }
    emails
}

pub fn remove_account(email: &str) -> Result<(), std::io::Error> {
    let dir = get_account_dir(email);
    if dir.exists() {
        remove_dir_all(dir)?;
    }

    // Update global config if this was the active email
    let mut global = load_global_config();
    if Some(email.to_string()) == global.active_email {
        global.active_email = list_accounts().first().cloned();
        save_global_config(&global)?;
    }

    Ok(())
}

#[derive(Debug)]
pub struct FileTokenStorage;

impl crate::google_api::TokenStorage for FileTokenStorage {
    fn save_tokens(
        &self,
        email: &str,
        tokens: &StoredTokens,
    ) -> Result<(), Box<dyn std::error::Error>> {
        save_account_tokens(email, tokens)?;
        Ok(())
    }

    fn read_cache(&self, email: &str, cache_key: &str) -> Option<String> {
        let cache_path = get_account_dir(email).join(format!("{}_cache.json", cache_key));
        std::fs::read_to_string(cache_path).ok()
    }

    fn write_cache(
        &self,
        email: &str,
        cache_key: &str,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cache_path = get_account_dir(email).join(format!("{}_cache.json", cache_key));
        write_atomically(&cache_path, content, None)?;
        Ok(())
    }
}
