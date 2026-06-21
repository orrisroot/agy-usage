use crate::config::get_active_account_tokens;
use crate::google_api::{ApiClient, TriggerOptions};
use std::collections::HashMap;

#[derive(serde::Serialize, serde::Deserialize)]
struct WakeupCache {
    pub history: HashMap<String, u64>,
}
pub struct WakeupOptions {
    pub models: Option<Vec<String>>,
    pub prompt: Option<String>,
    pub account: Option<String>,
    pub keep_system_prompt: bool,
    pub debug: bool,
}

pub async fn run_wakeup(options: WakeupOptions) -> Result<(), Box<dyn std::error::Error>> {
    let tokens = if let Some(ref email) = options.account {
        match crate::config::load_account_tokens(email) {
            Some(t) => t,
            None => return Err(format!("Account {} not found. Run login first.", email).into()),
        }
    } else {
        match get_active_account_tokens() {
            Some(t) => t,
            None => {
                return Err("No active account. Please login using: agy-usage login".into());
            }
        }
    };

    println!("Checking authentication status...");
    let mut api_client = ApiClient::new(tokens, options.debug);

    // Resolve project ID (automatically handles onboarding & dirty state if needed)
    let project_id = api_client.resolve_project_id(false).await;

    // Determine target models
    let default_models = vec![
        "gpt-oss-120b-medium".to_string(),
        "gemini-3.5-flash-extra-low".to_string(),
    ];
    let validated_models = options.models.unwrap_or(default_models);

    if validated_models.is_empty() {
        println!("No models to trigger.");
        return Ok(());
    }

    // Default trigger prompt is a single character for extreme token saving
    let trigger_prompt = options.prompt.unwrap_or_else(|| ".".to_string());

    let cache_path =
        crate::config::get_account_dir(&api_client.tokens().email).join("wakeup_cache.json");
    let mut wakeup_cache = if let Ok(content) = std::fs::read_to_string(&cache_path) {
        serde_json::from_str::<WakeupCache>(&content).unwrap_or_else(|_| WakeupCache {
            history: HashMap::new(),
        })
    } else {
        WakeupCache {
            history: HashMap::new(),
        }
    };
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let skip_ttl = 5 * 60 * 1000; // 5 minutes

    println!(
        "\n🚀 Triggering {} models (extreme token saving: prompt=\"{}\", max_tokens=1, system_prompt={})...",
        validated_models.len(),
        trigger_prompt,
        if options.keep_system_prompt {
            "enabled"
        } else {
            "disabled"
        }
    );

    for model_id in validated_models {
        println!("\n⏳ Triggering {}...", model_id);

        if let Some(&last_time) = wakeup_cache.history.get(&model_id)
            && now < last_time + skip_ttl
        {
            println!("\x1b[33m⏭️  Skipped\x1b[0m (already triggered recently)");
            continue;
        }

        let trigger_opts = TriggerOptions {
            model: model_id.clone(),
            prompt: trigger_prompt.clone(),
            max_output_tokens: Some(1), // 1 token completion limit
            use_system_instruction: options.keep_system_prompt,
            project_id: project_id.clone(),
        };

        match api_client.trigger_model(&trigger_opts).await {
            Ok(result) => {
                if result.success {
                    println!("\x1b[32;1m✅ Success!\x1b[0m ({}ms)", result.duration_ms);
                    if !result.text.is_empty() {
                        println!("   Response: {:?}", result.text);
                    }
                    if let Some(ref usage) = result.token_usage {
                        println!(
                            "   Tokens Used: Prompt={}, Completion={}, Total={}",
                            usage.prompt, usage.completion, usage.total
                        );
                    }
                    wakeup_cache.history.insert(
                        model_id.clone(),
                        chrono::Utc::now().timestamp_millis() as u64,
                    );
                } else {
                    let err = result.error.unwrap_or_else(|| "Unknown error".to_string());
                    println!("\x1b[31;1m❌ Failed:\x1b[0m {}", err);
                }
            }
            Err(e) => {
                println!("\x1b[31;1m❌ Error:\x1b[0m {}", e);
            }
        }
    }

    if let Ok(content) = serde_json::to_string_pretty(&wakeup_cache)
        && let Err(e) = std::fs::write(&cache_path, content)
    {
        eprintln!("Warning: Failed to write wakeup cache: {}", e);
    }

    println!("\n✨ Wakeup/Trigger cycle complete.");
    Ok(())
}
