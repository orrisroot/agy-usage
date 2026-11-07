use serde::Deserialize;

/// Raw output of `agy -p "/usage" --output-format json`, plus its parsed form.
pub struct UsageReport {
    pub raw: serde_json::Value,
    pub output: UsageOutput,
}

#[derive(Debug, Deserialize)]
pub struct UsageOutput {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub response: String,
    #[serde(default)]
    pub command: Option<UsageCommand>,
}

#[derive(Debug, Deserialize)]
pub struct UsageCommand {
    #[serde(default)]
    pub data: Option<UsageData>,
}

#[derive(Debug, Deserialize)]
pub struct UsageData {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub groups: Vec<UsageGroup>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UsageGroup {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub buckets: Vec<UsageBucket>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UsageBucket {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub remaining_fraction: Option<f64>,
    #[serde(default)]
    pub reset_time: Option<String>,
}

impl UsageOutput {
    pub fn description(&self) -> Option<&str> {
        self.command
            .as_ref()
            .and_then(|command| command.data.as_ref())
            .and_then(|data| data.description.as_deref())
    }

    /// Returns structured groups when `command.data` is available, otherwise
    /// falls back to parsing the tab-separated `response` text.
    pub fn groups(&self) -> Vec<UsageGroup> {
        if let Some(data) = self
            .command
            .as_ref()
            .and_then(|command| command.data.as_ref())
            && !data.groups.is_empty()
        {
            return data.groups.clone();
        }

        parse_response_groups(&self.response)
    }
}

#[derive(Debug, Deserialize)]
pub struct AgyAccount {
    #[serde(default)]
    pub auth_method: Option<String>,
    #[serde(default)]
    pub id_token_email: Option<String>,
    #[serde(default)]
    pub token: Option<serde_json::Value>,
}

impl AgyAccount {
    pub fn email(&self) -> Option<&str> {
        self.id_token_email
            .as_deref()
            .filter(|email| !email.is_empty())
    }

    pub fn is_authenticated(&self) -> bool {
        self.email().is_some()
            || self.token.as_ref().is_some_and(|token| {
                token
                    .get("access_token")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| !value.is_empty())
            })
    }
}

/// Reads the account information stored by the agy CLI.
///
/// agy stores credentials in the OS keyring (service `gemini`, user
/// `antigravity`) and falls back to a token file when the keyring is
/// unavailable.
pub fn read_account() -> Option<AgyAccount> {
    read_account_from_keyring().or_else(read_account_from_file)
}

fn read_account_from_keyring() -> Option<AgyAccount> {
    let entry = keyring::Entry::new("gemini", "antigravity").ok()?;
    let secret = entry.get_password().ok()?;
    parse_account(&secret)
}

fn read_account_from_file() -> Option<AgyAccount> {
    let path = dirs::home_dir()?
        .join(".gemini")
        .join("antigravity-cli")
        .join("antigravity-oauth-token");
    let content = std::fs::read_to_string(path).ok()?;
    parse_account(&content)
}

fn parse_account(content: &str) -> Option<AgyAccount> {
    let account: AgyAccount = serde_json::from_str(content).ok()?;
    account.is_authenticated().then_some(account)
}

fn parse_response_groups(response: &str) -> Vec<UsageGroup> {
    let mut groups: Vec<UsageGroup> = Vec::new();

    for line in response.lines() {
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() < 3 {
            continue;
        }

        let group_name = columns[0].trim();
        if group_name.is_empty() {
            continue;
        }

        let bucket = UsageBucket {
            id: None,
            name: Some(columns[1].trim().to_string()).filter(|name| !name.is_empty()),
            remaining_fraction: parse_percentage(columns[2]),
            reset_time: columns
                .get(3)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        };

        if let Some(group) = groups.iter_mut().find(|group| group.name == group_name) {
            group.buckets.push(bucket);
        } else {
            groups.push(UsageGroup {
                name: group_name.to_string(),
                description: None,
                buckets: vec![bucket],
            });
        }
    }

    groups
}

fn parse_percentage(value: &str) -> Option<f64> {
    let value = value.trim().trim_end_matches('%').trim();
    value
        .parse::<f64>()
        .ok()
        .map(|percent| (percent / 100.0).clamp(0.0, 1.0))
}

#[derive(Debug, Deserialize)]
pub struct TriggerOutput {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub response: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub duration_seconds: f64,
    #[serde(default)]
    pub usage: Option<TriggerUsage>,
}

#[derive(Debug, Deserialize)]
pub struct TriggerUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

pub async fn fetch_models(debug: bool) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = tokio::process::Command::new("agy")
        .arg("models")
        .output()
        .await
        .map_err(|e| {
            format!(
                "Failed to run `agy models`: {}. Make sure the agy CLI is installed and available in PATH.",
                e
            )
        })?;

