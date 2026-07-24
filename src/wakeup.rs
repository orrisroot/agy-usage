use crate::config::get_active_account_tokens;
use crate::google_api::{ApiClient, TriggerOptions};
use crate::storage::StorageProvider;
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
                let model_ids = extract_model_ids(&available);
                select_default_models(&model_ids)
            }
            Err(e) => {
                println!(
                    "Warning: Failed to fetch available models ({}), using hardcoded defaults.",
                    e
                );
                vec![
                    "gpt-oss-120b-medium".to_string(),
                    "gemini-3.6-flash-low".to_string(),
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

    let storage = crate::storage::FileStorage;
    let email = api_client.tokens().email().to_string();
    let mut wakeup_cache = if let Some(content) = storage.read_cache(&email, "wakeup") {
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

    if let Ok(content) = serde_json::to_string_pretty(&wakeup_cache) {
        if let Err(e) = storage.write_cache(&email, "wakeup", &content) {
            eprintln!("Warning: Failed to write wakeup cache: {}", e);
        }
    }

    println!("\n✨ Wakeup/Trigger cycle complete.");
    Ok(())
}

fn get_model_cost_score(model_id: &str) -> u32 {
    let mut score: i32 = 0;

    // 1. Model family/provider base cost
    if model_id.contains("gemini") {
        if model_id.contains("flash") {
            score += 100;
        } else if model_id.contains("pro") {
            score += 200;
        } else {
            score += 150;
        }
    } else {
        if model_id.contains("gpt-oss") {
            score += 100;
        } else if model_id.contains("sonnet") {
            score += 300;
        } else if model_id.contains("opus") {
            score += 500;
        } else {
            score += 400;
        }
    }

    // 2. Specific tier suffix cost adjustment
    if model_id.contains("lite") {
        score += 10;
    } else if model_id.contains("low") {
        score += 20;
    } else if model_id.contains("medium") {
        score += 30;
    } else if model_id.contains("high")
        || model_id.contains("thinking")
        || model_id.contains("agent")
    {
        score += 40;
    } else {
        score += 30;
    }

    // 3. Version preference: Newer versions (3.6) are preferred over older versions (3.5)
    if model_id.contains("3.6") {
        score -= 15;
    } else if model_id.contains("3.5") {
        score -= 5;
    }

    score.max(0) as u32
}

pub fn extract_model_ids(available: &serde_json::Value) -> Vec<String> {
    let mut model_ids = Vec::new();

    // Filter out deprecated model IDs if present
    let deprecated_ids: Vec<String> = available
        .get("deprecatedModelIds")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    if let Some(sorts) = available.get("agentModelSorts").and_then(|s| s.as_array()) {
        for sort in sorts {
            if let Some(groups) = sort.get("groups").and_then(|g| g.as_array()) {
                for group in groups {
                    if let Some(ids) = group.get("modelIds").and_then(|m| m.as_array()) {
                        for val in ids {
                            if let Some(id) = val.as_str() {
                                if !deprecated_ids.contains(&id.to_string())
                                    && !model_ids.contains(&id.to_string())
                                {
                                    model_ids.push(id.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(models) = available.get("models").and_then(|m| m.as_array()) {
        for item in models {
            let id_opt = if let Some(s) = item.as_str() {
                Some(s.to_string())
            } else if let Some(obj) = item.as_object() {
                obj.get("modelId")
                    .or_else(|| obj.get("id"))
                    .or_else(|| obj.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            };
            if let Some(id) = id_opt {
                if !deprecated_ids.contains(&id) && !model_ids.contains(&id) {
                    model_ids.push(id);
                }
            }
        }
    }

    model_ids
}

pub fn select_default_models(model_ids: &[String]) -> Vec<String> {
    let mut gemini_list = Vec::new();
    let mut other_list = Vec::new();

    for id in model_ids {
        if id.contains("gemini") {
            gemini_list.push(id.clone());
        } else {
            other_list.push(id.clone());
        }
    }

    let mut selected = Vec::new();

    if !gemini_list.is_empty() {
        gemini_list.sort_by_key(|id| get_model_cost_score(id));
        if let Some(cheapest_gemini) = gemini_list.first() {
            selected.push(cheapest_gemini.clone());
        }
    } else {
        selected.push("gemini-3.6-flash-low".to_string());
    }

    if !other_list.is_empty() {
        other_list.sort_by_key(|id| get_model_cost_score(id));
        if let Some(cheapest_other) = other_list.first() {
            selected.push(cheapest_other.clone());
        }
    } else {
        selected.push("gpt-oss-120b-medium".to_string());
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_model_ids_multiple_groups() {
        let json_val = serde_json::json!({
            "agentModelSorts": [
                {
                    "sortType": "AGENT_MODEL_SORT_TYPE_RECOMMENDED",
                    "groups": [
                        {
                            "displayName": "Claude & GPT",
                            "modelIds": ["gpt-oss-120b-medium", "claude-sonnet-4-6"]
                        },
                        {
                            "displayName": "Gemini",
                            "modelIds": ["gemini-3.5-flash-low", "gemini-3.6-flash-low"]
                        }
                    ]
                }
            ]
        });

        let ids = extract_model_ids(&json_val);
        assert_eq!(
            ids,
            vec![
                "gpt-oss-120b-medium",
                "claude-sonnet-4-6",
                "gemini-3.5-flash-low",
                "gemini-3.6-flash-low"
            ]
        );

        let selected = select_default_models(&ids);
        assert!(selected.contains(&"gemini-3.6-flash-low".to_string()));
        assert!(selected.contains(&"gpt-oss-120b-medium".to_string()));
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_select_default_models_fallback_when_gemini_missing() {
        let ids = vec!["gpt-oss-120b-medium".to_string()];
        let selected = select_default_models(&ids);
        assert!(selected.contains(&"gemini-3.6-flash-low".to_string()));
        assert!(selected.contains(&"gpt-oss-120b-medium".to_string()));
        assert_eq!(selected.len(), 2);
    }
}
