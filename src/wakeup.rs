use crate::config::get_active_account_tokens;
use crate::google_api::{
    TriggerOptions, fetch_available_models, get_valid_tokens, resolve_project_id, trigger_model,
};
use std::collections::HashSet;

pub struct WakeupOptions {
    pub models: Option<Vec<String>>,
    pub prompt: Option<String>,
    pub account: Option<String>,
    pub keep_system_prompt: bool,
}

pub async fn run_wakeup(options: WakeupOptions) -> Result<(), Box<dyn std::error::Error>> {
    let mut tokens = if let Some(ref email) = options.account {
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
    let access_token = get_valid_tokens(&mut tokens).await?;

    // Extract project ID if it changed or was resolved
    let project_id = resolve_project_id(&access_token, tokens.project_id.as_deref()).await;
    if project_id != tokens.project_id {
        tokens.project_id = project_id.clone();
        crate::config::save_account_tokens(&tokens.email, &tokens)?;
    }

    println!("Fetching available models to verify...");
    let models_resp = match fetch_available_models(&access_token, project_id.as_deref()).await {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!("Warning: Failed to fetch available models quota ({})", e);
            None
        }
    };

    // Determine target models
    let default_models = vec![
        "gpt-oss-120b-medium".to_string(),
        "gemini-3.5-flash-extra-low".to_string(),
    ];
    let target_models = options.models.unwrap_or(default_models);

    // Verify models if possible
    let mut available_model_ids = HashSet::new();
    if let Some(ref resp) = models_resp {
        if let Some(ref models_map) = resp.models {
            for k in models_map.keys() {
                available_model_ids.insert(k.clone());
            }
        }
    }

    let mut validated_models = Vec::new();
    for model in target_models {
        if !available_model_ids.is_empty() && !available_model_ids.contains(&model) {
            println!(
                "\x1b[33mWarning:\x1b[0m Model \"{}\" was not found in the available models list.",
                model
            );
            println!("Do you still want to proceed triggering it? [y/N]");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();
            if input == "y" || input == "yes" {
                validated_models.push(model);
            } else {
                println!("Skipping model \"{}\".", model);
            }
        } else {
            validated_models.push(model);
        }
    }

    if validated_models.is_empty() {
        println!("No models to trigger.");
        return Ok(());
    }

    // Default trigger prompt is a single character for extreme token saving
    let trigger_prompt = options.prompt.unwrap_or_else(|| ".".to_string());

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

        let trigger_opts = TriggerOptions {
            model: model_id.clone(),
            prompt: trigger_prompt.clone(),
            max_output_tokens: Some(1), // 1 token completion limit
            use_system_instruction: options.keep_system_prompt,
            project_id: project_id.clone(),
        };

        match trigger_model(&access_token, &trigger_opts).await {
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

    println!("\n✨ Wakeup/Trigger cycle complete.");
    Ok(())
}
