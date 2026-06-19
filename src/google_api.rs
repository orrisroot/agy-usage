use crate::config::StoredTokens;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub const OAUTH_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
pub const OAUTH_CLIENT_SECRET: &str = "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";
pub const OAUTH_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub const CLOUDCODE_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
pub const USER_AGENT: &str = "antigravity";

#[derive(Deserialize, Debug)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
}

#[derive(Deserialize, Debug)]
pub struct UserInfoResponse {
    pub email: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelQuotaInfo {
    #[serde(rename = "remainingFraction")]
    pub remaining_fraction: Option<f64>,
    #[serde(rename = "resetTime")]
    pub reset_time: Option<String>,
    #[serde(rename = "isExhausted")]
    pub is_exhausted: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelInfo {
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub label: Option<String>,
    #[serde(rename = "quotaInfo")]
    pub quota_info: Option<ModelQuotaInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FetchAvailableModelsResponse {
    pub models: Option<HashMap<String, ModelInfo>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlanInfo {
    #[serde(rename = "monthlyPromptCredits")]
    pub monthly_prompt_credits: Option<u64>,
    #[serde(rename = "planType")]
    pub plan_type: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tier {
    pub id: Option<String>,
    #[serde(rename = "isDefault")]
    pub is_default: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PaidTier {
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CurrentTier {
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoadCodeAssistResponse {
    #[serde(rename = "cloudaicompanionProject")]
    pub cloudaicompanion_project: Option<serde_json::Value>,
    #[serde(rename = "availablePromptCredits")]
    pub available_prompt_credits: Option<u64>,
    #[serde(rename = "planInfo")]
    pub plan_info: Option<PlanInfo>,
    #[serde(rename = "allowedTiers")]
    pub allowed_tiers: Option<Vec<Tier>>,
    #[serde(rename = "paidTier")]
    pub paid_tier: Option<PaidTier>,
    #[serde(rename = "currentTier")]
    pub current_tier: Option<CurrentTier>,
}

pub fn extract_project_id(val: &serde_json::Value) -> Option<String> {
    match val {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(s)) = map.get("id") {
                Some(s.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

pub struct ApiResponse {
    pub status: reqwest::StatusCode,
    #[allow(dead_code)]
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
}

impl ApiResponse {
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn error_for_status(self) -> Result<Self, Box<dyn std::error::Error>> {
        if self.status.is_client_error() || self.status.is_server_error() {
            let body_str = self.text();
            Err(format!("HTTP {} - {}", self.status, body_str).into())
        } else {
            Ok(self)
        }
    }
}

pub async fn execute_request(
    builder: reqwest::RequestBuilder,
    body_for_log: Option<String>,
    debug: bool,
) -> Result<ApiResponse, reqwest::Error> {
    let request = builder.build()?;
    if debug {
        eprintln!("\n\x1b[33;1m--- API Request ---\x1b[0m");
        eprintln!("Method: {}", request.method());
        eprintln!("URL: {}", request.url());
        eprintln!("Headers:");
        for (name, value) in request.headers() {
            if name == reqwest::header::AUTHORIZATION {
                if let Ok(val_str) = value.to_str() {
                    if val_str.starts_with("Bearer ") {
                        let token = &val_str[7..];
                        let masked = if token.len() > 12 {
                            format!("Bearer {}...{}", &token[..6], &token[token.len() - 6..])
                        } else {
                            "Bearer ***".to_string()
                        };
                        eprintln!("  {}: {}", name, masked);
                    } else {
                        eprintln!("  {}: ***", name);
                    }
                } else {
                    eprintln!("  {}: ***", name);
                }
            } else {
                eprintln!("  {}: {:?}", name, value);
            }
        }
        if let Some(body) = body_for_log {
            let mut sanitized = body;
            if sanitized.contains(OAUTH_CLIENT_SECRET) {
                sanitized = sanitized.replace(OAUTH_CLIENT_SECRET, "GOCSPX-***");
            }
            eprintln!("Body: {}", sanitized);
        } else if let Some(body) = request.body() {
            if let Some(bytes) = body.as_bytes() {
                if let Ok(s) = std::str::from_utf8(bytes) {
                    let mut sanitized = s.to_string();
                    if sanitized.contains(OAUTH_CLIENT_SECRET) {
                        sanitized = sanitized.replace(OAUTH_CLIENT_SECRET, "GOCSPX-***");
                    }
                    eprintln!("Body: {}", sanitized);
                }
            }
        }
        eprintln!("\x1b[33;1m-------------------\x1b[0m");
    }

    let client = reqwest::Client::new();
    let response = client.execute(request).await?;

    let status = response.status();
    let headers = response.headers().clone();
    let body_bytes = response.bytes().await?;
    let body = body_bytes.to_vec();

    if debug {
        eprintln!("\n\x1b[32;1m--- API Response ---\x1b[0m");
        eprintln!("Status: {}", status);
        eprintln!("Headers:");
        for (name, value) in &headers {
            eprintln!("  {}: {:?}", name, value);
        }
        if let Ok(s) = std::str::from_utf8(&body) {
            eprintln!("Body: {}", s);
        } else {
            eprintln!("Body: <binary/non-utf8>");
        }
        eprintln!("\x1b[32;1m--------------------\x1b[0m");
    }

    Ok(ApiResponse {
        status,
        headers,
        body,
    })
}

pub async fn exchange_code_for_tokens(
    code: &str,
    redirect_uri: &str,
    debug: bool,
) -> Result<OAuthTokenResponse, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let params = [
        ("code", code),
        ("client_id", OAUTH_CLIENT_ID),
        ("client_secret", OAUTH_CLIENT_SECRET),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    let builder = client.post(OAUTH_TOKEN_URL).form(&params);

    let log_body = format!(
        "code={}&client_id={}&client_secret=GOCSPX-***&redirect_uri={}&grant_type=authorization_code",
        code, OAUTH_CLIENT_ID, redirect_uri
    );
    let res = execute_request(builder, Some(log_body), debug).await?;
    let res = res.error_for_status()?;
    let parsed = res.json::<OAuthTokenResponse>()?;
    Ok(parsed)
}

pub async fn refresh_access_token(
    refresh_token: &str,
    debug: bool,
) -> Result<OAuthTokenResponse, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let params = [
        ("refresh_token", refresh_token),
        ("client_id", OAUTH_CLIENT_ID),
        ("client_secret", OAUTH_CLIENT_SECRET),
        ("grant_type", "refresh_token"),
    ];
    let builder = client.post(OAUTH_TOKEN_URL).form(&params);

    let log_body = format!(
        "refresh_token=***&client_id={}&client_secret=GOCSPX-***&grant_type=refresh_token",
        OAUTH_CLIENT_ID
    );
    let res = execute_request(builder, Some(log_body), debug).await?;
    let res = res.error_for_status()?;
    let parsed = res.json::<OAuthTokenResponse>()?;
    Ok(parsed)
}

pub async fn get_user_email(
    access_token: &str,
    debug: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let builder = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token);

    let res = execute_request(builder, None, debug).await?;
    let res = res.error_for_status()?;
    let parsed = res.json::<UserInfoResponse>()?;
    Ok(parsed.email)
}

pub async fn load_code_assist(
    access_token: &str,
    debug: bool,
) -> Result<LoadCodeAssistResponse, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "metadata": {
            "ideType": "ANTIGRAVITY",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI"
        }
    });

    let builder = client
        .post(format!("{}/v1internal:loadCodeAssist", CLOUDCODE_BASE_URL))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .json(&payload);

    let res = execute_request(builder, Some(payload.to_string()), debug).await?;
    let res = res.error_for_status()?;
    let parsed = res.json::<LoadCodeAssistResponse>()?;
    Ok(parsed)
}

pub async fn fetch_available_models(
    access_token: &str,
    project_id: Option<&str>,
    debug: bool,
) -> Result<FetchAvailableModelsResponse, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let payload = if let Some(proj) = project_id {
        serde_json::json!({ "project": proj })
    } else {
        serde_json::json!({})
    };

    let builder = client
        .post(format!(
            "{}/v1internal:fetchAvailableModels",
            CLOUDCODE_BASE_URL
        ))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .json(&payload);

    let res = execute_request(builder, Some(payload.to_string()), debug).await?;
    let res = res.error_for_status()?;
    let parsed = res.json::<FetchAvailableModelsResponse>()?;
    Ok(parsed)
}

pub async fn try_onboard_user(
    access_token: &str,
    tier_id: &str,
    debug: bool,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "tierId": tier_id,
        "metadata": {
            "ideType": "ANTIGRAVITY",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI"
        }
    });

    let builder = client
        .post(format!("{}/v1internal:onboardUser", CLOUDCODE_BASE_URL))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .json(&payload);

    let res = execute_request(builder, Some(payload.to_string()), debug).await?;
    if !res.status.is_success() {
        return Ok(None);
    }

    #[derive(Deserialize)]
    struct OnboardResponse {
        done: Option<bool>,
        response: Option<serde_json::Value>,
    }

    if let Ok(onboard_res) = res.json::<OnboardResponse>() {
        if onboard_res.done == Some(true) {
            if let Some(resp) = onboard_res.response {
                if let Some(proj) = resp.get("cloudaicompanionProject") {
                    return Ok(extract_project_id(proj));
                }
            }
        }
    }
    Ok(None)
}

pub fn pick_onboard_tier(
    response: &LoadCodeAssistResponse,
    default_tier: Option<&str>,
) -> Option<String> {
    if let Some(ref tiers) = response.allowed_tiers {
        // Find default tier
        if let Some(t) = tiers.iter().find(|t| t.is_default == Some(true)) {
            if let Some(ref id) = t.id {
                return Some(id.clone());
            }
        }
        // Find first tier
        if let Some(t) = tiers.first() {
            if let Some(ref id) = t.id {
                return Some(id.clone());
            }
        }
        if !tiers.is_empty() {
            return Some("LEGACY".to_string());
        }
    }
    default_tier.map(|s| s.to_string())
}

pub async fn resolve_project_id(
    access_token: &str,
    cached_project_id: Option<&str>,
    debug: bool,
) -> Option<String> {
    if let Some(p) = cached_project_id {
        if !p.is_empty() {
            return Some(p.to_string());
        }
    }

    let load_resp = match load_code_assist(access_token, debug).await {
        Ok(resp) => resp,
        Err(_) => return None,
    };

    if let Some(ref proj_val) = load_resp.cloudaicompanion_project {
        if let Some(p) = extract_project_id(proj_val) {
            return Some(p);
        }
    }

    // Attempt onboarding
    let tier_id = load_resp
        .paid_tier
        .as_ref()
        .and_then(|t| t.id.clone())
        .or_else(|| load_resp.current_tier.as_ref().and_then(|t| t.id.clone()));

    let onboard_tier = pick_onboard_tier(&load_resp, tier_id.as_deref())?;

    if let Ok(Some(proj_id)) = try_onboard_user(access_token, &onboard_tier, debug).await {
        return Some(proj_id);
    }

    // Poll loadCodeAssist with retries
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(2000)).await;
        if let Ok(resp) = load_code_assist(access_token, debug).await {
            if let Some(ref proj_val) = resp.cloudaicompanion_project {
                if let Some(p) = extract_project_id(proj_val) {
                    return Some(p);
                }
            }
        }
    }

    None
}

pub async fn get_valid_tokens(
    tokens: &mut StoredTokens,
    debug: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    // Expiry buffer is 5 minutes (300,000 ms)
    let buffer = 5 * 60 * 1000;
    if now >= tokens.expires_at - buffer {
        // Refresh token
        let res = refresh_access_token(&tokens.refresh_token, debug).await?;
        tokens.access_token = res.access_token;
        if let Some(rt) = res.refresh_token {
            tokens.refresh_token = rt;
        }
        tokens.expires_at = now + res.expires_in * 1000;

        // Save back
        crate::config::save_account_tokens(&tokens.email, tokens)?;
    }
    Ok(tokens.access_token.clone())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TriggerTokenUsage {
    pub prompt: u32,
    pub completion: u32,
    pub total: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TriggerResult {
    pub success: bool,
    pub duration_ms: u128,
    pub text: String,
    pub token_usage: Option<TriggerTokenUsage>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TriggerOptions {
    pub model: String,
    pub prompt: String,
    pub max_output_tokens: Option<u32>,
    pub use_system_instruction: bool,
    pub project_id: Option<String>,
}

// Structs for request serialization
#[derive(Serialize, Debug)]
pub struct Part {
    pub text: String,
}

#[derive(Serialize, Debug)]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

#[derive(Serialize, Debug)]
pub struct SystemInstruction {
    pub parts: Vec<Part>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    pub temperature: f64,
    pub max_output_tokens: Option<u32>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequestDetails {
    pub contents: Vec<Content>,
    #[serde(rename = "session_id")]
    pub session_id: String,
    pub system_instruction: Option<SystemInstruction>,
    pub generation_config: GenerationConfig,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequest {
    pub request_id: String,
    pub model: String,
    pub user_agent: String,
    pub request_type: String,
    pub project: Option<String>,
    pub request: AgentRequestDetails,
}

// SSE parser structures
#[derive(Deserialize, Debug)]
pub struct SSEUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    pub prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    pub candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    pub total_token_count: Option<u32>,
}

#[derive(Deserialize, Debug)]
pub struct SSEPart {
    pub text: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct SSEContent {
    pub parts: Option<Vec<SSEPart>>,
}

#[derive(Deserialize, Debug)]
pub struct SSECandidate {
    pub content: Option<SSEContent>,
}

#[derive(Deserialize, Debug)]
pub struct SSEResponsePayload {
    pub candidates: Option<Vec<SSECandidate>>,
    #[serde(rename = "usageMetadata")]
    pub usage_metadata: Option<SSEUsageMetadata>,
}

#[derive(Deserialize, Debug)]
pub struct SSERoot {
    pub response: Option<SSEResponsePayload>,
    pub candidates: Option<Vec<SSECandidate>>,
    #[serde(rename = "usageMetadata")]
    pub usage_metadata: Option<SSEUsageMetadata>,
}

fn generate_uuid() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // UUID v4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant RFC 4122
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

pub async fn trigger_model(
    access_token: &str,
    options: &TriggerOptions,
    debug: bool,
) -> Result<TriggerResult, Box<dyn std::error::Error>> {
    use rand::RngExt;
    let start_time = std::time::Instant::now();

    // 1. Warm up session
    let _ = load_code_assist(access_token, debug).await;

    // 2. Build Request Body
    let request_id = generate_uuid();
    let session_id = generate_uuid();

    let system_instruction = if options.use_system_instruction {
        Some(SystemInstruction {
            parts: vec![Part {
                text: "You are Antigravity, a powerful agentic AI coding assistant designed by the Google Deepmind team working on Advanced Agentic Coding. You are pair programming with a USER to solve their coding task. The task may require creating a new codebase, modifying or debugging an existing codebase, or simply answering a question.**Absolute paths only****Proactiveness**".to_string(),
            }],
        })
    } else {
        None
    };

    let payload = AgentRequest {
        request_id,
        model: options.model.clone(),
        user_agent: USER_AGENT.to_string(),
        request_type: "agent".to_string(),
        project: options.project_id.clone(),
        request: AgentRequestDetails {
            contents: vec![Content {
                role: "user".to_string(),
                parts: vec![Part {
                    text: options.prompt.clone(),
                }],
            }],
            session_id,
            system_instruction,
            generation_config: GenerationConfig {
                temperature: 0.0,
                max_output_tokens: options.max_output_tokens,
            },
        },
    };

    let base_urls = [
        "https://cloudcode-pa.googleapis.com",
        "https://daily-cloudcode-pa.sandbox.googleapis.com",
    ];

    let stream_path = "/v1internal:streamGenerateContent?alt=sse";

    let mut last_error = None;

    for base_url in &base_urls {
        let url = format!("{}{}", base_url, stream_path);

        // Retry logic: 3 attempts per URL
        for attempt in 1..=3 {
            if attempt > 1 {
                // Backoff delay
                let delay = 500 * (1 << (attempt - 2)) + rand::rng().random_range(0..100);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }

            let client = reqwest::Client::new();
            let builder = client
                .post(&url)
                .bearer_auth(access_token)
                .header("User-Agent", USER_AGENT)
                .header("Content-Type", "application/json")
                .header("Accept-Encoding", "gzip")
                .json(&payload);

            let payload_str = serde_json::to_string(&payload).unwrap_or_default();
            match execute_request(builder, Some(payload_str), debug).await {
                Ok(res) => {
                    let status = res.status;
                    if status.is_success() {
                        let text = res.text();
                        // Parse SSE response
                        let mut full_text = String::new();
                        let mut usage = None;

                        for line in text.lines() {
                            if line.starts_with("data: ") {
                                let json_str = &line[6..];
                                if json_str.trim() == "[DONE]" {
                                    continue;
                                }
                                if let Ok(root) = serde_json::from_str::<SSERoot>(json_str) {
                                    let candidates = root
                                        .response
                                        .as_ref()
                                        .and_then(|r| r.candidates.as_ref())
                                        .or(root.candidates.as_ref());
                                    if let Some(candidates) = candidates {
                                        for cand in candidates {
                                            if let Some(ref content) = cand.content {
                                                if let Some(ref parts) = content.parts {
                                                    for part in parts {
                                                        if let Some(ref t) = part.text {
                                                            full_text.push_str(t);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    let metadata = root
                                        .response
                                        .as_ref()
                                        .and_then(|r| r.usage_metadata.as_ref())
                                        .or(root.usage_metadata.as_ref());
                                    if let Some(m) = metadata {
                                        usage = Some(TriggerTokenUsage {
                                            prompt: m.prompt_token_count.unwrap_or(0),
                                            completion: m.candidates_token_count.unwrap_or(0),
                                            total: m.total_token_count.unwrap_or(0),
                                        });
                                    }
                                }
                            }
                        }

                        return Ok(TriggerResult {
                            success: true,
                            duration_ms: start_time.elapsed().as_millis(),
                            text: full_text,
                            token_usage: usage,
                            error: None,
                        });
                    } else {
                        let err_text = res.text();
                        last_error = Some(format!("HTTP {} - {}", status, err_text));
                        if status == 429 || status.is_server_error() {
                            // Retry
                            continue;
                        } else {
                            // Non-retryable error
                            break;
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }
        }
    }

    let err_msg = last_error.unwrap_or_else(|| "All trigger attempts failed".to_string());
    Ok(TriggerResult {
        success: false,
        duration_ms: start_time.elapsed().as_millis(),
        text: String::new(),
        token_usage: None,
        error: Some(err_msg),
    })
}
