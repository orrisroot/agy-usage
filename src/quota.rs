use crate::config::get_active_account_tokens;
use crate::google_api::{
    ModelInfo, RetrieveUserQuotaSummaryResponse, fetch_available_models, get_valid_tokens,
    load_code_assist, resolve_project_id, retrieve_user_quota_summary,
};
use chrono::{DateTime, Utc};
use unicode_width::UnicodeWidthStr;

pub struct QuotaOptions {
    pub all_models: bool,
    pub json: bool,
    pub account: Option<String>,
    pub debug: bool,
}

pub async fn run_quota(options: QuotaOptions) -> Result<(), Box<dyn std::error::Error>> {
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
    let access_token = get_valid_tokens(&mut tokens, options.debug).await?;

    println!("Fetching quota information from Google API...");
    let code_assist = load_code_assist(&access_token, options.debug).await?;

    // Extract project ID if it changed or was resolved
    let project_id =
        resolve_project_id(&access_token, tokens.project_id.as_deref(), options.debug).await;
    if project_id != tokens.project_id {
        tokens.project_id = project_id.clone();
        crate::config::save_account_tokens(&tokens.email, &tokens)?;
    }

    let models_resp =
        match fetch_available_models(&access_token, project_id.as_deref(), options.debug).await {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("Warning: Failed to fetch available models quota ({})", e);
                None
            }
        };

    let quota_summary_resp = match retrieve_user_quota_summary(
        &access_token,
        project_id.as_deref(),
        options.debug,
    )
    .await
    {
        Ok(qs) => Some(qs),
        Err(e) => {
            eprintln!("Warning: Failed to fetch user quota summary ({})", e);
            None
        }
    };

    if options.json {
        print_json(
            &tokens.email,
            &code_assist,
            &models_resp,
            &quota_summary_resp,
        );
    } else {
        print_pretty(
            &tokens.email,
            &code_assist,
            &models_resp,
            &quota_summary_resp,
            options.all_models,
        );
    }

    Ok(())
}

fn should_show_model(model_id: &str, model: &ModelInfo, all_models: bool) -> bool {
    if model_id.starts_with("chat_") || model_id.starts_with("tab_") {
        return false;
    }
    if model_id.contains("image") {
        return false;
    }
    if model_id.starts_with("rev") {
        return false;
    }
    if model_id.contains("mquery") || model_id.contains("lite") {
        return false;
    }
    if model.quota_info.is_none() {
        return false;
    }

    let is_autocomplete = model_id.contains("gemini-2.5")
        || model
            .display_name
            .as_ref()
            .map(|n| n.contains("Gemini 2.5"))
            .unwrap_or(false);

    if !all_models && is_autocomplete {
        return false;
    }

    true
}

fn format_time_until_reset(reset_time_str: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(reset_time_str) {
        let diff = dt.with_timezone(&Utc).signed_duration_since(Utc::now());
        let sec = diff.num_seconds();
        if sec <= 0 {
            return "Resets soon".to_string();
        }
        let hours = sec / 3600;
        let mins = (sec % 3600) / 60;
        if hours >= 24 {
            let days = hours / 24;
            let hours = hours % 24;
            format!("{}d {}h {}m", days, hours, mins)
        } else if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}m", mins)
        }
    } else {
        reset_time_str.to_string()
    }
}

fn format_remaining(fraction: Option<f64>, is_exhausted: bool) -> String {
    if is_exhausted {
        return "\x1b[31;1m❌ EXHAUSTED\x1b[0m".to_string();
    }
    match fraction {
        Some(f) => {
            let pct = (f * 100.0).round() as i32;
            if pct >= 75 {
                format!("\x1b[32m🟢 {}%\x1b[0m", pct)
            } else if pct >= 50 {
                format!("\x1b[33m🟡 {}%\x1b[0m", pct)
            } else if pct >= 25 {
                format!("\x1b[38;5;208m🟠 {}%\x1b[0m", pct)
            } else {
                format!("\x1b[31m🔴 {}%\x1b[0m", pct)
            }
        }
        None => "N/A".to_string(),
    }
}

fn strip_ansi_codes(s: &str) -> String {
    let mut clean = String::new();
    let mut in_escape = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if c == 'm' {
                in_escape = false;
            }
            continue;
        }
        clean.push(c);
    }
    clean
}

