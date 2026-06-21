use crate::config::StoredTokens;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const OAUTH_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
pub const OAUTH_CLIENT_SECRET: &str = "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";
pub const OAUTH_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub const CLOUDCODE_BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
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
pub struct QuotaSummaryBucket {
    #[serde(rename = "bucketId")]
    pub bucket_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub window: Option<String>,
    pub remaining: Option<i64>,
    #[serde(rename = "remainingFraction")]
    pub remaining_fraction: Option<f64>,
    #[serde(rename = "remainingAmount")]
    pub remaining_amount: Option<f64>,
    pub disabled: Option<bool>,
    #[serde(rename = "resetTime")]
    pub reset_time: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QuotaSummaryGroup {
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub buckets: Option<Vec<QuotaSummaryBucket>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RetrieveUserQuotaSummaryResponse {
    pub buckets: Option<Vec<QuotaSummaryBucket>>,
    pub groups: Option<Vec<QuotaSummaryGroup>>,
    pub description: Option<String>,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CachedCodeAssist {
    pub response: LoadCodeAssistResponse,
    pub fetched_at: u64,
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

async fn execute_request_internal(
    client: &reqwest::Client,
    builder: reqwest::RequestBuilder,
    body_for_log: Option<String>,
    debug: bool,
) -> Result<ApiResponse, reqwest::Error> {
    let request = builder.build()?;
    if debug {
        eprintln!("\n{}", "--- API Request ---".yellow().bold());
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
            eprintln!("Body: {}", sanitize_body(&body));
        } else if let Some(body) = request.body() {
            if let Some(bytes) = body.as_bytes() {
                if let Ok(s) = std::str::from_utf8(bytes) {
                    eprintln!("Body: {}", sanitize_body(s));
                }
            }
        }
        eprintln!("{}", "-------------------".yellow().bold());
    }

    let response = client.execute(request).await?;

    let status = response.status();
    let headers = response.headers().clone();
    let body_bytes = response.bytes().await?;
    let body = body_bytes.to_vec();

    if debug {
        eprintln!("\n{}", "--- API Response ---".green().bold());
        eprintln!("Status: {}", status);
        eprintln!("Headers:");
        for (name, value) in &headers {
            eprintln!("  {}: {:?}", name, value);
        }
        if let Ok(s) = std::str::from_utf8(&body) {
            eprintln!("Body: {}", sanitize_body(s));
        } else {
            eprintln!("Body: <binary/non-utf8>");
        }
        eprintln!("{}", "--------------------".green().bold());
    }

    Ok(ApiResponse {
        status,
        headers,
        body,
    })
}

pub async fn execute_request(
    builder: reqwest::RequestBuilder,
    body_for_log: Option<String>,
    debug: bool,
) -> Result<ApiResponse, reqwest::Error> {
    let client = reqwest::Client::new();
    execute_request_internal(&client, builder, body_for_log, debug).await
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

#[derive(Clone, Debug)]
pub struct ApiClient {
    client: reqwest::Client,
    tokens: StoredTokens,
    debug: bool,
}

impl ApiClient {
    pub fn new(tokens: StoredTokens, debug: bool) -> Self {
        Self {
            client: reqwest::Client::new(),
            tokens,
            debug,
        }
    }

    pub fn tokens(&self) -> &StoredTokens {
        &self.tokens
    }

    pub async fn ensure_valid_token(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        // Expiry buffer is 5 minutes (300,000 ms)
        let buffer = 5 * 60 * 1000;
        if now >= self.tokens.expires_at - buffer {
            // Refresh token
            let res = refresh_access_token(&self.tokens.refresh_token, self.debug).await?;
            self.tokens.access_token = res.access_token;
            if let Some(rt) = res.refresh_token {
                self.tokens.refresh_token = rt;
            }
            self.tokens.expires_at = now + res.expires_in * 1000;
            crate::config::save_account_tokens(&self.tokens.email, &self.tokens)?;
        }
        Ok(self.tokens.access_token.clone())
    }

    async fn execute_request(
        &self,
        builder: reqwest::RequestBuilder,
        body_for_log: Option<String>,
    ) -> Result<ApiResponse, reqwest::Error> {
        execute_request_internal(&self.client, builder, body_for_log, self.debug).await
    }

    pub async fn load_code_assist(
        &mut self,
    ) -> Result<LoadCodeAssistResponse, Box<dyn std::error::Error>> {
        let cache_path =
            crate::config::get_account_dir(&self.tokens.email).join("code_assist_cache.json");
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let cache_ttl = 5 * 60 * 1000; // 5 minutes

        if let Ok(content) = std::fs::read_to_string(&cache_path) {
            if let Ok(cached) = serde_json::from_str::<CachedCodeAssist>(&content) {
                if now < cached.fetched_at + cache_ttl {
                    return Ok(cached.response);
                }
            }
        }

        let access_token = self.ensure_valid_token().await?;
        let payload = serde_json::json!({
            "metadata": {
                "ideType": "ANTIGRAVITY",
                "platform": "PLATFORM_UNSPECIFIED",
                "pluginType": "GEMINI"
            }
        });

        let client = self.client.clone();
        let builder = client
            .post(format!("{}/v1internal:loadCodeAssist", CLOUDCODE_BASE_URL))
            .bearer_auth(access_token)
            .header("User-Agent", USER_AGENT)
            .json(&payload);

        let res = self
            .execute_request(builder, Some(payload.to_string()))
            .await?;
        let res = res.error_for_status()?;
        let parsed = res.json::<LoadCodeAssistResponse>()?;

        // Save to cache
        let cached = CachedCodeAssist {
            response: parsed.clone(),
            fetched_at: now,
        };
        if let Ok(content) = serde_json::to_string(&cached) {
            if let Err(e) = std::fs::write(&cache_path, content) {
                eprintln!("Warning: Failed to write code assist cache: {}", e);
            }
        }

        Ok(parsed)
    }

    pub async fn retrieve_user_quota_summary(
        &mut self,
    ) -> Result<RetrieveUserQuotaSummaryResponse, Box<dyn std::error::Error>> {
        let access_token = self.ensure_valid_token().await?;
        let payload = if let Some(ref proj) = self.tokens.project_id {
            serde_json::json!({ "project": proj })
        } else {
            serde_json::json!({})
        };

        let client = self.client.clone();
        let builder = client
            .post(format!(
                "{}/v1internal:retrieveUserQuotaSummary",
                CLOUDCODE_BASE_URL
            ))
            .bearer_auth(access_token)
            .header("User-Agent", USER_AGENT)
            .json(&payload);

        let res = self
            .execute_request(builder, Some(payload.to_string()))
            .await?;
        let res = res.error_for_status()?;
        let parsed = res.json::<RetrieveUserQuotaSummaryResponse>()?;
        Ok(parsed)
    }

    pub async fn try_onboard_user(
        &mut self,
        tier_id: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let access_token = self.ensure_valid_token().await?;
        let payload = serde_json::json!({
            "tierId": tier_id,
            "metadata": {
                "ideType": "ANTIGRAVITY",
                "platform": "PLATFORM_UNSPECIFIED",
                "pluginType": "GEMINI"
            }
        });

        let client = self.client.clone();
        let builder = client
            .post(format!("{}/v1internal:onboardUser", CLOUDCODE_BASE_URL))
            .bearer_auth(access_token)
            .header("User-Agent", USER_AGENT)
            .json(&payload);

        let res = self
            .execute_request(builder, Some(payload.to_string()))
            .await?;
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

    pub async fn resolve_project_id(&mut self) -> Option<String> {
        let cached_p = self.tokens.project_id.clone();

        let load_resp = match self.load_code_assist().await {
            Ok(resp) => resp,
            Err(_) => return None,
        };

        let target_tier_id = load_resp
            .paid_tier
            .as_ref()
            .and_then(|t| t.id.clone())
            .or_else(|| load_resp.current_tier.as_ref().and_then(|t| t.id.clone()));

        let current_tier_id = load_resp.current_tier.as_ref().and_then(|t| t.id.clone());

        if target_tier_id == current_tier_id {
            if let Some(ref proj_val) = load_resp.cloudaicompanion_project {
                if let Some(p) = extract_project_id(proj_val) {
                    if Some(&p) != cached_p.as_ref() {
                        self.tokens.project_id = Some(p.clone());
                        let _ =
                            crate::config::save_account_tokens(&self.tokens.email, &self.tokens);
                    }
                    return Some(p);
                }
            }
        }

        // Attempt onboarding
        let onboard_tier = pick_onboard_tier(&load_resp, target_tier_id.as_deref())?;

        if let Ok(Some(proj_id)) = self.try_onboard_user(&onboard_tier).await {
            self.tokens.project_id = Some(proj_id.clone());
            let _ = crate::config::save_account_tokens(&self.tokens.email, &self.tokens);
            return Some(proj_id);
        }

        // Poll loadCodeAssist with retries
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(2000)).await;
            if let Ok(resp) = self.load_code_assist().await {
                if let Some(ref proj_val) = resp.cloudaicompanion_project {
                    if let Some(p) = extract_project_id(proj_val) {
                        self.tokens.project_id = Some(p.clone());
                        let _ =
                            crate::config::save_account_tokens(&self.tokens.email, &self.tokens);
                        return Some(p);
                    }
                }
            }
        }

        None
    }

    pub async fn trigger_model(
        &mut self,
        options: &TriggerOptions,
    ) -> Result<TriggerResult, Box<dyn std::error::Error>> {
        use rand::RngExt;
        let start_time = std::time::Instant::now();

        // 1. Warm up session
        let _ = self.load_code_assist().await;

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

        let stream_path = "/v1internal:streamGenerateContent?alt=sse";

        let mut last_error = None;

        let url = format!("{}{}", CLOUDCODE_BASE_URL, stream_path);

        // Retry logic: 3 attempts per URL
        for attempt in 1..=3 {
            if attempt > 1 {
                // Backoff delay
                let delay = 500 * (1 << (attempt - 2)) + rand::rng().random_range(0..100);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }

            let access_token = self.ensure_valid_token().await?;
            let client = self.client.clone();
            let builder = client
                .post(&url)
                .bearer_auth(access_token)
                .header("User-Agent", USER_AGENT)
                .header("Content-Type", "application/json")
                .header("Accept-Encoding", "gzip")
                .json(&payload);

            let payload_str = serde_json::to_string(&payload).unwrap_or_default();
            match self.execute_request(builder, Some(payload_str)).await {
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

        let err_msg = last_error.unwrap_or_else(|| "All trigger attempts failed".to_string());
        Ok(TriggerResult {
            success: false,
            duration_ms: start_time.elapsed().as_millis(),
            text: String::new(),
            token_usage: None,
            error: Some(err_msg),
        })
    }
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

pub fn sanitize_body(body: &str) -> String {
    // Try parsing as JSON first
    if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(body) {
        sanitize_json_value(&mut val);
        return serde_json::to_string(&val).unwrap_or_else(|_| body.to_string());
    }

    // Try parsing as form urlencoded
    if body.contains('=') && !body.contains('{') && !body.contains('[') {
        let mut params = Vec::new();
        let mut modified = false;
        for (k, v) in url::form_urlencoded::parse(body.as_bytes()) {
            if k == "access_token" || k == "refresh_token" || k == "client_secret" || k == "code" {
                params.push((k.into_owned(), "***".to_string()));
                modified = true;
            } else {
                params.push((k.into_owned(), v.into_owned()));
            }
        }
        if modified {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in params {
                serializer.append_pair(&k, &v);
            }
            return serializer.finish();
        }
    }

    body.to_string()
}

fn sanitize_json_value(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                let is_sensitive = k == "access_token"
                    || k == "accessToken"
                    || k == "refresh_token"
                    || k == "refreshToken"
                    || k == "client_secret"
                    || k == "clientSecret"
                    || k == "code";
                if is_sensitive {
                    *v = serde_json::Value::String("***".to_string());
                } else {
                    sanitize_json_value(v);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                sanitize_json_value(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_json() {
        let input = r#"{"access_token":"secret123","refresh_token":"ref456","other":"public"}"#;
        let sanitized = sanitize_body(input);
        let parsed: serde_json::Value = serde_json::from_str(&sanitized).unwrap();
        assert_eq!(parsed["access_token"], "***");
        assert_eq!(parsed["refresh_token"], "***");
        assert_eq!(parsed["other"], "public");
    }

    #[test]
    fn test_sanitize_nested_json() {
        let input = r#"{"nested":{"accessToken":"secret123"},"list":[{"code":"mycode"}]}"#;
        let sanitized = sanitize_body(input);
        let parsed: serde_json::Value = serde_json::from_str(&sanitized).unwrap();
        assert_eq!(parsed["nested"]["accessToken"], "***");
        assert_eq!(parsed["list"][0]["code"], "***");
    }

    #[test]
    fn test_sanitize_form_urlencoded() {
        let input = "code=mycode&client_id=123&client_secret=secret&other=val";
        let sanitized = sanitize_body(input);
        let params: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(sanitized.as_bytes())
                .into_owned()
                .collect();
        assert_eq!(params.get("code").unwrap(), "***");
        assert_eq!(params.get("client_id").unwrap(), "123");
        assert_eq!(params.get("client_secret").unwrap(), "***");
        assert_eq!(params.get("other").unwrap(), "val");
    }
}
