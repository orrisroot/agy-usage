pub mod client;
pub mod sanitizer;
pub mod types;

#[allow(unused_imports)]
pub use client::*;
#[allow(unused_imports)]
pub use sanitizer::*;
#[allow(unused_imports)]
pub use types::*;

#[allow(unused_imports)]
pub use crate::storage::StorageProvider as TokenStorage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredTokens;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Default, Clone)]
    struct MockStorage {
        saved_tokens: Arc<Mutex<Option<StoredTokens>>>,
        cache: Arc<Mutex<std::collections::HashMap<String, String>>>,
    }

    impl crate::storage::StorageProvider for MockStorage {
        fn save_tokens(
            &self,
            _email: &str,
            tokens: &StoredTokens,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let mut guard = self.saved_tokens.lock().unwrap();
            *guard = Some(tokens.clone());
            Ok(())
        }

        fn read_cache(&self, _email: &str, cache_key: &str) -> Option<String> {
            let guard = self.cache.lock().unwrap();
            guard.get(cache_key).cloned()
        }

        fn write_cache(
            &self,
            _email: &str,
            cache_key: &str,
            content: &str,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let mut guard = self.cache.lock().unwrap();
            guard.insert(cache_key.to_string(), content.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_api_client_cached_load_code_assist() {
        let tokens = StoredTokens::new(
            "fake_access_token".to_string(),
            "fake_refresh_token".to_string(),
            u64::MAX, // never expires for test
            "test@example.com".to_string(),
            Some("fake-project-123".to_string()),
        );

        let storage = MockStorage::default();

        // Seed cache with mock response
        let cached_data = serde_json::json!({
            "response": {
                "availablePromptCredits": 100,
                "planInfo": { "planType": "free-tier" }
            },
            "fetched_at": u64::MAX - 1000
        });
        storage
            .cache
            .lock()
            .unwrap()
            .insert("code_assist".to_string(), cached_data.to_string());

        let mut client = ApiClient::new(tokens, Box::new(storage), false);
        let resp = client.load_code_assist(false).await.unwrap();
        assert_eq!(resp.available_prompt_credits, Some(100));
    }
}
