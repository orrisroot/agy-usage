use crate::config::get_active_account_tokens;
use crate::google_api::{ApiClient, RetrieveUserQuotaSummaryResponse};
use chrono::{DateTime, Utc};
use colored::Colorize;
use std::io::IsTerminal;
use unicode_width::UnicodeWidthStr;

pub struct QuotaOptions {
    pub json: bool,
    pub account: Option<String>,
    pub debug: bool,
    pub force: bool,
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

    eprintln!("Checking authentication status...");
    let mut api_client = ApiClient::new(
        tokens,
        Box::new(crate::config::FileTokenStorage),
        options.debug,
    );

    eprintln!("Fetching quota information from Google API...");
    let code_assist = api_client.load_code_assist(options.force).await?;

    // Resolve project ID (automatically handles onboarding & dirty state if needed)
    let _project_id = api_client.resolve_project_id(options.force).await;

    let quota_summary_resp = match api_client.retrieve_user_quota_summary(options.force).await {
        Ok(qs) => Some(qs),
        Err(e) => {
            eprintln!("Warning: Failed to fetch user quota summary ({})", e);
            None
        }
    };

    if options.json {
        print_json(
            api_client.tokens().email(),
            &code_assist,
            &quota_summary_resp,
        );
    } else if std::io::stdout().is_terminal() {
        eprintln!(); // Print the separating newline to stderr
        print_pretty(
            api_client.tokens().email(),
            &code_assist,
            &quota_summary_resp,
        );
    } else {
        print_markdown(
            api_client.tokens().email(),
            &code_assist,
            &quota_summary_resp,
        );
    }

    Ok(())
}

fn format_reset_time(reset_time_str: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(reset_time_str) {
        let diff = dt.with_timezone(&Utc).signed_duration_since(Utc::now());
        let sec = diff.num_seconds();

        let local_dt = dt.with_timezone(&chrono::Local);
        let reset_at = local_dt.format("%Y-%m-%d %H:%M").to_string();

        if sec <= 0 {
            return format!("{} - Resets soon", reset_at);
        }

        let total_mins = (sec + 59) / 60;
        let hours = total_mins / 60;
        let mins = total_mins % 60;

        let duration = if hours >= 24 {
            let days = hours / 24;
            let hours = hours % 24;
            format!("{}d {}h {}m", days, hours, mins)
        } else if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}m", mins)
        };
        format!("{} - {}", reset_at, duration)
    } else {
        reset_time_str.to_string()
    }
}

fn format_remaining(fraction: Option<f64>, is_exhausted: bool) -> String {
    let bar_len = 15;
    if is_exhausted {
        let bar = "░░░EXHAUSTED░░░";
        return format!("[{}] {:>3}%", bar, 0).red().bold().to_string();
    }
    match fraction {
        Some(f) => {
            let pct = (f * 100.0).round() as i32;
            let clamped_f = f.clamp(0.0, 1.0);

            let total_steps = bar_len * 8;
            let active_steps = (clamped_f * total_steps as f64).round() as usize;

            let full_blocks = active_steps / 8;
            let partial_step = active_steps % 8;
            let partials = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

            let partial_char = partials[partial_step];
            let empty_blocks = bar_len - full_blocks - (if partial_step > 0 { 1 } else { 0 });

            let bar = format!(
                "{}{}{}",
                "█".repeat(full_blocks),
                partial_char,
                "░".repeat(empty_blocks)
            );

            if pct >= 75 {
                format!("[{}] {:>3}%", bar, pct).green().to_string()
            } else if pct >= 50 {
                format!("[{}] {:>3}%", bar, pct).yellow().to_string()
            } else if pct >= 25 {
                format!("[{}] {:>3}%", bar, pct)
                    .truecolor(255, 165, 0)
                    .to_string()
            } else {
                format!("[{}] {:>3}%", bar, pct).red().to_string()
            }
        }
        None => "N/A".to_string(),
    }
}