    if debug && !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            format!("`agy models` failed with {}", output.status).into()
        } else {
            format!("`agy models` failed: {}", stderr).into()
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let models = parse_models_output(&stdout);

    if models.is_empty() {
        return Err("`agy models` returned no models".into());
    }

    Ok(models)
}

fn parse_models_output(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(id, _)| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

pub async fn trigger_model(
    model: &str,
    prompt: &str,
    debug: bool,
) -> Result<TriggerOutput, Box<dyn std::error::Error>> {
    let output = tokio::process::Command::new("agy")
        .args([
            "--disable-slash-commands",
            "-p",
            prompt,
            "--model",
            model,
            "--output-format",
            "json",
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to run `agy` for model {}: {}", model, e))?;

    if debug && !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            format!("`agy` failed for model {} with {}", model, output.status).into()
        } else {
            format!("`agy` failed for model {}: {}", model, stderr).into()
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<TriggerOutput>(&stdout)
        .map_err(|e| format!("Failed to parse `agy` output for model {}: {}", model, e).into())
}

pub async fn fetch_usage(debug: bool) -> Result<UsageReport, Box<dyn std::error::Error>> {
    let output = tokio::process::Command::new("agy")
        .args(["-p", "/usage", "--output-format", "json"])
        .output()
        .await
        .map_err(|e| {
            format!(
                "Failed to run `agy`: {}. Make sure the agy CLI is installed and available in PATH.",
                e
            )
        })?;

    if debug && !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            format!(
                "`agy -p \"/usage\" --output-format json` failed with {}",
                output.status
            )
            .into()
        } else {
            format!(
                "`agy -p \"/usage\" --output-format json` failed: {}",
                stderr
            )
            .into()
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: serde_json::Value = serde_json::from_str(&stdout).map_err(|e| {
        format!(
            "Failed to parse `agy` output as JSON: {}. Output: {}",
            e,
            stdout.trim()
        )
    })?;
    let parsed: UsageOutput = serde_json::from_value(raw.clone())
        .map_err(|e| format!("Unexpected `agy` output: {}", e))?;

    if !parsed.status.is_empty() && parsed.status != "SUCCESS" {
        let message = parsed.response.trim();
        return Err(if message.is_empty() {
            format!("`agy /usage` returned status {}", parsed.status).into()
        } else {
            format!(
                "`agy /usage` returned status {}: {}",
                parsed.status, message
            )
            .into()
        });
    }

    Ok(UsageReport {
        raw,
        output: parsed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OUTPUT: &str = r#"{
        "conversation_id": "",
        "status": "SUCCESS",
        "response": "Gemini Models\tWeekly Limit Remaining\t99%\t2026-11-07T13:04:58Z\n",
        "command": {
            "name": "usage",
            "data": {
                "description": "Shared quota description",
                "groups": [
                    {
                        "name": "Gemini Models",
                        "description": "Models within this group: Gemini Flash, Gemini Pro",
                        "buckets": [
                            {
                                "id": "gemini-weekly",
                                "name": "Weekly Limit Remaining",
                                "description": "You have used some of your weekly limit.",
                                "window": "weekly",
                                "remaining_fraction": 0.9946703314781189,
                                "reset_time": "2026-11-07T13:04:58Z"
                            },
                            {
                                "id": "gemini-5h",
                                "name": "Five Hour Limit Remaining",
                                "window": "5h",
                                "remaining_fraction": 0.9897106289863586,
                                "reset_time": "2026-11-07T03:45:34Z"
                            }
                        ]
                    },
                    {
                        "name": "Claude and GPT models",
                        "description": "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
                        "buckets": [
                            {
                                "id": "3p-weekly",
                                "name": "Weekly Limit Remaining",
                                "window": "weekly",
                                "remaining_fraction": 1,
                                "reset_time": "2026-11-13T22:56:42Z"
                            }
                        ]
                    }
                ]
            }
        }
    }"#;

    #[test]
    fn test_parse_usage_output() {
        let output: UsageOutput = serde_json::from_str(SAMPLE_OUTPUT).unwrap();

        assert_eq!(output.status, "SUCCESS");
        assert_eq!(output.description(), Some("Shared quota description"));

        let groups = output.groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "Gemini Models");
        assert_eq!(groups[0].buckets.len(), 2);
        let weekly_fraction = groups[0].buckets[0].remaining_fraction.unwrap();
        assert!((weekly_fraction - 0.9946703314781189).abs() < 1e-12);
        assert_eq!(groups[1].name, "Claude and GPT models");
        assert_eq!(groups[1].buckets[0].remaining_fraction, Some(1.0));
    }

    #[test]
    fn test_parse_response_fallback() {
        let output = UsageOutput {
            status: "SUCCESS".to_string(),
            response: "Gemini Models\tWeekly Limit Remaining\t99%\t2026-11-07T13:04:58Z\n\
                       Gemini Models\tFive Hour Limit Remaining\t50%\t2026-11-07T03:45:34Z\n\
                       Claude and GPT models\tWeekly Limit Remaining\t100%\t2026-11-13T22:56:42Z\n"
                .to_string(),
            command: None,
        };

        let groups = output.groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "Gemini Models");
        assert_eq!(groups[0].buckets.len(), 2);
        assert_eq!(groups[0].buckets[0].remaining_fraction, Some(0.99));
        assert_eq!(groups[0].buckets[1].remaining_fraction, Some(0.5));
        assert_eq!(groups[1].name, "Claude and GPT models");
    }

    #[test]
    fn test_parse_percentage() {
        assert_eq!(parse_percentage("99%"), Some(0.99));
        assert_eq!(parse_percentage("100%"), Some(1.0));
        assert_eq!(parse_percentage("0%"), Some(0.0));
        assert_eq!(parse_percentage("N/A"), None);
    }

    #[test]
    fn test_parse_models_output() {
        let stdout = "gemini-3.8-flash-low\tGemini 3.8 Flash (Low)\n\
                      claude-sonnet-5-5-low\tClaude Sonnet 5.5 (Low)\n\
                      some informational line without a tab\n";
        assert_eq!(
            parse_models_output(stdout),
            vec!["gemini-3.8-flash-low", "claude-sonnet-5-5-low"]
        );
    }

    #[test]
    fn test_parse_trigger_output() {
        let output: TriggerOutput = serde_json::from_str(
            r#"{
                "status": "SUCCESS",
                "response": "hello",
                "duration_seconds": 1.25,
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 2,
                    "thinking_tokens": 1,
                    "cache_read_tokens": 0,
                    "total_tokens": 12
                }
            }"#,
        )
        .unwrap();

        assert_eq!(output.status, "SUCCESS");
        assert_eq!(output.response, "hello");
        assert_eq!(output.duration_seconds, 1.25);
        assert_eq!(output.usage.unwrap().total_tokens, 12);
    }

    #[test]
    fn test_parse_agy_account() {
        let account = parse_account(
            r#"{
                "auth_method": "consumer",
                "id_token_email": "user@example.com",
                "token": {"access_token": "secret"}
            }"#,
        )
        .unwrap();

        assert_eq!(account.email(), Some("user@example.com"));
        assert_eq!(account.auth_method.as_deref(), Some("consumer"));
    }

    #[test]
    fn test_parse_agy_account_without_email() {
        let account =
            parse_account(r#"{"auth_method": "api_key", "token": {"access_token": "secret"}}"#)
                .unwrap();
        assert_eq!(account.email(), None);
        assert_eq!(account.auth_method.as_deref(), Some("api_key"));

        assert!(parse_account(r#"{"auth_method": "consumer"}"#).is_none());
        assert!(parse_account(r#"{"auth_method": "api_key", "token": {}}"#).is_none());
        assert!(parse_account(r#"{"id_token_email": ""}"#).is_none());
        assert!(parse_account("not json").is_none());
    }
}
