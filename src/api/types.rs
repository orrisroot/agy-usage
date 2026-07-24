use serde::{Deserialize, Serialize};

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CachedQuotaSummary {
    pub response: RetrieveUserQuotaSummaryResponse,
    pub fetched_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CachedAvailableModels {
    pub response: serde_json::Value,
    pub fetched_at: u64,
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