fn strip_ansi_codes(s: &str) -> String {
    let mut clean = String::new();
    let mut in_escape = false;
    let chars = s.chars().peekable();
    for c in chars {
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

    print_border_line('╭', '┬', '╮', &widths);

    print!("│");
    for (i, h) in headers.iter().enumerate() {
        let header_str = format!("{:<width$}", h, width = widths[i]).cyan().bold();
        print!(" {} │", header_str);
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

    print_border_line('╰', '┴', '╯', &widths);
}

/// Helper to format a single QuotaSummaryBucket into table row fields.
fn format_quota_bucket_row(bucket: &crate::google_api::QuotaSummaryBucket) -> Vec<String> {
    let name = bucket
        .display_name
        .as_deref()
        .or(bucket.bucket_id.as_deref())
        .unwrap_or("Unknown Bucket");

    let rem_pct = if bucket.disabled == Some(true) {
        let bar = "███UNLIMITED███";
        format!("[{}]  N/A", bar).green().bold().to_string()
    } else {
        let is_exhausted = bucket.remaining_fraction.map(|f| f <= 0.0).unwrap_or(false);
        format_remaining(bucket.remaining_fraction, is_exhausted)
    };

    let reset_time = bucket
        .reset_time
        .as_ref()
        .map(|t| format_reset_time(t))
        .unwrap_or_else(|| "N/A".to_string());

    vec![name.to_string(), rem_pct, reset_time]
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
    println!("{}", "📊 Antigravity Quota Status".blue().bold());
    println!("   Retrieved: {}", now_str);
    println!("   Active Account: {}", email.bold());

    let plan_id = code_assist
        .plan_info
        .as_ref()
        .and_then(|p| p.plan_type.clone())
        .or_else(|| code_assist.paid_tier.as_ref().and_then(|t| t.id.clone()))
        .or_else(|| code_assist.current_tier.as_ref().and_then(|t| t.id.clone()))
        .unwrap_or_else(|| "Unknown".to_string());

    let plan_label = match plan_id.as_str() {
        "g1-pro-tier" => "Google AI Pro".to_string(),
        "free-tier" => "Free Tier".to_string(),
        "standard-tier" => "Standard Tier".to_string(),
        other => {
            let mut name = other.to_string();
            if name.ends_with("-tier") {
                name = name[..name.len() - 5].to_string();
            }
            name.split('-')
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    };

    println!("   Plan Type: {}", plan_label.magenta());

    if let Some(avail) = code_assist.available_prompt_credits {
        if let Some(monthly) = code_assist
            .plan_info
            .as_ref()
            .and_then(|p| p.monthly_prompt_credits)
        {
            println!(
                "   Prompt Credits: {} / {} ({} remaining)",
                avail.to_string().green(),
                monthly,
                format_remaining(Some(avail as f64 / monthly as f64), false)
            );
        } else {
            println!("   Prompt Credits: {}", avail.to_string().green());
        }
    } else {
        println!("   Prompt Credits: {}", "Unlimited".green());
    }

    if let Some(summary) = quota_summary_resp {
        // 1. Display individual buckets if they exist
        let mut quota_rows = vec![];
        if let Some(buckets) = &summary.buckets {
            for bucket in buckets {
                quota_rows.push(format_quota_bucket_row(bucket));
            }
        }

        if !quota_rows.is_empty() {
            println!("\n{}", "📋 User Quota Summary (Buckets)".cyan().bold());
            print_table(&["Quota Bucket", "Remaining %", "Reset Time"], &quota_rows);
        }

        // 2. Display groups if they exist
        if let Some(groups) = &summary.groups {
            for group in groups {
                let group_name = group.display_name.as_deref().unwrap_or("Unnamed Group");
                let group_desc = group.description.as_deref().unwrap_or("");

                println!("\n{} {}", "👥".cyan().bold(), group_name.cyan().bold());
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
                    print_table(&["Quota Bucket", "Remaining %", "Reset Time"], &group_rows);
                }
            }
        }
    }

    if let Some(summary) = quota_summary_resp
        && let Some(description) = &summary.description
        && !description.is_empty()
    {
        println!("\n{}", description);
    }
}

fn print_markdown_table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }
    println!("| {} |", headers.join(" | "));
    println!(
        "|{}|",
        headers.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    );
    for row in rows {
        let clean_row: Vec<String> = row.iter().map(|v| strip_ansi_codes(v)).collect();
        println!("| {} |", clean_row.join(" | "));
    }
}

