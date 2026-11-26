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
        let models = crate::agy::fetch_models(options.debug).await?;
        let groups = match crate::agy::fetch_usage(options.debug).await {
            Ok(report) => report.output.groups(),
            Err(e) => {
                eprintln!(
                    "Warning: Failed to fetch quota groups from `agy /usage` ({}); falling back to default grouping.",
                    e
                );
                Vec::new()
            }
        };
        let selection = resolve_default_models(&models, &groups);
        if selection.used_heuristic_fallback {
            eprintln!(
                "Warning: Could not match available models to quota groups from `agy /usage`; falling back to default grouping."
            );
        } else {
            for group_name in &selection.unmatched_groups {
                eprintln!(
                    "Warning: No available models matched quota group \"{}\"; skipping.",
                    group_name
                );
            }
        }
        selection.models
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
                        println!("   {}", format_trigger_usage(usage));
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

/// Formats token usage reported by `agy --output-format json`.
///
/// `agy` reports `input_tokens` as uncached input tokens (separate from
/// `cache_read_tokens`, which is excluded from `input_tokens` and `total_tokens`),
/// while `thinking_tokens` is a subset of `output_tokens`.
fn format_trigger_usage(usage: &crate::agy::TriggerUsage) -> String {
    let input_part = if usage.cache_read_tokens.is_some() {
        format!("Input={} (uncached)", usage.input_tokens)
    } else {
        format!("Input={}", usage.input_tokens)
    };

    let output_part = match usage.thinking_tokens {
        Some(thinking) if thinking <= usage.output_tokens => {
            format!(
                "Output={} (incl. {} thinking)",
                usage.output_tokens, thinking
            )
        }
        Some(thinking) => format!("Output={}, Thinking={}", usage.output_tokens, thinking),
        None => format!("Output={}", usage.output_tokens),
    };

    let mut formatted = format!(
        "Tokens Used: {}, {}, Total={}",
        input_part, output_part, usage.total_tokens
    );

    if let Some(cache_read) = usage.cache_read_tokens {
        formatted.push_str(&format!(" | Cache Read={}", cache_read));
    }

    formatted
}

pub fn parse_model_version(model_id: &str) -> Option<(u32, u32)> {
    let model_id = model_id.to_ascii_lowercase();

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

    // 2. Gemini format: gemini-4-neon-..., gemini-3.8-..., gemini-2.5-..., gemini-3-...
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

    if model_id == "argon" || model_id.starts_with("argon-") {
        return Some((4, 0));
    }

    None
}

