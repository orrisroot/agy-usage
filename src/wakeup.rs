use crate::storage::StorageProvider;
use std::collections::HashMap;

#[derive(serde::Serialize, serde::Deserialize)]
struct WakeupCache {
    pub history: HashMap<String, u64>,
}
pub struct WakeupOptions {
    pub models: Option<Vec<String>>,
    pub prompt: Option<String>,
    pub debug: bool,
}

pub async fn run_wakeup(options: WakeupOptions) -> Result<(), Box<dyn std::error::Error>> {
    let validated_models = if let Some(models) = options.models {
        models
    } else {
        let model_ids = crate::agy::fetch_models(options.debug).await?;
        select_default_models(&model_ids)
    };

    if validated_models.is_empty() {
        println!("No models to trigger.");
        return Ok(());
    }

    // Default trigger prompt is a single character for extreme token saving
    let trigger_prompt = options.prompt.unwrap_or_else(|| ".".to_string());

    let storage = crate::storage::FileStorage;
    // Keep caches isolated per agy account when the account can be determined.
    let cache_owner = crate::agy::read_account()
        .and_then(|account| account.email().map(str::to_string))
        .unwrap_or_else(|| "agy".to_string());
    let mut wakeup_cache = if let Some(content) = storage.read_cache(&cache_owner, "wakeup") {
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
        "\n🚀 Triggering {} models (prompt=\"{}\")...",
        validated_models.len(),
        trigger_prompt
    );

    let mut attempted = 0usize;
    let mut failed = 0usize;

    for model_id in validated_models {
        println!("\n⏳ Triggering {}...", model_id);

        if let Some(&last_time) = wakeup_cache.history.get(&model_id)
            && now < last_time + skip_ttl
        {
            println!("\x1b[33m⏭️  Skipped\x1b[0m (already triggered recently)");
            continue;
        }

        attempted += 1;
        match crate::agy::trigger_model(&model_id, &trigger_prompt, options.debug).await {
            Ok(result) => {
                if result.status == "SUCCESS" {
                    println!(
                        "\x1b[32;1m✅ Success!\x1b[0m ({:.2}s)",
                        result.duration_seconds
                    );
                    if !result.response.is_empty() {
                        println!("   Response: {:?}", result.response);
                    }
                    if let Some(ref usage) = result.usage {
                        println!(
                            "   Tokens Used: Input={}, Output={}, Total={}",
                            usage.input_tokens, usage.output_tokens, usage.total_tokens
                        );
                    }
                    wakeup_cache.history.insert(
                        model_id.clone(),
                        chrono::Utc::now().timestamp_millis() as u64,
                    );
                } else {
                    failed += 1;
                    let err = result.error.unwrap_or(result.response);
                    let err = err.trim();
                    println!(
                        "\x1b[31;1m❌ Failed:\x1b[0m {}",
                        if err.is_empty() { "Unknown error" } else { err }
                    );
                }
            }
            Err(e) => {
                failed += 1;
                println!("\x1b[31;1m❌ Error:\x1b[0m {}", e);
            }
        }
    }

    if let Ok(content) = serde_json::to_string_pretty(&wakeup_cache)
        && let Err(e) = storage.write_cache(&cache_owner, "wakeup", &content)
    {
        eprintln!("Warning: Failed to write wakeup cache: {}", e);
    }

    if failed > 0 {
        return Err(format!("{} of {} wakeup trigger(s) failed.", failed, attempted).into());
    }

    println!("\n✨ Wakeup/Trigger cycle complete.");
    Ok(())
}

pub fn parse_model_version(model_id: &str) -> Option<(u32, u32)> {
    if model_id.contains("argon") {
        return Some((4, 0));
    }

    // 1. Claude format: claude-sonnet-5-5-low, claude-opus-5-5, claude-sonnet-4-8, claude-3-7-sonnet
    if let Some(pos) = model_id.find("claude-") {
        let remainder = &model_id[pos + 7..];
        let parts: Vec<&str> = remainder.split('-').collect();
        for i in 0..parts.len() {
            if let Ok(major) = parts[i].parse::<u32>() {
                let minor = if i + 1 < parts.len() {
                    parts[i + 1].parse::<u32>().unwrap_or(0)
                } else {
                    0
                };
                return Some((major, minor));
            }
        }
    }

    // 2. Gemini format: gemini-3.8-..., gemini-2.5-..., gemini-3-...
    if let Some(pos) = model_id.find("gemini-") {
        let remainder = &model_id[pos + 7..];
        let mut parts = remainder.split('-');
        if let Some(ver_part) = parts.next() {
            let mut num_parts = ver_part.split('.');
            if let Some(major_str) = num_parts.next()
                && let Ok(major) = major_str.parse::<u32>()
            {
                let minor = num_parts
                    .next()
                    .and_then(|m| m.parse::<u32>().ok())
                    .unwrap_or(0);
                return Some((major, minor));
            }
        }
    }
    None
}