fn print_markdown(
    email: &str,
    code_assist: &crate::google_api::LoadCodeAssistResponse,
    quota_summary_resp: &Option<RetrieveUserQuotaSummaryResponse>,
) {
    let now_str = Utc::now()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    println!("# 📊 Antigravity Quota Status\n");
    println!("- **Retrieved:** {}", now_str);
    println!("- **Active Account:** {}", email);

    let plan_id = code_assist
        .plan_info
        .as_ref()
        .and_then(|p| p.plan_type.clone())
        .or_else(|| code_assist.paid_tier.as_ref().and_then(|t| t.id.clone()))
        .or_else(|| code_assist.current_tier.as_ref().and_then(|t| t.id.clone()))
        .unwrap_or_else(|| "Unknown".to_string());

    let plan_label = match plan_id.as_str() {
        "g1-pro-tier" => "Google AI Pro".to_string(),
        "free-tier" => "Free Tier".to_string(),
        "standard-tier" => "Standard Tier".to_string(),
        other => {
            let mut name = other.to_string();
            if name.ends_with("-tier") {
                name = name[..name.len() - 5].to_string();
            }
            name.split('-')
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    };

    println!("- **Plan Type:** {}", plan_label);

    if let Some(avail) = code_assist.available_prompt_credits {
        if let Some(monthly) = code_assist
            .plan_info
            .as_ref()
            .and_then(|p| p.monthly_prompt_credits)
        {
            println!(
                "- **Prompt Credits:** {} / {} ({} remaining)",
                avail,
                monthly,
                strip_ansi_codes(&format_remaining(
                    Some(avail as f64 / monthly as f64),
                    false
                ))
            );
        } else {
            println!("- **Prompt Credits:** {}", avail);
        }
    } else {
        println!("- **Prompt Credits:** Unlimited");
    }

    if let Some(summary) = quota_summary_resp {
        let mut quota_rows = vec![];
        if let Some(buckets) = &summary.buckets {
            for bucket in buckets {
                quota_rows.push(format_quota_bucket_row(bucket));
            }
        }

        if !quota_rows.is_empty() {
            println!("\n## 📋 User Quota Summary (Buckets)\n");
            print_markdown_table(&["Quota Bucket", "Remaining %", "Reset Time"], &quota_rows);
        }

        if let Some(groups) = &summary.groups {
            for group in groups {
                let group_name = group.display_name.as_deref().unwrap_or("Unnamed Group");
                let group_desc = group.description.as_deref().unwrap_or("");

                println!("\n## 👥 {}\n", group_name);
                if !group_desc.is_empty() {
                    println!("{}\n", group_desc);
                }

                let mut group_rows = vec![];
                if let Some(buckets) = &group.buckets {
                    for bucket in buckets {
                        group_rows.push(format_quota_bucket_row(bucket));
                    }
                }

                if !group_rows.is_empty() {
                    print_markdown_table(
                        &["Quota Bucket", "Remaining %", "Reset Time"],
                        &group_rows,
                    );
                }
            }
        }

        if let Some(description) = &summary.description
            && !description.is_empty()
        {
            println!("\n{}", description);
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
    fn test_format_reset_time() {
        let now = Utc::now();
        let local_fmt = |dt: DateTime<Utc>| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        };

        // 1. Resets soon
        let past = now - Duration::seconds(10);
        assert_eq!(
            format_reset_time(&past.to_rfc3339()),
            format!("{} - Resets soon", local_fmt(past))
        );

        // 2. Under a minute -> 1m
        let seconds_30 = now + Duration::seconds(30);
        assert_eq!(
            format_reset_time(&seconds_30.to_rfc3339()),
            format!("{} - 1m", local_fmt(seconds_30))
        );

        // 3. 2 minutes 5 seconds -> 3m
        let mins_2_sec_5 = now + Duration::seconds(125);
        assert_eq!(
            format_reset_time(&mins_2_sec_5.to_rfc3339()),
            format!("{} - 3m", local_fmt(mins_2_sec_5))
        );

        // 4. 1h 1m 5s -> 1h 2m
        let hours_1_mins_1 = now + Duration::hours(1) + Duration::minutes(1) + Duration::seconds(5);
        assert_eq!(
            format_reset_time(&hours_1_mins_1.to_rfc3339()),
            format!("{} - 1h 2m", local_fmt(hours_1_mins_1))
        );

        // 5. 24h exactly -> 1d 0h 0m
        let hours_24 = now + Duration::hours(24);
        assert_eq!(
            format_reset_time(&hours_24.to_rfc3339()),
            format!("{} - 1d 0h 0m", local_fmt(hours_24))
        );

        // 6. 25h 1m 5s -> 1d 1h 2m
        let hours_25_mins_1 =
            now + Duration::hours(25) + Duration::minutes(1) + Duration::seconds(5);
        assert_eq!(
            format_reset_time(&hours_25_mins_1.to_rfc3339()),
            format!("{} - 1d 1h 2m", local_fmt(hours_25_mins_1))
        );

        // 7. 49h 5s -> 2d 1h 1m
        let hours_49 = now + Duration::hours(49) + Duration::seconds(5);
        assert_eq!(
            format_reset_time(&hours_49.to_rfc3339()),
            format!("{} - 2d 1h 1m", local_fmt(hours_49))
        );
    }

    #[test]
    fn test_format_remaining() {
        // Test that remaining percentage is rounded to the nearest integer
        // 12.34% -> 12% (0.1234)
        let formatted_12_3 = format_remaining(Some(0.1234), false);
        assert!(strip_ansi_codes(&formatted_12_3).contains(" 12%"));

        // 12.56% -> 13% (0.1256)
        let formatted_12_6 = format_remaining(Some(0.1256), false);
        assert!(strip_ansi_codes(&formatted_12_6).contains(" 13%"));

        // 12.5% -> 13% (0.125)
        let formatted_12_5 = format_remaining(Some(0.125), false);
        assert!(strip_ansi_codes(&formatted_12_5).contains(" 13%"));

        // 12.49% -> 12% (0.1249)
        let formatted_12_49 = format_remaining(Some(0.1249), false);
        assert!(strip_ansi_codes(&formatted_12_49).contains(" 12%"));

        // Exhausted status
        let formatted_exhausted = format_remaining(Some(0.0), true);
        assert_eq!(
            strip_ansi_codes(&formatted_exhausted),
            "[░░░EXHAUSTED░░░]   0%"
        );
    }
}
