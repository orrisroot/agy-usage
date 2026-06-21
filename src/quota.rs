use crate::config::get_active_account_tokens;
use crate::google_api::{ApiClient, RetrieveUserQuotaSummaryResponse};
use chrono::{DateTime, Utc};
use unicode_width::UnicodeWidthStr;

pub struct QuotaOptions {
    pub json: bool,
    pub account: Option<String>,
    pub debug: bool,
}

pub async fn run_quota(options: QuotaOptions) -> Result<(), Box<dyn std::error::Error>> {
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

    println!("Fetching quota information from Google API...");
    let code_assist = api_client.load_code_assist().await?;

    // Resolve project ID (automatically handles onboarding & dirty state if needed)
    let _project_id = api_client.resolve_project_id().await;

    let quota_summary_resp = match api_client.retrieve_user_quota_summary().await {
        Ok(qs) => Some(qs),
        Err(e) => {
            eprintln!("Warning: Failed to fetch user quota summary ({})", e);
            None
        }
    };

    if options.json {
        print_json(
            &api_client.tokens().email,
            &code_assist,
            &quota_summary_resp,
        );
    } else {
        print_pretty(
            &api_client.tokens().email,
            &code_assist,
            &quota_summary_resp,
        );
    }

    Ok(())
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
            let pct = (f * 100.0).floor() as i32;
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

/// Helper to format a single QuotaSummaryBucket into table row fields.
fn format_quota_bucket_row(bucket: &crate::google_api::QuotaSummaryBucket) -> Vec<String> {
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

    vec![name.to_string(), rem_pct, reset_in]
}



fn print_pretty(
    email: &str,
    code_assist: &crate::google_api::LoadCodeAssistResponse,
    quota_summary_resp: &Option<RetrieveUserQuotaSummaryResponse>,
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



    if let Some(summary) = quota_summary_resp {
        // 1. Display individual buckets if they exist
        let mut quota_rows = vec![];
        if let Some(buckets) = &summary.buckets {
            for bucket in buckets {
                quota_rows.push(format_quota_bucket_row(bucket));
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

                println!("\n\x1b[1;36m👥 {}\x1b[0m", group_name);
                if !group_desc.is_empty() {
                    println!("{}", group_desc);
                }

                let mut group_rows = vec![];
                if let Some(buckets) = &group.buckets {
                    for bucket in buckets {
                        group_rows.push(format_quota_bucket_row(bucket));
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
    quota_summary_resp: &Option<RetrieveUserQuotaSummaryResponse>,
) {
    #[derive(serde::Serialize)]
    struct JsonOutput<'a> {
        email: &'a str,
        timestamp: String,
        prompt_credits: Option<serde_json::Value>,
        quota_summary: Option<serde_json::Value>,
    }

    let out = JsonOutput {
        email,
        timestamp: Utc::now().to_rfc3339(),
        prompt_credits: serde_json::to_value(code_assist).ok(),
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