fn print_border_line(left: char, mid: char, right: char, widths: &[usize]) {
    print!("{}", left);
    for (i, w) in widths.iter().enumerate() {
        print!("{}", "─".repeat(w + 2));
        if i < widths.len() - 1 {
            print!("{}", mid);
        }
    }
    println!("{}", right);
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }

    let mut widths = vec![0; headers.len()];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = h.width();
    }
    for row in rows {
        for (i, val) in row.iter().enumerate() {
            let clean_len = strip_ansi_codes(val).width();
            if clean_len > widths[i] {
                widths[i] = clean_len;
            }
        }
    }

    print_border_line('┌', '┬', '┐', &widths);

    print!("│");
    for (i, h) in headers.iter().enumerate() {
        print!(" \x1b[1;36m{:<width$}\x1b[0m │", h, width = widths[i]);
    }
    println!();

    print_border_line('├', '┼', '┤', &widths);

    for row in rows {
        print!("│");
        for (i, val) in row.iter().enumerate() {
            let clean_val = strip_ansi_codes(val);
            let padding = widths[i] - clean_val.width();
            print!(" {}{} │", val, " ".repeat(padding));
        }
        println!();
    }

    print_border_line('└', '┴', '┘', &widths);
}

fn print_pretty(
    email: &str,
    code_assist: &crate::google_api::LoadCodeAssistResponse,
    models_resp: &Option<crate::google_api::FetchAvailableModelsResponse>,
    quota_summary_resp: &Option<RetrieveUserQuotaSummaryResponse>,
    all_models: bool,
) {
    let now_str = Utc::now()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    println!("\n\x1b[1;34m📊 Antigravity Quota Status\x1b[0m");
    println!("   Retrieved: {}", now_str);
    println!("   Active Account: \x1b[1m{}\x1b[0m", email);

    if let Some(ref plan) = code_assist.plan_info {
        if let Some(ref ptype) = plan.plan_type {
            println!("   Plan Type: \x1b[35m{}\x1b[0m", ptype);
        }
    }

    if let Some(avail) = code_assist.available_prompt_credits {
        if let Some(ref plan) = code_assist.plan_info {
            if let Some(monthly) = plan.monthly_prompt_credits {
                println!(
                    "   Prompt Credits: \x1b[32m{}\x1b[0m / {} ({} remaining)",
                    avail,
                    monthly,
                    format_remaining(Some(avail as f64 / monthly as f64), false)
                );
            }
        }
    }
    println!();

    let mut rows = vec![];
    if let Some(resp) = models_resp {
        if let Some(models) = &resp.models {
            let mut sorted_models: Vec<(&String, &ModelInfo)> = models.iter().collect();
            sorted_models.sort_by(|a, b| {
                let name_a =
                    a.1.display_name
                        .as_ref()
                        .or(a.1.label.as_ref())
                        .unwrap_or(a.0);
                let name_b =
                    b.1.display_name
                        .as_ref()
                        .or(b.1.label.as_ref())
                        .unwrap_or(b.0);
                name_a.cmp(name_b)
            });

            let mut seen_names = std::collections::HashSet::new();
            for (model_id, info) in sorted_models {
                if should_show_model(model_id, info, all_models) {
                    let name = info
                        .display_name
                        .as_ref()
                        .or(info.label.as_ref())
                        .cloned()
                        .unwrap_or_else(|| model_id.clone());

                    if !seen_names.insert(name.clone()) {
                        continue;
                    }

                    let quota = info.quota_info.as_ref().unwrap();
                    let rem_pct = format_remaining(
                        quota.remaining_fraction,
                        quota.is_exhausted.unwrap_or(false),
                    );
                    let reset_in = quota
                        .reset_time
                        .as_ref()
                        .map(|t| format_time_until_reset(t))
                        .unwrap_or_else(|| "N/A".to_string());

                    rows.push(vec![name, rem_pct, reset_in]);
                }
            }
        }
    }

    if !rows.is_empty() {
        println!("\x1b[1;36m📋 Model Quotas\x1b[0m");
        print_table(&["Model", "Remaining %", "Reset In"], &rows);
    } else {
        println!("No model quota information available.");
    }

    if let Some(summary) = quota_summary_resp {
        // 1. Display individual buckets if they exist
        let mut quota_rows = vec![];
        if let Some(buckets) = &summary.buckets {
            for bucket in buckets {
                let name = bucket
                    .display_name
                    .as_deref()
                    .or(bucket.bucket_id.as_deref())
                    .unwrap_or("Unknown Bucket");

                let rem_pct = if bucket.disabled == Some(true) {
                    "\x1b[32m🟢 Unlimited\x1b[0m".to_string()
                } else {
                    let is_exhausted = bucket.remaining_fraction.map(|f| f <= 0.0).unwrap_or(false);
                    format_remaining(bucket.remaining_fraction, is_exhausted)
                };

                let reset_in = bucket
                    .reset_time
                    .as_ref()
                    .map(|t| format_time_until_reset(t))
                    .unwrap_or_else(|| "N/A".to_string());

                quota_rows.push(vec![name.to_string(), rem_pct, reset_in]);
            }
        }

        if !quota_rows.is_empty() {
            println!("\n\x1b[1;36m📋 User Quota Summary (Buckets)\x1b[0m");
            print_table(&["Quota Bucket", "Remaining %", "Reset In"], &quota_rows);
        }

        // 2. Display groups if they exist
        if let Some(groups) = &summary.groups {
            for group in groups {
                let group_name = group.display_name.as_deref().unwrap_or("Unnamed Group");
                let group_desc = group.description.as_deref().unwrap_or("");

                let cleaned_desc = if group_desc.starts_with("Models within this group: ") {
                    &group_desc["Models within this group: ".len()..]
                } else {
                    group_desc
                };

                if cleaned_desc.is_empty() {
                    println!("\n\x1b[1;36m👥 Group: {}\x1b[0m", group_name);
                } else {
                    println!(
                        "\n\x1b[1;36m👥 Group: {}\x1b[0m - {}",
                        group_name, cleaned_desc
                    );
                }

                let mut group_rows = vec![];
                if let Some(buckets) = &group.buckets {
                    for bucket in buckets {
                        let name = bucket
                            .display_name
                            .as_deref()
                            .or(bucket.bucket_id.as_deref())
                            .unwrap_or("Unknown Bucket");

                        let rem_pct = if bucket.disabled == Some(true) {
                            "\x1b[32m🟢 Unlimited\x1b[0m".to_string()
                        } else {
                            let is_exhausted =
                                bucket.remaining_fraction.map(|f| f <= 0.0).unwrap_or(false);
                            format_remaining(bucket.remaining_fraction, is_exhausted)
                        };

                        let reset_in = bucket
                            .reset_time
                            .as_ref()
                            .map(|t| format_time_until_reset(t))
                            .unwrap_or_else(|| "N/A".to_string());

                        group_rows.push(vec![name.to_string(), rem_pct, reset_in]);
                    }
                }

                if !group_rows.is_empty() {
                    print_table(&["Quota Bucket", "Remaining %", "Reset In"], &group_rows);
                }
            }
        }
    }

    if let Some(summary) = quota_summary_resp {
        if let Some(description) = &summary.description {
            if !description.is_empty() {
                println!("\nQuota Description:\n{}", description);
            }
        }
    }
}