fn get_model_cost_score(model_id: &str) -> u32 {
    let mut score: i32 = 0;

    // 1. Model family/provider base cost
    if model_id.contains("gemini") || model_id.contains("argon") {
        if model_id.contains("flash") {
            score += 100;
        } else if model_id.contains("pro") {
            score += 200;
        } else if model_id.contains("argon") {
            score += 300;
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
    } else if model_id.contains("extra-low") {
        score += 15;
    } else if model_id.contains("low") {
        score += 20;
    } else if model_id.contains("tiered") {
        score += 25;
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

    // 3. Version preference: newer versions are preferred when several
    // selectable models are returned by `agy models`.
    if let Some((major, minor)) = parse_model_version(model_id) {
        let ver_val = major * 10 + minor.min(9);
        let discount = match ver_val {
            v if v >= 55 => 30,
            v if v >= 50 => 25,
            v if v >= 40 => 20,
            39 => 19,
            38 => 18,
            37 => 17,
            36 => 15,
            35 => 12,
            31..=34 => 8,
            30 => 6,
            25..=29 => 2,
            _ => 0,
        };
        score -= discount;
    }

    score.max(0) as u32
}

pub fn select_default_models(model_ids: &[String]) -> Vec<String> {
    let mut gemini_3x_list = Vec::new();
    let mut argon_list = Vec::new();
    let mut other_list = Vec::new();

    // `agy models` only returns currently selectable models, so no separate
    // retirement/deprecation filtering is needed here.
    for id in model_ids {
        if id.contains("argon") {
            argon_list.push(id.clone());
        } else if id.contains("gemini") {
            gemini_3x_list.push(id.clone());
        } else {
            other_list.push(id.clone());
        }
    }

    let mut selected = Vec::new();

    // 1. Cheapest conventional Gemini model (Gemini 3.x Flash / Pro)
    if !gemini_3x_list.is_empty() {
        gemini_3x_list.sort_by_key(|id| get_model_cost_score(id));
        if let Some(cheapest_gemini) = gemini_3x_list.first() {
            selected.push(cheapest_gemini.clone());
        }
    }

    // 2. Cheapest Argon model (when available, triggered alongside the conventional Gemini model)
    if !argon_list.is_empty() {
        argon_list.sort_by_key(|id| get_model_cost_score(id));
        if let Some(cheapest_argon) = argon_list.first() {
            selected.push(cheapest_argon.clone());
        }
    }

    // 3. Cheapest other model (Claude / GPT)
    if !other_list.is_empty() {
        other_list.sort_by_key(|id| get_model_cost_score(id));
        if let Some(cheapest_other) = other_list.first() {
            selected.push(cheapest_other.clone());
        }
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_default_models_uses_only_available_models() {
        let ids = vec!["gpt-oss-120b-medium".to_string()];
        let selected = select_default_models(&ids);
        assert_eq!(selected, vec!["gpt-oss-120b-medium".to_string()]);

        let ids = vec!["gemini-3.8-flash-low".to_string()];
        let selected = select_default_models(&ids);
        assert_eq!(selected, vec!["gemini-3.8-flash-low".to_string()]);
    }

    #[test]
    fn test_parse_model_version() {
        assert_eq!(parse_model_version("gemini-4-argon"), Some((4, 0)));
        assert_eq!(parse_model_version("argon"), Some((4, 0)));
        assert_eq!(parse_model_version("gemini-3.8-flash-low"), Some((3, 8)));
        assert_eq!(parse_model_version("gemini-3.7-flash-tiered"), Some((3, 7)));
        assert_eq!(parse_model_version("gemini-3.6-flash-low"), Some((3, 6)));
        assert_eq!(parse_model_version("gemini-3.5-flash-low"), Some((3, 5)));
        assert_eq!(parse_model_version("gemini-3.1-pro-low"), Some((3, 1)));
        assert_eq!(parse_model_version("gemini-3-flash"), Some((3, 0)));
        assert_eq!(parse_model_version("gemini-2.5-flash"), Some((2, 5)));
        assert_eq!(parse_model_version("claude-opus-5-5-low"), Some((5, 5)));
        assert_eq!(parse_model_version("claude-sonnet-5-5-high"), Some((5, 5)));
        assert_eq!(parse_model_version("claude-sonnet-4-8"), Some((4, 8)));
        assert_eq!(parse_model_version("claude-sonnet-4-6"), Some((4, 6)));
        assert_eq!(parse_model_version("claude-3-7-sonnet"), Some((3, 7)));
        assert_eq!(parse_model_version("gpt-oss-120b-medium"), None);
    }

    #[test]
    fn test_is_not_filtered_by_retirement_rules() {
        // `agy models` is the source of truth for availability; any returned
        // Gemini model must remain selectable regardless of its version.
        let ids = vec!["gemini-3.6-flash-low".to_string()];
        assert_eq!(
            select_default_models(&ids),
            vec!["gemini-3.6-flash-low".to_string()]
        );
    }

    #[test]
    fn test_get_model_cost_score_prefers_newer_versions() {
        let score_38_flash_low = get_model_cost_score("gemini-3.8-flash-low");
        let score_37_flash_low = get_model_cost_score("gemini-3.7-flash-low");
        let score_36_flash_low = get_model_cost_score("gemini-3.6-flash-low");

        assert!(score_38_flash_low < score_37_flash_low);
        assert!(score_37_flash_low < score_36_flash_low);

        let score_38_pro_low = get_model_cost_score("gemini-3.8-pro-low");
        let score_31_pro_low = get_model_cost_score("gemini-3.1-pro-low");
        assert!(score_38_pro_low < score_31_pro_low);

        // Flash (10x) < Pro (20x) < Argon (30x)
        let score_argon_low = get_model_cost_score("gemini-4-argon-low");
        let score_argon = get_model_cost_score("gemini-4-argon");
        let score_argon_high = get_model_cost_score("gemini-4-argon-high");

        assert!(score_38_flash_low < score_38_pro_low);
        assert!(score_38_pro_low < score_argon_low);
        assert!(score_argon_low < score_argon);
        assert!(score_argon < score_argon_high);

        // Claude 5.5 scoring
        let score_sonnet_55_low = get_model_cost_score("claude-sonnet-5-5-low");
        let score_sonnet_55_med = get_model_cost_score("claude-sonnet-5-5-medium");
        let score_opus_55_low = get_model_cost_score("claude-opus-5-5-low");
        let score_sonnet_48 = get_model_cost_score("claude-sonnet-4-8");

        assert!(score_sonnet_55_low < score_sonnet_55_med);
        assert!(score_sonnet_55_med < score_opus_55_low);
        assert!(score_sonnet_55_low < score_sonnet_48);
    }

    #[test]
    fn test_select_default_models_triggers_both_gemini_3x_and_argon() {
        // When Gemini 3.8, Argon, and Claude 5.5 are available, each category is selected
        let ids = vec![
            "gemini-4-argon-high".to_string(),
            "gemini-4-argon-low".to_string(),
            "gemini-4-argon-medium".to_string(),
            "gemini-3.8-flash-low".to_string(),
            "claude-sonnet-5-5-low".to_string(),
        ];
        let selected = select_default_models(&ids);
        assert!(selected.contains(&"gemini-3.8-flash-low".to_string()));
        assert!(selected.contains(&"gemini-4-argon-low".to_string()));
        assert!(selected.contains(&"claude-sonnet-5-5-low".to_string()));
        assert_eq!(selected.len(), 3);

        // When only Gemini 3.8 and GPT are available
        let ids_without_argon = vec![
            "gemini-3.8-flash-low".to_string(),
            "gpt-oss-120b-medium".to_string(),
        ];
        let selected_no_argon = select_default_models(&ids_without_argon);
        assert!(selected_no_argon.contains(&"gemini-3.8-flash-low".to_string()));
        assert!(selected_no_argon.contains(&"gpt-oss-120b-medium".to_string()));
        assert_eq!(selected_no_argon.len(), 2);
    }
}
