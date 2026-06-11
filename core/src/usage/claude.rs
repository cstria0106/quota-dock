use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::StatusCode;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, RETRY_AFTER, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;

use super::{
    HTTP_TIMEOUT, UsageCollector, UsageProvider, UsageRegistry, UsageTheme, UsageWindow, home_dir,
    local, percent_from_value, read_json, unix_now, window,
};

pub const PROVIDER_ID: &str = "CLAUDE";
const THEME_COLOR: &str = "#D97757";
const FALLBACK_CLAUDE_CODE_VERSION: &str = "2.1.0";
const CLAUDE_RATE_LIMIT_BACKOFF_SECS: u64 = 5 * 60;
static CLAUDE_OAUTH_RATE_LIMITED_UNTIL: AtomicU64 = AtomicU64::new(0);

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
    if let Some(blocked_until) = claude_oauth_rate_limited_until(unix_now()) {
        return Err(rate_limit_error(blocked_until));
    }

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
        .header(USER_AGENT, claude_code_user_agent())
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        let now = unix_now();
        let blocked_until = retry_after_unix(response.headers(), now)
            .unwrap_or_else(|| now.saturating_add(CLAUDE_RATE_LIMIT_BACKOFF_SECS));
        CLAUDE_OAUTH_RATE_LIMITED_UNTIL.store(blocked_until, Ordering::Relaxed);
        return Err(rate_limit_error(blocked_until));
    }
    if !status.is_success() {
        return Err(format!("Claude usage API returned {status}"));
    }
    CLAUDE_OAUTH_RATE_LIMITED_UNTIL.store(0, Ordering::Relaxed);

    let usage = response
        .json::<Value>()
        .map_err(|err| format!("decode Claude usage response: {err}"))?;
    let windows = claude_usage_windows(&usage);
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

fn claude_usage_windows(usage: &Value) -> Vec<UsageWindow> {
    let mut windows = Vec::new();
    push_claude_window(&mut windows, usage, "five_hour", "5h", "5h");
    push_claude_window(&mut windows, usage, "seven_day", "7d", "Week");
    push_claude_window(
        &mut windows,
        usage,
        "seven_day_sonnet",
        "7d-sonnet",
        "Sonnet",
    );
    push_claude_window(&mut windows, usage, "seven_day_opus", "7d-opus", "Opus");
    push_claude_window(
        &mut windows,
        usage,
        "seven_day_oauth_apps",
        "7d-oauth",
        "OAuth",
    );
    push_first_claude_window(
        &mut windows,
        usage,
        &[
            "seven_day_routines",
            "seven_day_claude_routines",
            "claude_routines",
            "routines",
            "routine",
            "seven_day_cowork",
            "cowork",
        ],
        "7d-routines",
        "Routines",
    );
    push_claude_extra_usage_window(&mut windows, usage);
    windows
}

fn push_claude_window(
    windows: &mut Vec<UsageWindow>,
    usage: &Value,
    api_key: &str,
    kind: &str,
    label: &str,
) -> bool {
    let Some(raw_window) = usage.get(api_key).and_then(Value::as_object) else {
        return false;
    };
    let Some(percent) = raw_window.get("utilization").and_then(percent_from_value) else {
        return false;
    };
    let resets_at = raw_window
        .get("resets_at")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    windows.push(window(kind, label, percent, resets_at, "live"));
    true
}

fn push_first_claude_window(
    windows: &mut Vec<UsageWindow>,
    usage: &Value,
    api_keys: &[&str],
    kind: &str,
    label: &str,
) {
    for api_key in api_keys {
        if !usage
            .as_object()
            .is_some_and(|object| object.contains_key(*api_key))
        {
            continue;
        }
        if push_claude_window(windows, usage, api_key, kind, label) {
            return;
        }
        if usage.get(*api_key).is_some_and(Value::is_null) {
            windows.push(window(kind, label, 0, None, "live"));
            return;
        }
    }
}

