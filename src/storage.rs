use crate::config::{StoredTokens, get_account_dir, save_account_tokens};
use std::fs::{create_dir_all, read_to_string};
use std::path::Path;

pub trait StorageProvider: std::fmt::Debug + Send + Sync + 'static {
    fn save_tokens(
        &self,
        email: &str,
        tokens: &StoredTokens,
    ) -> Result<(), Box<dyn std::error::Error>>;

    fn read_cache(&self, email: &str, cache_key: &str) -> Option<String>;

    fn write_cache(
        &self,
        email: &str,
        cache_key: &str,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

pub fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn write_atomically(
    path: &Path,
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
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        if let Some(m) = mode {
            options.mode(m);
        }
        let mut file = options.open(&tmp_path)?;
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

#[derive(Debug)]
pub struct FileStorage;

impl StorageProvider for FileStorage {
    fn save_tokens(
        &self,
        email: &str,
        tokens: &StoredTokens,
    ) -> Result<(), Box<dyn std::error::Error>> {
        save_account_tokens(email, tokens)?;
        Ok(())
    }

    fn read_cache(&self, email: &str, cache_key: &str) -> Option<String> {
        let safe_key = sanitize_key(cache_key);
        let cache_path = get_account_dir(email).join(format!("{}_cache.json", safe_key));
        read_to_string(cache_path).ok()
    }

    fn write_cache(
        &self,
        email: &str,
        cache_key: &str,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let safe_key = sanitize_key(cache_key);
        let cache_path = get_account_dir(email).join(format!("{}_cache.json", safe_key));
        write_atomically(&cache_path, content, None)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_key() {
        assert_eq!(sanitize_key("code_assist"), "code_assist");
        assert_eq!(sanitize_key("../path/traversal"), "___path_traversal");
        assert_eq!(sanitize_key("user:data"), "user_data");
    }
}