fn get_model_cost_score(model_id: &str) -> u32 {
    let model_id = model_id.to_ascii_lowercase();
    let mut score: i32 = 0;

    // 1. Model family/provider base cost
    if model_id.contains("gemini")
        || model_id.contains("argon")
        || model_id.contains("orchestrator")
    {
        if model_id.contains("orchestrator") {
            score += 400;
        } else if model_id.contains("flash") {
            score += 100;
        } else if model_id.contains("pro") {
            score += 200;
        } else if model_id.contains("argon")
            || model_id.contains("carbon")
            || model_id.contains("hydrogen")
            || model_id.contains("neon")
        {
            score += 300;
        } else {
            score += 350;
        }
    } else if model_id.contains("gpt-oss") {
        score += 100;
    } else if model_id.contains("haiku") {
        score += 150;
    } else if model_id.contains("sonnet") {
        score += 300;
    } else if model_id.contains("opus") {
        score += 500;
    } else {
        score += 400;
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
    } else if model_id.contains("xhigh") || model_id.contains("extra-high") {
        score += 50;
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
    if let Some((major, minor)) = parse_model_version(&model_id) {
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

fn normalize_label(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn tokenize_alnum(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn is_token_subsequence(needle: &[String], haystack: &[String]) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut needle_idx = 0;
    for token in haystack {
        if *token == needle[needle_idx] {
            needle_idx += 1;
            if needle_idx == needle.len() {
                return true;
            }
        }
    }
    false
}

fn strip_leading_and(s: &str) -> &str {
    match s.get(..4) {
        Some(prefix) if prefix.eq_ignore_ascii_case("and ") => s[4..].trim(),
        _ => s,
    }
}

fn parse_group_model_names(description: &str) -> Vec<String> {
    let Some(raw_list) = description.lines().find_map(|line| {
        let (prefix, rest) = line.split_once(':')?;
        let prefix_lower = prefix.to_ascii_lowercase();
        (!prefix_lower.contains('.') && prefix_lower.contains("model")).then_some(rest)
    }) else {
        return Vec::new();
    };

    let list_segment = raw_list
        .split(". ")
        .next()
        .unwrap_or(raw_list)
        .trim()
        .trim_end_matches('.')
        .trim();

    list_segment
        .split(',')
        .flat_map(|item| {
            let item = strip_leading_and(item.trim().trim_end_matches('.').trim());
            item.split(" and ")
                .map(|part| part.trim().trim_end_matches('.').trim())
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn fallback_group_labels(group_name: &str) -> Vec<String> {
    let normalized = normalize_label(group_name);
    if normalized.is_empty() {
        return Vec::new();
    }

    let base = normalized
        .strip_suffix(" models")
        .or_else(|| normalized.strip_suffix(" model"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&normalized);

    base.split(" and ")
        .flat_map(|part| part.split([',', '/', '&']))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn match_model_label(model: &crate::agy::AgyModel, label: &str) -> Option<(u8, usize)> {
    let norm_label = normalize_label(label);
    if norm_label.is_empty() {
        return None;
    }

    let label_tokens = tokenize_alnum(&norm_label);
    let norm_name = normalize_label(&model.name);
    if !norm_name.is_empty() {
        if norm_name == norm_label {
            return Some((3, norm_label.len()));
        }
        if let Some(rest) = norm_name.strip_prefix(&norm_label)
            && rest.starts_with([' ', '(', '-', '/'])
        {
            return Some((2, norm_label.len()));
        }
        let name_tokens = tokenize_alnum(&norm_name);
        if is_token_subsequence(&label_tokens, &name_tokens) {
            return Some((1, norm_label.len()));
        }
    }

    let id_tokens = tokenize_alnum(&model.id);
    if is_token_subsequence(&label_tokens, &id_tokens) {
        return Some((0, norm_label.len()));
    }

    None
}

fn select_default_models_heuristic(models: &[crate::agy::AgyModel]) -> Vec<String> {
    let mut gemini_list = Vec::new();
    let mut other_list = Vec::new();

    for model in models {
        let id_lower = model.id.to_ascii_lowercase();
        if id_lower.contains("gemini")
            || id_lower.contains("argon")
            || id_lower.contains("orchestrator")
        {
            gemini_list.push(model);
        } else {
            other_list.push(model);
        }
    }

    let mut selected = Vec::new();

    if !gemini_list.is_empty() {
        gemini_list.sort_by_key(|model| get_model_cost_score(&model.id));
        selected.push(gemini_list[0].id.clone());
    }

    if !other_list.is_empty() {
        other_list.sort_by_key(|model| get_model_cost_score(&model.id));
        selected.push(other_list[0].id.clone());
    }

    selected
}

struct ModelSelection {
    models: Vec<String>,
    unmatched_groups: Vec<String>,
    used_heuristic_fallback: bool,
}

fn find_best_matching_group<'a>(
    model: &crate::agy::AgyModel,
    candidates: impl IntoIterator<Item = (usize, &'a [String])>,
) -> Option<usize> {
    let mut best_group: Option<(usize, (u8, usize))> = None;

    for (group_idx, labels) in candidates {
        let mut best_label_score: Option<(u8, usize)> = None;
        for label in labels {
            if let Some(score) = match_model_label(model, label)
                && best_label_score.is_none_or(|cur| score > cur)
            {
                best_label_score = Some(score);
            }
        }

        if let Some(score) = best_label_score
            && best_group.is_none_or(|(_, cur_score)| score > cur_score)
        {
            best_group = Some((group_idx, score));
        }
    }

    best_group.map(|(group_idx, _)| group_idx)
}

fn resolve_default_models(
    models: &[crate::agy::AgyModel],
    groups: &[crate::agy::UsageGroup],
) -> ModelSelection {
    if models.is_empty() {
        return ModelSelection {
            models: Vec::new(),
            unmatched_groups: Vec::new(),
            used_heuristic_fallback: false,
        };
    }

    // `agy models` only returns currently selectable models, so no separate
    // retirement/deprecation filtering is needed here.
    if !groups.is_empty() {
        let mut used_fallback = Vec::with_capacity(groups.len());
        let mut group_labels = Vec::with_capacity(groups.len());
        for group in groups {
            let parsed = group
                .description
                .as_deref()
                .map(parse_group_model_names)
                .unwrap_or_default();
            if parsed.is_empty() {
                used_fallback.push(true);
                group_labels.push(fallback_group_labels(&group.name));
            } else {
                used_fallback.push(false);
                group_labels.push(parsed);
            }
        }

        let mut buckets: Vec<Vec<&crate::agy::AgyModel>> = vec![Vec::new(); groups.len()];
        let mut unmatched_models = Vec::new();
        let mut any_matched = false;

        // Pass 1: Assign each model to the best-matching group from primary labels.
        for model in models {
            if let Some(group_idx) = find_best_matching_group(
                model,
                group_labels
                    .iter()
                    .enumerate()
                    .map(|(idx, labels)| (idx, labels.as_slice())),
            ) {
                buckets[group_idx].push(model);
                any_matched = true;
            } else {
                unmatched_models.push(model);
            }
        }

        // Pass 2: For groups whose description yielded no matches, fall back to
        // matching their group name against models left unmatched in Pass 1.
        if !unmatched_models.is_empty() {
            let recovery_candidates: Vec<(usize, Vec<String>)> = groups
                .iter()
                .enumerate()
                .filter(|(idx, _)| buckets[*idx].is_empty() && !used_fallback[*idx])
                .map(|(idx, group)| (idx, fallback_group_labels(&group.name)))
                .collect();

            if !recovery_candidates.is_empty() {
                for model in unmatched_models {
                    if let Some(group_idx) = find_best_matching_group(
                        model,
                        recovery_candidates
                            .iter()
                            .map(|(idx, labels)| (*idx, labels.as_slice())),
                    ) {
                        buckets[group_idx].push(model);
                        any_matched = true;
                    }
                }
            }
        }

        if any_matched {
            let mut selected = Vec::new();
            let mut unmatched_groups = Vec::new();
            for (idx, mut bucket) in buckets.into_iter().enumerate() {
                if bucket.is_empty() {
                    unmatched_groups.push(groups[idx].name.clone());
                } else {
                    bucket.sort_by_key(|model| get_model_cost_score(&model.id));
                    selected.push(bucket[0].id.clone());
                }
            }
            return ModelSelection {
                models: selected,
                unmatched_groups,
                used_heuristic_fallback: false,
            };
        }
    }

    ModelSelection {
        models: select_default_models_heuristic(models),
        unmatched_groups: Vec::new(),
        used_heuristic_fallback: !groups.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agy::{AgyModel, UsageGroup};

    fn model(id: &str, name: &str) -> AgyModel {
        AgyModel {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    fn group(name: &str, description: Option<&str>) -> UsageGroup {
        UsageGroup {
            name: name.to_string(),
            description: description.map(str::to_string),
            buckets: Vec::new(),
        }
    }

    fn select_default_models(models: &[AgyModel], groups: &[UsageGroup]) -> Vec<String> {
        resolve_default_models(models, groups).models
    }

    fn live_agy_models() -> Vec<AgyModel> {
        vec![
            model("gemini-4-neon-high", "Gemini 4 Neon (High)"),
            model("gemini-4-neon-xhigh", "Gemini 4 Neon (XHigh)"),
            model("gemini-4-neon-medium", "Gemini 4 Neon (Medium)"),
            model("gemini-4-neon-low", "Gemini 4 Neon (Low)"),
            model("gemini-4-carbon-medium", "Gemini 4 Carbon (Medium)"),
            model("gemini-4-carbon-high", "Gemini 4 Carbon (High)"),
            model("gemini-4-carbon-low", "Gemini 4 Carbon (Low)"),
            model("gemini-4-argon-medium", "Gemini 4 Argon (Medium)"),
            model("gemini-4-argon-high", "Gemini 4 Argon (High)"),
            model("gemini-4-argon-low", "Gemini 4 Argon (Low)"),
            model("gemini-4-hydrogen-high", "Gemini 4 Hydrogen (High)"),
            model("gemini-4-hydrogen-xhigh", "Gemini 4 Hydrogen (XHigh)"),
            model("gemini-4-hydrogen-medium", "Gemini 4 Hydrogen (Medium)"),
            model("gemini-4-hydrogen-low", "Gemini 4 Hydrogen (Low)"),
            model("gemini-pro-orchestrator-neon", "Orchestrator (Neon)"),
            model("gemini-pro-orchestrator", "Orchestrator (Carbon)"),
            model("gemini-pro-orchestrator-argon", "Orchestrator (Argon)"),
            model("gemini-3.8-flash-high", "Gemini 3.8 Flash (High)"),
            model("gemini-3.8-flash-medium", "Gemini 3.8 Flash (Medium)"),
            model("gemini-3.8-flash-low", "Gemini 3.8 Flash (Low)"),
            model("claude-opus-5-5-low", "Claude Opus 5.5 (Low)"),
            model("claude-opus-5-5-medium", "Claude Opus 5.5 (Medium)"),
            model("claude-opus-5-5-high", "Claude Opus 5.5 (High)"),
            model("claude-sonnet-5-5-low", "Claude Sonnet 5.5 (Low)"),
            model("claude-sonnet-5-5-medium", "Claude Sonnet 5.5 (Medium)"),
            model("claude-sonnet-5-5-high", "Claude Sonnet 5.5 (High)"),
            model("gpt-oss-120b-medium", "GPT-OSS 120B (Medium)"),
        ]
    }

    #[test]
    fn test_select_default_models_uses_only_available_models() {
        let models = vec![model("gpt-oss-120b-medium", "GPT-OSS 120B (Medium)")];
        let selected = select_default_models(&models, &[]);
        assert_eq!(selected, vec!["gpt-oss-120b-medium".to_string()]);

        let models = vec![model("gemini-3.8-flash-low", "Gemini 3.8 Flash (Low)")];
        let selected = select_default_models(&models, &[]);
        assert_eq!(selected, vec!["gemini-3.8-flash-low".to_string()]);
    }

    #[test]
    fn test_parse_model_version() {
        assert_eq!(parse_model_version("gemini-4-argon"), Some((4, 0)));
        assert_eq!(parse_model_version("gemini-4-neon-xhigh"), Some((4, 0)));
        assert_eq!(parse_model_version("gemini-4-carbon-low"), Some((4, 0)));
        assert_eq!(parse_model_version("gemini-4-hydrogen-low"), Some((4, 0)));
        assert_eq!(parse_model_version("gemini-pro-orchestrator-argon"), None);
        assert_eq!(parse_model_version("gemini-pro-orchestrator-neon"), None);
        assert_eq!(parse_model_version("gemini-pro-orchestrator"), None);
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
        let models = vec![model("gemini-3.6-flash-low", "Gemini 3.6 Flash (Low)")];
        assert_eq!(
            select_default_models(&models, &[]),
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

        // Flash (10x) < Pro (20x) < Gemini 4 family (30x) < Unknown Gemini (35x) < Orchestrator (40x)
        let score_argon_low = get_model_cost_score("gemini-4-argon-low");
        let score_argon = get_model_cost_score("gemini-4-argon");
        let score_argon_high = get_model_cost_score("gemini-4-argon-high");
        let score_carbon_low = get_model_cost_score("gemini-4-carbon-low");
        let score_hydrogen_low = get_model_cost_score("gemini-4-hydrogen-low");
        let score_neon_low = get_model_cost_score("gemini-4-neon-low");
        let score_unknown_gemini_low = get_model_cost_score("gemini-4-xenon-low");
        let score_orch_argon = get_model_cost_score("gemini-pro-orchestrator-argon");
        let score_orch_neon = get_model_cost_score("gemini-pro-orchestrator-neon");
        let score_orch_carbon = get_model_cost_score("gemini-pro-orchestrator");

        assert!(score_38_flash_low < score_38_pro_low);
        assert!(score_38_pro_low < score_argon_low);
        assert!(score_38_pro_low < score_carbon_low);
        assert!(score_38_pro_low < score_hydrogen_low);
        assert!(score_38_pro_low < score_neon_low);
        assert!(score_argon_low < score_argon);
        assert!(score_argon < score_argon_high);
        assert!(score_neon_low < score_unknown_gemini_low);
        assert!(score_unknown_gemini_low < score_orch_argon);
        assert!(score_argon_low < score_orch_argon);
        assert!(score_neon_low < score_orch_neon);
        assert!(score_carbon_low < score_orch_carbon);

        // Tier ordering: low < medium < high < xhigh
        for prefix in ["gemini-4-neon", "gemini-4-hydrogen"] {
            let low = get_model_cost_score(&format!("{}-low", prefix));
            let medium = get_model_cost_score(&format!("{}-medium", prefix));
            let high = get_model_cost_score(&format!("{}-high", prefix));
            let xhigh = get_model_cost_score(&format!("{}-xhigh", prefix));
            assert!(low < medium);
            assert!(medium < high);
            assert!(high < xhigh);
        }

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
    fn test_select_default_models_with_live_two_group_usage() {
        let models = live_agy_models();
        let groups = vec![
            group(
                "Gemini Models",
                Some(
                    "Models within this group: Gemini 4 Neon, Gemini 4 Carbon, Gemini 4 Argon, Gemini 4 Hydrogen, Orchestrator (Neon), Orchestrator (Carbon), Orchestrator (Argon), Gemini 3.8 Flash",
                ),
            ),
            group(
                "Claude and GPT models",
                Some("Models within this group: Claude Opus 5.5, Claude Sonnet 5.5, GPT-OSS 120B"),
            ),
        ];

        let selected = select_default_models(&models, &groups);
        assert_eq!(
            selected,
            vec![
                "gemini-3.8-flash-low".to_string(),
                "gpt-oss-120b-medium".to_string(),
            ]
        );
    }

    #[test]
    fn test_select_default_models_with_dedicated_argon_group() {
        let models = live_agy_models();
        let groups = vec![
            group(
                "Gemini Models",
                Some(
                    "Models within this group: Gemini 4 Neon, Gemini 4 Carbon, Gemini 4 Hydrogen, Orchestrator (Neon), Orchestrator (Carbon), Gemini 3.8 Flash",
                ),
            ),
            group(
                "Gemini 4 Argon",
                Some("Models within this group: Gemini 4 Argon, Orchestrator (Argon)"),
            ),
            group(
                "Claude and GPT models",
                Some("Models within this group: Claude Opus 5.5, Claude Sonnet 5.5, GPT-OSS 120B"),
            ),
        ];

        let selected = select_default_models(&models, &groups);
        assert_eq!(
            selected,
            vec![
                "gemini-3.8-flash-low".to_string(),
                "gemini-4-argon-low".to_string(),
                "gpt-oss-120b-medium".to_string(),
            ]
        );
    }

    #[test]
    fn test_select_default_models_with_historical_short_descriptions() {
        let models = vec![
            model("gemini-3.8-flash-low", "Gemini 3.8 Flash (Low)"),
            model("gemini-3.8-pro-low", "Gemini 3.8 Pro (Low)"),
            model("claude-sonnet-5-5-low", "Claude Sonnet 5.5 (Low)"),
            model("gpt-oss-120b-medium", "GPT-OSS 120B (Medium)"),
        ];
        let groups = vec![
            group(
                "Gemini Models",
                Some("Models within this group: Gemini Flash, Gemini Pro"),
            ),
            group(
                "Claude and GPT models",
                Some("Models within this group: Claude Opus, Claude Sonnet, GPT-OSS"),
            ),
        ];

        let selected = select_default_models(&models, &groups);
        assert_eq!(
            selected,
            vec![
                "gemini-3.8-flash-low".to_string(),
                "gpt-oss-120b-medium".to_string(),
            ]
        );
    }

    #[test]
    fn test_select_default_models_with_tsv_fallback_groups_without_description() {
        let models = live_agy_models();
        let groups = vec![
            group("Gemini Models", None),
            group("Claude and GPT models", None),
        ];

        let selected = select_default_models(&models, &groups);
        assert_eq!(
            selected,
            vec![
                "gemini-3.8-flash-low".to_string(),
                "gpt-oss-120b-medium".to_string(),
            ]
        );
    }

    #[test]
    fn test_select_default_models_heuristic_fallback_merges_argon_into_gemini() {
        let models = vec![
            model("gemini-4-argon-high", "Gemini 4 Argon (High)"),
            model("gemini-4-argon-low", "Gemini 4 Argon (Low)"),
            model("gemini-4-argon-medium", "Gemini 4 Argon (Medium)"),
            model("gemini-4-neon-low", "Gemini 4 Neon (Low)"),
            model("gemini-3.8-flash-low", "Gemini 3.8 Flash (Low)"),
            model("claude-sonnet-5-5-low", "Claude Sonnet 5.5 (Low)"),
        ];
        let selected = select_default_models(&models, &[]);
        assert_eq!(
            selected,
            vec![
                "gemini-3.8-flash-low".to_string(),
                "claude-sonnet-5-5-low".to_string(),
            ]
        );

        let models_without_flash = vec![
            model("gemini-4-argon-low", "Gemini 4 Argon (Low)"),
            model("gpt-oss-120b-medium", "GPT-OSS 120B (Medium)"),
        ];
        let selected_no_flash = select_default_models(&models_without_flash, &[]);
        assert_eq!(
            selected_no_flash,
            vec![
                "gemini-4-argon-low".to_string(),
                "gpt-oss-120b-medium".to_string(),
            ]
        );
    }

    #[test]
    fn test_parse_group_model_names() {
        assert_eq!(
            parse_group_model_names("Models within this group: Gemini 4 Neon, Gemini 3.8 Flash"),
            vec!["Gemini 4 Neon", "Gemini 3.8 Flash"]
        );
        assert_eq!(
            parse_group_model_names(
                "Models within this group: Claude Opus 5.5, GPT-OSS 120B. Quota is shared."
            ),
            vec!["Claude Opus 5.5", "GPT-OSS 120B"]
        );
        assert_eq!(
            parse_group_model_names(
                "Models within this group: Claude Opus 5.5, Claude Sonnet 5.5, and GPT-OSS 120B"
            ),
            vec!["Claude Opus 5.5", "Claude Sonnet 5.5", "GPT-OSS 120B"]
        );
        assert_eq!(
            parse_group_model_names("Models within this group: Gemini Flash and Gemini Pro"),
            vec!["Gemini Flash", "Gemini Pro"]
        );
        assert!(
            parse_group_model_names("These models share a 5h limit. Note: resets every 5h.")
                .is_empty()
        );
        assert_eq!(
            parse_group_model_names("Models: 日本語モデル"),
            vec!["日本語モデル"]
        );
    }

    #[test]
    fn test_select_default_models_recovers_group_when_description_does_not_match() {
        let models = live_agy_models();
        let groups_unknown_desc = vec![
            group(
                "Gemini Models",
                Some("Models within this group: Gemini 3.8 Flash"),
            ),
            group(
                "Claude & GPT models",
                Some("Models within this group: Unknown Label"),
            ),
        ];
        assert_eq!(
            select_default_models(&models, &groups_unknown_desc),
            vec![
                "gemini-3.8-flash-low".to_string(),
                "gpt-oss-120b-medium".to_string(),
            ]
        );

        let groups_prose_desc = vec![
            group(
                "Gemini Models",
                Some("Models within this group: Gemini 3.8 Flash"),
            ),
            group(
                "Claude & GPT models",
                Some("These models share a 5h limit. Note: see docs"),
            ),
        ];
        assert_eq!(
            select_default_models(&models, &groups_prose_desc),
            vec![
                "gemini-3.8-flash-low".to_string(),
                "gpt-oss-120b-medium".to_string(),
            ]
        );
    }

    #[test]
    fn test_resolve_default_models_falls_back_to_heuristic_when_no_group_matches() {
        let models = live_agy_models();
        let groups = vec![
            group("Group A", Some("Models within this group: Unknown Alpha")),
            group("Group B", Some("Models within this group: Unknown Beta")),
        ];

        let selection = resolve_default_models(&models, &groups);
        assert!(selection.used_heuristic_fallback);
        assert!(selection.unmatched_groups.is_empty());
        assert_eq!(
            selection.models,
            vec![
                "gemini-3.8-flash-low".to_string(),
                "gpt-oss-120b-medium".to_string(),
            ]
        );
    }

    #[test]
    fn test_resolve_default_models_reports_unmatched_group() {
        let models = live_agy_models();
        let groups = vec![
            group(
                "Gemini Models",
                Some(
                    "Models within this group: Gemini 4 Neon, Gemini 4 Carbon, Gemini 4 Argon, Gemini 4 Hydrogen, Orchestrator (Neon), Orchestrator (Carbon), Orchestrator (Argon), Gemini 3.8 Flash",
                ),
            ),
            group("Future Models", Some("Models within this group: Future X")),
            group(
                "Claude and GPT models",
                Some("Models within this group: Claude Opus 5.5, Claude Sonnet 5.5, GPT-OSS 120B"),
            ),
        ];

        let selection = resolve_default_models(&models, &groups);
        assert!(!selection.used_heuristic_fallback);
        assert_eq!(
            selection.unmatched_groups,
            vec!["Future Models".to_string()]
        );
        assert_eq!(
            selection.models,
            vec![
                "gemini-3.8-flash-low".to_string(),
                "gpt-oss-120b-medium".to_string(),
            ]
        );
    }

    #[test]
    fn test_select_default_models_empty_models_returns_empty() {
        let groups = vec![group("Gemini Models", None)];
        assert!(select_default_models(&[], &groups).is_empty());
    }

    #[test]
    fn test_format_trigger_usage_with_cache_and_thinking() {
        let usage = crate::agy::TriggerUsage {
            input_tokens: 7474,
            output_tokens: 497,
            thinking_tokens: Some(490),
            cache_read_tokens: Some(7992),
            total_tokens: 7971,
        };
        assert_eq!(
            format_trigger_usage(&usage),
            "Tokens Used: Input=7474 (uncached), Output=497 (incl. 490 thinking), Total=7971 | Cache Read=7992"
        );
    }

    #[test]
    fn test_format_trigger_usage_with_zero_optional_fields() {
        let usage = crate::agy::TriggerUsage {
            input_tokens: 10080,
            output_tokens: 29,
            thinking_tokens: Some(0),
            cache_read_tokens: Some(0),
            total_tokens: 10109,
        };
        assert_eq!(
            format_trigger_usage(&usage),
            "Tokens Used: Input=10080 (uncached), Output=29 (incl. 0 thinking), Total=10109 | Cache Read=0"
        );
    }

    #[test]
    fn test_format_trigger_usage_without_optional_fields() {
        let usage = crate::agy::TriggerUsage {
            input_tokens: 100,
            output_tokens: 20,
            thinking_tokens: None,
            cache_read_tokens: None,
            total_tokens: 120,
        };
        assert_eq!(
            format_trigger_usage(&usage),
            "Tokens Used: Input=100, Output=20, Total=120"
        );
    }

    #[test]
    fn test_format_trigger_usage_with_partial_optional_fields() {
        let cache_only = crate::agy::TriggerUsage {
            input_tokens: 5602,
            output_tokens: 147,
            thinking_tokens: None,
            cache_read_tokens: Some(2880),
            total_tokens: 5749,
        };
        assert_eq!(
            format_trigger_usage(&cache_only),
            "Tokens Used: Input=5602 (uncached), Output=147, Total=5749 | Cache Read=2880"
        );

        let thinking_only = crate::agy::TriggerUsage {
            input_tokens: 100,
            output_tokens: 20,
            thinking_tokens: Some(12),
            cache_read_tokens: None,
            total_tokens: 120,
        };
        assert_eq!(
            format_trigger_usage(&thinking_only),
            "Tokens Used: Input=100, Output=20 (incl. 12 thinking), Total=120"
        );
    }

    #[test]
    fn test_format_trigger_usage_handles_thinking_exceeding_output() {
        let usage = crate::agy::TriggerUsage {
            input_tokens: 100,
            output_tokens: 10,
            thinking_tokens: Some(25),
            cache_read_tokens: Some(0),
            total_tokens: 110,
        };
        assert_eq!(
            format_trigger_usage(&usage),
            "Tokens Used: Input=100 (uncached), Output=10, Thinking=25, Total=110 | Cache Read=0"
        );
    }
}
