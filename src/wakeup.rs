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
    let mut api_client = ApiClient::new(
        tokens,
        Box::new(crate::config::FileTokenStorage),
        options.debug,
    );

    // Resolve project ID (automatically handles onboarding & dirty state if needed)
    let project_id = api_client.resolve_project_id(false).await;

    // Determine target models
    let validated_models = if let Some(models) = options.models {
        models
    } else {
        println!("Resolving available models from API...");
        match api_client.fetch_available_models(false).await {
            Ok(available) => {
                let model_ids: Vec<String> = available
                    .get("agentModelSorts")
                    .and_then(|sorts| sorts.as_array())
                    .and_then(|sorts_arr| sorts_arr.first())
                    .and_then(|first_sort| first_sort.get("groups"))
                    .and_then(|groups| groups.as_array())
                    .and_then(|groups_arr| groups_arr.first())
                    .and_then(|first_group| first_group.get("modelIds"))
                    .and_then(|model_ids_val| model_ids_val.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|val| val.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                if model_ids.is_empty() {
                    vec![
                        "gpt-oss-120b-medium".to_string(),
                        "gemini-3.5-flash-extra-low".to_string(),
                    ]
                } else {
                    let mut gemini_list = Vec::new();
                    let mut other_list = Vec::new();

                    for id in model_ids {
                        if id.contains("gemini") {
                            gemini_list.push(id);
                        } else {
                            other_list.push(id);
                        }
                    }

                    let mut selected = Vec::new();
                    if !gemini_list.is_empty() {
                        gemini_list.sort_by_key(|id| get_model_cost_score(id));
                        if let Some(cheapest_gemini) = gemini_list.first() {
                            selected.push(cheapest_gemini.clone());
                        }
                    }
                    if !other_list.is_empty() {
                        other_list.sort_by_key(|id| get_model_cost_score(id));
                        if let Some(cheapest_other) = other_list.first() {
                            selected.push(cheapest_other.clone());
                        }
                    }

                    if selected.is_empty() {
                        vec![
                            "gpt-oss-120b-medium".to_string(),
                            "gemini-3.5-flash-extra-low".to_string(),
                        ]
                    } else {
                        selected
                    }
                }
            }
            Err(e) => {
                println!(
                    "Warning: Failed to fetch available models ({}), using hardcoded defaults.",
                    e
                );
                vec![
                    "gpt-oss-120b-medium".to_string(),
                    "gemini-3.5-flash-extra-low".to_string(),
                ]
            }
        }
    };

    if validated_models.is_empty() {
        println!("No models to trigger.");
        return Ok(());
    }

    // Default trigger prompt is a single character for extreme token saving
    let trigger_prompt = options.prompt.unwrap_or_else(|| ".".to_string());

    let cache_path =
        crate::config::get_account_dir(api_client.tokens().email()).join("wakeup_cache.json");
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

fn get_model_cost_score(model_id: &str) -> u32 {
    let mut score = 0;

    // 1. Model family/provider base cost
    if model_id.contains("gemini") {
        if model_id.contains("flash") {
            score += 10;
        } else if model_id.contains("pro") {
            score += 20;
        } else {
            score += 15;
        }
    } else {
        if model_id.contains("gpt-oss") {
            score += 10;
        } else if model_id.contains("sonnet") {
            score += 30;
        } else if model_id.contains("opus") {
            score += 50;
        } else {
            score += 40;
        }
    }

    // 2. Specific tier suffix cost adjustment
    if model_id.contains("extra-low") || model_id.contains("lite") {
        score += 1;
    } else if model_id.contains("low") {
        score += 2;
    } else if model_id.contains("medium") {
        score += 3;
    } else if model_id.contains("high")
        || model_id.contains("thinking")
        || model_id.contains("agent")
    {
        score += 4;
    } else {
        score += 3;
    }

    score
}
