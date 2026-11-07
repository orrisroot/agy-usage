use crate::agy::UsageOutput;
use chrono::{DateTime, Utc};
use colored::Colorize;
use std::io::IsTerminal;
use unicode_width::UnicodeWidthStr;

pub struct QuotaOptions {
    pub json: bool,
    pub debug: bool,
}

pub async fn run_quota(options: QuotaOptions) -> Result<(), Box<dyn std::error::Error>> {
    let report = crate::agy::fetch_usage(options.debug).await?;

    if options.json {
        print_json(&report.raw);
    } else if std::io::stdout().is_terminal() {
        eprintln!(); // Print the separating newline to stderr
        print_pretty(&report.output);
    } else {
        print_markdown(&report.output);
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

/// Helper to format a single usage bucket into table row fields.
fn format_usage_bucket_row(bucket: &crate::agy::UsageBucket, group_name: &str) -> Vec<String> {
    let raw_name = bucket
        .name
        .as_deref()
        .or(bucket.id.as_deref())
        .unwrap_or("Unknown Bucket");

    let name = raw_name
        .strip_suffix(" Remaining")
        .or_else(|| raw_name.strip_suffix("Remaining"))
        .unwrap_or(raw_name)
        .trim();

    // The group name is already shown as the table heading, so avoid repeating
    // it in the bucket column (e.g. "Gemini 4 Argon Weekly Limit" -> "Weekly Limit").
    let stripped_name = if group_name.is_empty() {
        name
    } else {
        name.strip_prefix(group_name)
            .map(str::trim_start)
            .unwrap_or(name)
    };
    let name = if stripped_name.is_empty() {
        name
    } else {
        stripped_name
    };
    let name = if name.is_empty() {
        "Unknown Bucket"
    } else {
        name
    };

    let is_exhausted = bucket.remaining_fraction.map(|f| f <= 0.0).unwrap_or(false);
    let rem_pct = format_remaining(bucket.remaining_fraction, is_exhausted);

    let reset_time = bucket
        .reset_time
        .as_deref()
        .map(format_reset_time)
        .unwrap_or_else(|| "N/A".to_string());

    vec![name.to_string(), rem_pct, reset_time]
}

fn print_pretty(usage: &UsageOutput) {
    let now_str = Utc::now()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    println!("{}", "📊 Antigravity Quota Status".blue().bold());
    println!("   Retrieved: {}", now_str);

    for group in usage.groups() {
        println!("\n{} {}", "👥".cyan().bold(), group.name.cyan().bold());
        if let Some(description) = group.description.as_deref()
            && !description.is_empty()
        {
            println!("{}", description);
        }

        let rows: Vec<Vec<String>> = group
            .buckets
            .iter()
            .map(|bucket| format_usage_bucket_row(bucket, &group.name))
            .collect();
        print_table(&["Quota Bucket", "Remaining %", "Reset Time"], &rows);
    }

    if let Some(description) = usage.description()
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

fn print_markdown(usage: &UsageOutput) {
    let now_str = Utc::now()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    println!("# 📊 Antigravity Quota Status\n");
    println!("- **Retrieved:** {}", now_str);

    for group in usage.groups() {
        println!("\n## 👥 {}\n", group.name);
        if let Some(description) = group.description.as_deref()
            && !description.is_empty()
        {
            println!("{}\n", description);
        }

        let rows: Vec<Vec<String>> = group
            .buckets
            .iter()
            .map(|bucket| format_usage_bucket_row(bucket, &group.name))
            .collect();
        print_markdown_table(&["Quota Bucket", "Remaining %", "Reset Time"], &rows);
    }

    if let Some(description) = usage.description()
        && !description.is_empty()
    {
        println!("\n{}", description);
    }
}

fn print_json(raw: &serde_json::Value) {
    match serde_json::to_string_pretty(raw) {
        Ok(json_str) => println!("{}", json_str),
        Err(e) => eprintln!("Warning: Failed to serialize `agy` output: {}", e),
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

    #[test]
    fn test_format_usage_bucket_row_trims_remaining() {
        let bucket1 = crate::agy::UsageBucket {
            id: None,
            name: Some("Weekly Limit Remaining".to_string()),
            remaining_fraction: Some(1.0),
            reset_time: None,
        };
        let row1 = format_usage_bucket_row(&bucket1, "Gemini Models");
        assert_eq!(row1[0], "Weekly Limit");

        let bucket2 = crate::agy::UsageBucket {
            id: None,
            name: Some("Five Hour Limit Remaining".to_string()),
            remaining_fraction: Some(1.0),
            reset_time: None,
        };
        let row2 = format_usage_bucket_row(&bucket2, "Gemini Models");
        assert_eq!(row2[0], "Five Hour Limit");
    }

    #[test]
    fn test_format_usage_bucket_row_strips_group_name_prefix() {
        let bucket = crate::agy::UsageBucket {
            id: None,
            name: Some("Gemini 4 Argon Weekly Limit".to_string()),
            remaining_fraction: Some(1.0),
            reset_time: None,
        };
        let row = format_usage_bucket_row(&bucket, "Gemini 4 Argon");
        assert_eq!(row[0], "Weekly Limit");
    }

    #[test]
    fn test_format_usage_bucket_row_keeps_name_when_prefix_removal_is_empty() {
        let bucket = crate::agy::UsageBucket {
            id: None,
            name: Some("Gemini 4 Argon".to_string()),
            remaining_fraction: Some(1.0),
            reset_time: None,
        };
        let row = format_usage_bucket_row(&bucket, "Gemini 4 Argon");
        assert_eq!(row[0], "Gemini 4 Argon");
    }
}
