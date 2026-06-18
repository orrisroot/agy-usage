use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use crate::config::StoredTokens;

pub const OAUTH_CLIENT_ID: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
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

pub async fn exchange_code_for_tokens(code: &str, redirect_uri: &str) -> Result<OAuthTokenResponse, reqwest::Error> {
    let client = reqwest::Client::new();
    let params = [
        ("code", code),
        ("client_id", OAUTH_CLIENT_ID),
        ("client_secret", OAUTH_CLIENT_SECRET),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    let res = client
        .post(OAUTH_TOKEN_URL)
        .form(&params)
        .send()
        .await?
        .error_for_status()?
        .json::<OAuthTokenResponse>()
        .await?;
    Ok(res)
}

pub async fn refresh_access_token(refresh_token: &str) -> Result<OAuthTokenResponse, reqwest::Error> {
    let client = reqwest::Client::new();
    let params = [
        ("refresh_token", refresh_token),
        ("client_id", OAUTH_CLIENT_ID),
        ("client_secret", OAUTH_CLIENT_SECRET),
        ("grant_type", "refresh_token"),
    ];
    let res = client
        .post(OAUTH_TOKEN_URL)
        .form(&params)
        .send()
        .await?
        .error_for_status()?
        .json::<OAuthTokenResponse>()
        .await?;
    Ok(res)
}

pub async fn get_user_email(access_token: &str) -> Result<String, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json::<UserInfoResponse>()
        .await?;
    Ok(res.email)
}

pub async fn load_code_assist(access_token: &str) -> Result<LoadCodeAssistResponse, reqwest::Error> {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "metadata": {
            "ideType": "ANTIGRAVITY",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI"
        }
    });
    
    let res = client
        .post(format!("{}/v1internal:loadCodeAssist", CLOUDCODE_BASE_URL))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .json(&payload)
        .send()
        .await?
        .error_for_status()?
        .json::<LoadCodeAssistResponse>()
        .await?;
    Ok(res)
}

pub async fn fetch_available_models(access_token: &str, project_id: Option<&str>) -> Result<FetchAvailableModelsResponse, reqwest::Error> {
    let client = reqwest::Client::new();
    let payload = if let Some(proj) = project_id {
        serde_json::json!({ "project": proj })
    } else {
        serde_json::json!({})
    };
    
    let res = client
        .post(format!("{}/v1internal:fetchAvailableModels", CLOUDCODE_BASE_URL))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .json(&payload)
        .send()
        .await?
        .error_for_status()?
        .json::<FetchAvailableModelsResponse>()
        .await?;
    Ok(res)
}

pub async fn try_onboard_user(access_token: &str, tier_id: &str) -> Result<Option<String>, reqwest::Error> {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "tierId": tier_id,
        "metadata": {
            "ideType": "ANTIGRAVITY",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI"
        }
    });
    
    let res = client
        .post(format!("{}/v1internal:onboardUser", CLOUDCODE_BASE_URL))
        .bearer_auth(access_token)
        .header("User-Agent", USER_AGENT)
        .json(&payload)
        .send()
        .await?;
        
    if !res.status().is_success() {
        return Ok(None);
    }
    
    #[derive(Deserialize)]
    struct OnboardResponse {
        done: Option<bool>,
        response: Option<serde_json::Value>,
    }
    
    if let Ok(onboard_res) = res.json::<OnboardResponse>().await {
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

pub fn pick_onboard_tier(response: &LoadCodeAssistResponse, default_tier: Option<&str>) -> Option<String> {
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

pub async fn resolve_project_id(access_token: &str, cached_project_id: Option<&str>) -> Option<String> {
    if let Some(p) = cached_project_id {
        if !p.is_empty() {
            return Some(p.to_string());
        }
    }
    
    let load_resp = match load_code_assist(access_token).await {
        Ok(resp) => resp,
        Err(_) => return None,
    };
    
    if let Some(ref proj_val) = load_resp.cloudaicompanion_project {
        if let Some(p) = extract_project_id(proj_val) {
            return Some(p);
        }
    }
    
    // Attempt onboarding
    let tier_id = load_resp.paid_tier.as_ref().and_then(|t| t.id.clone())
        .or_else(|| load_resp.current_tier.as_ref().and_then(|t| t.id.clone()));
        
    let onboard_tier = pick_onboard_tier(&load_resp, tier_id.as_deref())?;
    
    if let Ok(Some(proj_id)) = try_onboard_user(access_token, &onboard_tier).await {
        return Some(proj_id);
    }
    
    // Poll loadCodeAssist with retries
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(2000)).await;
        if let Ok(resp) = load_code_assist(access_token).await {
            if let Some(ref proj_val) = resp.cloudaicompanion_project {
                if let Some(p) = extract_project_id(proj_val) {
                    return Some(p);
                }
            }
        }
    }
    
    None
}

pub async fn get_valid_tokens(tokens: &mut StoredTokens) -> Result<String, Box<dyn std::error::Error>> {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    // Expiry buffer is 5 minutes (300,000 ms)
    let buffer = 5 * 60 * 1000;
    if now >= tokens.expires_at - buffer {
        // Refresh token
        let res = refresh_access_token(&tokens.refresh_token).await?;
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