fn print_json(
    email: &str,
    code_assist: &crate::google_api::LoadCodeAssistResponse,
    models_resp: &Option<crate::google_api::FetchAvailableModelsResponse>,
    quota_summary_resp: &Option<RetrieveUserQuotaSummaryResponse>,
) {
    #[derive(serde::Serialize)]
    struct JsonOutput<'a> {
        email: &'a str,
        timestamp: String,
        prompt_credits: Option<serde_json::Value>,
        models: Option<serde_json::Value>,
        quota_summary: Option<serde_json::Value>,
    }

    let out = JsonOutput {
        email,
        timestamp: Utc::now().to_rfc3339(),
        prompt_credits: serde_json::to_value(code_assist).ok(),
        models: models_resp
            .as_ref()
            .and_then(|r| serde_json::to_value(r).ok()),
        quota_summary: quota_summary_resp
            .as_ref()
            .and_then(|r| serde_json::to_value(r).ok()),
    };

    if let Ok(json_str) = serde_json::to_string_pretty(&out) {
        println!("{}", json_str);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_format_time_until_reset() {
        let now = Utc::now();

        // 1. Resets soon
        let past = now - Duration::seconds(10);
        assert_eq!(format_time_until_reset(&past.to_rfc3339()), "Resets soon");

        // 2. Under a minute -> 0m
        let seconds_30 = now + Duration::seconds(30);
        assert_eq!(format_time_until_reset(&seconds_30.to_rfc3339()), "0m");

        // 3. 2 minutes
        let mins_2 = now + Duration::seconds(125);
        assert_eq!(format_time_until_reset(&mins_2.to_rfc3339()), "2m");

        // 4. 1h 1m
        let hours_1_mins_1 = now + Duration::hours(1) + Duration::minutes(1) + Duration::seconds(5);
        assert_eq!(
            format_time_until_reset(&hours_1_mins_1.to_rfc3339()),
            "1h 1m"
        );

        // 5. 24h (1d 0h 0m)
        let hours_24 = now + Duration::hours(24) + Duration::seconds(5);
        assert_eq!(format_time_until_reset(&hours_24.to_rfc3339()), "1d 0h 0m");

        // 6. 25h 1m (1d 1h 1m)
        let hours_25_mins_1 =
            now + Duration::hours(25) + Duration::minutes(1) + Duration::seconds(5);
        assert_eq!(
            format_time_until_reset(&hours_25_mins_1.to_rfc3339()),
            "1d 1h 1m"
        );

        // 7. 49h (2d 1h 0m)
        let hours_49 = now + Duration::hours(49) + Duration::seconds(5);
        assert_eq!(format_time_until_reset(&hours_49.to_rfc3339()), "2d 1h 0m");
    }
}