fn push_claude_extra_usage_window(windows: &mut Vec<UsageWindow>, usage: &Value) {
    let Some(extra_usage) = usage.get("extra_usage").and_then(Value::as_object) else {
        return;
    };
    if extra_usage.get("is_enabled").and_then(Value::as_bool) == Some(false) {
        return;
    }
    let percent = extra_usage
        .get("utilization")
        .and_then(percent_from_value)
        .or_else(|| {
            let used = extra_usage.get("used_credits").and_then(Value::as_f64)?;
            let limit = extra_usage.get("monthly_limit").and_then(Value::as_f64)?;
            (limit > 0.0).then(|| ((used / limit) * 100.0).round().clamp(0.0, 100.0) as u8)
        });
    let Some(percent) = percent else {
        return;
    };
    windows.push(window("extra-usage", "Spend", percent, None, "live"));
}

fn claude_oauth_rate_limited_until(now: u64) -> Option<u64> {
    let blocked_until = CLAUDE_OAUTH_RATE_LIMITED_UNTIL.load(Ordering::Relaxed);
    (blocked_until > now).then_some(blocked_until)
}

fn retry_after_unix(headers: &HeaderMap, now: u64) -> Option<u64> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?;
    retry_after_value_unix(raw, now)
}

fn retry_after_value_unix(raw: &str, now: u64) -> Option<u64> {
    let seconds = raw.trim().parse::<u64>().ok()?;
    Some(now.saturating_add(seconds))
}

fn rate_limit_error(blocked_until: u64) -> String {
    format!("rate limited until unix:{blocked_until}")
}

fn claude_code_user_agent() -> String {
    claude_code_user_agent_with_version(claude_code_version_output().as_deref())
}

fn claude_code_user_agent_with_version(version_output: Option<&str>) -> String {
    let version = version_output
        .and_then(normalized_claude_code_version)
        .unwrap_or_else(|| FALLBACK_CLAUDE_CODE_VERSION.to_string());
    format!("claude-code/{version}")
}

fn claude_code_version_output() -> Option<String> {
    if let Ok(version) = std::env::var("CLAUDE_CODE_VERSION")
        && !version.trim().is_empty()
    {
        return Some(version);
    }

    let output = Command::new("claude").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return Some(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    (!stderr.is_empty()).then_some(stderr)
}

fn normalized_claude_code_version(raw: &str) -> Option<String> {
    raw.split_whitespace()
        .find_map(|token| {
            let token = token
                .trim_start_matches('v')
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.' && ch != '-');
            token
                .chars()
                .any(|ch| ch.is_ascii_digit())
                .then(|| token.to_string())
        })
        .filter(|version| !version.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_extended_oauth_usage_windows() {
        let usage = json!({
            "five_hour": { "utilization": 1, "resets_at": "2025-12-25T12:00:00.000Z" },
            "seven_day": { "utilization": 4, "resets_at": "2025-12-31T00:00:00.000Z" },
            "seven_day_sonnet": { "utilization": 1 },
            "seven_day_opus": { "utilization": 2 },
            "seven_day_oauth_apps": { "utilization": 3 },
            "seven_day_cowork": null,
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 2050,
                "used_credits": 325
            }
        });

        let windows = claude_usage_windows(&usage);
        let summaries = windows
            .iter()
            .map(|window| {
                (
                    window.kind.as_str(),
                    window.label.as_str(),
                    window.used_percent,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            summaries,
            vec![
                ("5h", "5h", 1),
                ("7d", "Week", 4),
                ("7d-sonnet", "Sonnet", 1),
                ("7d-opus", "Opus", 2),
                ("7d-oauth", "OAuth", 3),
                ("7d-routines", "Routines", 0),
                ("extra-usage", "Spend", 16),
            ]
        );
    }

    #[test]
    fn parses_retry_after_seconds() {
        assert_eq!(retry_after_value_unix("120", 1_000), Some(1_120));
        assert_eq!(retry_after_value_unix("soon", 1_000), None);
    }

    #[test]
    fn builds_codexbar_style_claude_user_agent() {
        assert_eq!(
            claude_code_user_agent_with_version(Some("Claude Code v2.3.4")),
            "claude-code/2.3.4"
        );
        assert_eq!(
            claude_code_user_agent_with_version(Some("claude-code 1.2.3")),
            "claude-code/1.2.3"
        );
        assert_eq!(
            claude_code_user_agent_with_version(None),
            "claude-code/2.1.0"
        );
    }
}
