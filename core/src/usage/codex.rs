use std::path::PathBuf;

use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;

use super::{
    HTTP_TIMEOUT, UsageCollector, UsagePixelArt, UsageProvider, UsageRegistry, UsageTheme,
    clamp_percent_i64, codex_home, local, read_json, window,
};

pub const PROVIDER_ID: &str = "CODEX";
const THEME_COLOR: &str = "#3B82F6";
const PIXEL_ART_SIZE: usize = 32;

pub struct CodexUsageCollector;

impl UsageCollector for CodexUsageCollector {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn collect(&self) -> UsageProvider {
        match fetch_codex_oauth_provider() {
            Ok(provider) => provider,
            Err(err) => local::estimate_provider(
                PROVIDER_ID,
                "CODEX",
                codex_theme(),
                codex_pixel_art(),
                codex_log_roots(),
                err,
            ),
        }
    }
}

pub fn register(registry: &mut UsageRegistry) {
    registry.register(CodexUsageCollector);
}

fn fetch_codex_oauth_provider() -> Result<UsageProvider, String> {
    let auth_path = codex_home().join("auth.json");
    let auth: CodexAuthFile = read_json(&auth_path)?;
    let access_token = auth
        .openai_api_key
        .or_else(|| {
            auth.tokens
                .as_ref()
                .and_then(|tokens| tokens.access_token.clone())
        })
        .ok_or_else(|| format!("{} has no Codex access token", auth_path.display()))?;
    if access_token.trim().is_empty() {
        return Err(format!(
            "{} has an empty Codex access token",
            auth_path.display()
        ));
    }

    let client = Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|err| err.to_string())?;
    let mut request = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(access_token.trim())
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "monitor-cli");
    if let Some(account_id) = auth.tokens.and_then(|tokens| tokens.account_id)
        && !account_id.trim().is_empty()
    {
        request = request.header("ChatGPT-Account-Id", account_id);
    }

    let response = request.send().map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Codex usage API returned {}", response.status()));
    }

    let usage = response
        .json::<CodexUsageResponse>()
        .map_err(|err| format!("decode Codex usage response: {err}"))?;
    let rate_limit = usage
        .rate_limit
        .ok_or_else(|| "Codex usage response has no rate_limit".to_string())?;
    let mut windows = Vec::new();
    if let Some(primary) = rate_limit.primary_window {
        windows.push(window(
            "5h",
            "5h",
            clamp_percent_i64(primary.used_percent),
            Some(format!("unix:{}", primary.reset_at)),
            "live",
        ));
    }
    if let Some(secondary) = rate_limit.secondary_window {
        windows.push(window(
            "7d",
            "Week",
            clamp_percent_i64(secondary.used_percent),
            Some(format!("unix:{}", secondary.reset_at)),
            "live",
        ));
    }
    if windows.is_empty() {
        return Err("Codex usage response has no quota windows".to_string());
    }

    Ok(UsageProvider {
        id: PROVIDER_ID.to_string(),
        label: "CODEX".to_string(),
        theme_color: Some(THEME_COLOR.to_string()),
        theme: Some(codex_theme()),
        pixel_art: Some(codex_pixel_art()),
        source: "oauth".to_string(),
        account: None,
        plan: usage.plan_type.map(|plan| plan.to_ascii_uppercase()),
        windows,
    })
}

fn codex_log_roots() -> Vec<PathBuf> {
    let root = codex_home();
    vec![root.join("sessions"), root.join("archived_sessions")]
}

fn codex_theme() -> UsageTheme {
    UsageTheme {
        accent: THEME_COLOR.to_string(),
        panel: "#101823".to_string(),
        panel_soft: "#162338".to_string(),
        primary_panel: "#111C2D".to_string(),
        primary_panel_soft: "#1A3154".to_string(),
        track: "#263141".to_string(),
        pill: "#263246".to_string(),
    }
}

fn codex_pixel_art() -> UsagePixelArt {
    UsagePixelArt {
        color: THEME_COLOR.to_string(),
        rows: vec!["1".repeat(PIXEL_ART_SIZE); PIXEL_ART_SIZE],
    }
}

#[derive(Debug, Deserialize)]
struct CodexAuthFile {
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
    tokens: Option<CodexTokens>,
}

#[derive(Debug, Deserialize)]
struct CodexTokens {
    #[serde(alias = "accessToken")]
    access_token: Option<String>,
    #[serde(alias = "accountId")]
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexUsageResponse {
    #[serde(rename = "plan_type")]
    plan_type: Option<String>,
    #[serde(rename = "rate_limit")]
    rate_limit: Option<CodexRateLimit>,
}

#[derive(Debug, Deserialize)]
struct CodexRateLimit {
    #[serde(rename = "primary_window")]
    primary_window: Option<CodexWindow>,
    #[serde(rename = "secondary_window")]
    secondary_window: Option<CodexWindow>,
}

#[derive(Debug, Deserialize)]
struct CodexWindow {
    used_percent: i64,
    reset_at: i64,
}
