use std::path::PathBuf;

use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;

use super::{
    HTTP_TIMEOUT, UsageCollector, UsageProvider, UsageRegistry, UsageTheme, home_dir, local,
    percent_from_value, read_json, unix_now, window,
};

pub const PROVIDER_ID: &str = "CLAUDE";
const THEME_COLOR: &str = "#D97757";

pub struct ClaudeUsageCollector;

impl UsageCollector for ClaudeUsageCollector {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn collect(&self) -> UsageProvider {
        match fetch_claude_oauth_provider() {
            Ok(provider) => provider,
            Err(err) => local::estimate_provider(
                PROVIDER_ID,
                "CLAUDE",
                claude_theme(),
                None,
                claude_log_roots(),
                err,
            ),
        }
    }
}

pub fn register(registry: &mut UsageRegistry) {
    registry.register(ClaudeUsageCollector);
}

fn fetch_claude_oauth_provider() -> Result<UsageProvider, String> {
    let auth_path = home_dir().join(".claude").join(".credentials.json");
    let credentials: ClaudeCredentialsFile = read_json(&auth_path)?;
    let oauth = credentials
        .claude_ai_oauth
        .ok_or_else(|| format!("{} has no claudeAiOauth entry", auth_path.display()))?;
    let access_token = oauth
        .access_token
        .ok_or_else(|| format!("{} has no Claude access token", auth_path.display()))?;
    if access_token.trim().is_empty() {
        return Err(format!(
            "{} has an empty Claude access token",
            auth_path.display()
        ));
    }
    if let Some(expires_at) = oauth.expires_at {
        let now_ms = unix_now().saturating_mul(1000);
        if expires_at <= now_ms as f64 {
            return Err("Claude OAuth token is expired".to_string());
        }
    }

    let client = Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(access_token.trim())
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .header("anthropic-beta", "oauth-2025-04-20")
        .header(USER_AGENT, "claude-code/2.1.0 monitor-cli")
        .send()
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Claude usage API returned {}", response.status()));
    }

    let usage = response
        .json::<Value>()
        .map_err(|err| format!("decode Claude usage response: {err}"))?;
    let mut windows = Vec::new();
    push_claude_window(&mut windows, &usage, "five_hour", "5h", "5h");
    push_claude_window(&mut windows, &usage, "seven_day", "7d", "Week");
    push_claude_window(
        &mut windows,
        &usage,
        "seven_day_sonnet",
        "7d-sonnet",
        "Sonnet",
    );
    push_claude_window(&mut windows, &usage, "seven_day_opus", "7d-opus", "Opus");
    if windows.is_empty() {
        return Err("Claude usage response has no quota windows".to_string());
    }

    Ok(UsageProvider {
        id: PROVIDER_ID.to_string(),
        label: "CLAUDE".to_string(),
        theme_color: Some(THEME_COLOR.to_string()),
        theme: Some(claude_theme()),
        pixel_art: None,
        source: "oauth".to_string(),
        account: None,
        plan: oauth
            .subscription_type
            .or(oauth.rate_limit_tier)
            .map(|plan| plan.to_ascii_uppercase()),
        windows,
    })
}

fn push_claude_window(
    windows: &mut Vec<super::UsageWindow>,
    usage: &Value,
    api_key: &str,
    kind: &str,
    label: &str,
) {
    let Some(raw_window) = usage.get(api_key) else {
        return;
    };
    let Some(percent) = raw_window.get("utilization").and_then(percent_from_value) else {
        return;
    };
    let resets_at = raw_window
        .get("resets_at")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    windows.push(window(kind, label, percent, resets_at, "live"));
}

fn claude_log_roots() -> Vec<PathBuf> {
    if let Ok(value) = std::env::var("CLAUDE_CONFIG_DIR") {
        let roots = value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(|entry| PathBuf::from(entry).join("projects"))
            .collect::<Vec<_>>();
        if !roots.is_empty() {
            return roots;
        }
    }

    let home = home_dir();
    vec![
        home.join(".config").join("claude").join("projects"),
        home.join(".claude").join("projects"),
    ]
}

fn claude_theme() -> UsageTheme {
    UsageTheme {
        accent: THEME_COLOR.to_string(),
        panel: "#1D1714".to_string(),
        panel_soft: "#2A1E18".to_string(),
        primary_panel: "#231912".to_string(),
        primary_panel_soft: "#3A251A".to_string(),
        track: "#3A2B25".to_string(),
        pill: "#3B2B25".to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct ClaudeCredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOAuth>,
}

#[derive(Debug, Deserialize)]
struct ClaudeOAuth {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<f64>,
    #[serde(rename = "rateLimitTier")]
    rate_limit_tier: Option<String>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}
