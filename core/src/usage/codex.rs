use std::collections::BTreeSet;
use std::path::PathBuf;

use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::{
    HTTP_TIMEOUT, UsageCollector, UsageProvider, UsageRegistry, UsageTheme, clamp_percent_i64,
    codex_home, local, read_json, window,
};

pub const PROVIDER_ID: &str = "CODEX";
const THEME_COLOR: &str = "#3B82F6";

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
                None,
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
        .header(USER_AGENT, "CodexBar");
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
    let windows = codex_usage_windows(&rate_limit, &usage.additional_rate_limits);
    if windows.is_empty() {
        return Err("Codex usage response has no quota windows".to_string());
    }

    Ok(UsageProvider {
        id: PROVIDER_ID.to_string(),
        label: "CODEX".to_string(),
        theme_color: Some(THEME_COLOR.to_string()),
        theme: Some(codex_theme()),
        pixel_art: None,
        source: "oauth".to_string(),
        account: None,
        plan: usage.plan_type.map(|plan| plan.to_ascii_uppercase()),
        windows,
    })
}

fn codex_usage_windows(
    rate_limit: &CodexRateLimit,
    additional_rate_limits: &[CodexAdditionalRateLimit],
) -> Vec<super::UsageWindow> {
    let mut windows = Vec::new();
    push_codex_window(&mut windows, "5h", "5h", rate_limit.primary_window.as_ref());
    push_codex_window(
        &mut windows,
        "7d",
        "Week",
        rate_limit.secondary_window.as_ref(),
    );
    push_additional_codex_windows(&mut windows, additional_rate_limits);
    windows
}

fn push_codex_window(
    windows: &mut Vec<super::UsageWindow>,
    kind: &str,
    label: &str,
    raw_window: Option<&CodexWindow>,
) {
    let Some(raw_window) = raw_window else {
        return;
    };
    windows.push(window(
        kind,
        label,
        clamp_percent_i64(raw_window.used_percent),
        Some(format!("unix:{}", raw_window.reset_at)),
        "live",
    ));
}

fn push_additional_codex_windows(
    windows: &mut Vec<super::UsageWindow>,
    additional_rate_limits: &[CodexAdditionalRateLimit],
) {
    let mut used_kinds = BTreeSet::new();
    for entry in additional_rate_limits {
        if is_spark_limit(entry) {
            push_spark_codex_windows(windows, entry, &mut used_kinds);
            continue;
        }

        let Some(raw_window) = entry.rate_limit.as_ref().and_then(|rate_limit| {
            rate_limit
                .primary_window
                .as_ref()
                .or(rate_limit.secondary_window.as_ref())
        }) else {
            continue;
        };
        let Some(kind) = additional_codex_window_kind(entry) else {
            continue;
        };
        if !used_kinds.insert(kind.clone()) {
            continue;
        }
        let label = additional_codex_window_label(entry);
        windows.push(window(
            kind.as_str(),
            label.as_str(),
            clamp_percent_i64(raw_window.used_percent),
            Some(format!("unix:{}", raw_window.reset_at)),
            "live",
        ));
    }
}

fn push_spark_codex_windows(
    windows: &mut Vec<super::UsageWindow>,
    entry: &CodexAdditionalRateLimit,
    used_kinds: &mut BTreeSet<String>,
) {
    let Some(rate_limit) = &entry.rate_limit else {
        return;
    };
    let candidates = [
        (rate_limit.primary_window.as_ref(), ("codex-spark", "Spark")),
        (
            rate_limit.secondary_window.as_ref(),
            ("codex-spark-weekly", "Spark Wk"),
        ),
    ];
    for (raw_window, fallback) in candidates {
        let Some(raw_window) = raw_window else {
            continue;
        };
        let (kind, label) = spark_kind_and_label(raw_window, fallback);
        if !used_kinds.insert(kind.to_string()) {
            continue;
        }
        windows.push(window(
            kind,
            label,
            clamp_percent_i64(raw_window.used_percent),
            Some(format!("unix:{}", raw_window.reset_at)),
            "live",
        ));
    }
}

fn spark_kind_and_label(
    raw_window: &CodexWindow,
    fallback: (&'static str, &'static str),
) -> (&'static str, &'static str) {
    let minutes = raw_window.limit_window_seconds / 60;
    if minutes > 0 && minutes <= 6 * 60 {
        return ("codex-spark", "Spark");
    }
    if minutes >= 6 * 24 * 60 {
        return ("codex-spark-weekly", "Spark Wk");
    }
    fallback
}

fn additional_codex_window_kind(entry: &CodexAdditionalRateLimit) -> Option<String> {
    first_non_empty(&[entry.metered_feature.as_ref(), entry.limit_name.as_ref()])
        .map(|source| format!("codex-{}", slug(source)))
        .filter(|kind| kind != "codex-")
}

fn additional_codex_window_label(entry: &CodexAdditionalRateLimit) -> String {
    first_non_empty(&[entry.limit_name.as_ref(), entry.metered_feature.as_ref()])
        .map(compact_label)
        .unwrap_or_else(|| "Extra".to_string())
}

fn is_spark_limit(entry: &CodexAdditionalRateLimit) -> bool {
    [entry.limit_name.as_ref(), entry.metered_feature.as_ref()]
        .into_iter()
        .flatten()
        .any(|value| value.to_ascii_lowercase().contains("spark"))
}

fn first_non_empty<'a>(values: &[Option<&'a String>]) -> Option<&'a str> {
    values.iter().find_map(|value| {
        value
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    })
}

fn compact_label(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() <= 12 {
        return value.to_string();
    }
    value.chars().take(12).collect()
}

fn slug(value: &str) -> String {
    let mut result = String::new();
    let mut last_was_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            result.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            result.push('-');
            last_was_dash = true;
        }
    }
    result.trim_matches('-').to_string()
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
    #[serde(
        rename = "additional_rate_limits",
        default,
        deserialize_with = "deserialize_additional_rate_limits"
    )]
    additional_rate_limits: Vec<CodexAdditionalRateLimit>,
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
    #[serde(default)]
    limit_window_seconds: i64,
}

#[derive(Debug, Deserialize)]
struct CodexAdditionalRateLimit {
    #[serde(rename = "limit_name")]
    limit_name: Option<String>,
    #[serde(rename = "metered_feature")]
    metered_feature: Option<String>,
    #[serde(rename = "rate_limit")]
    rate_limit: Option<CodexRateLimit>,
}

fn deserialize_additional_rate_limits<'de, D>(
    deserializer: D,
) -> Result<Vec<CodexAdditionalRateLimit>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = value else {
        return Ok(Vec::new());
    };
    Ok(items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_codex_additional_rate_limits() {
        let usage = serde_json::from_value::<CodexUsageResponse>(serde_json::json!({
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 10,
                    "reset_at": 1_781_098_000,
                    "limit_window_seconds": 18_000
                },
                "secondary_window": {
                    "used_percent": 20,
                    "reset_at": 1_781_702_800,
                    "limit_window_seconds": 604_800
                }
            },
            "additional_rate_limits": [
                {
                    "limit_name": "GPT-5.3-Codex-Spark",
                    "metered_feature": "codex_spark",
                    "rate_limit": {
                        "primary_window": {
                            "used_percent": 30,
                            "reset_at": 1_781_098_000,
                            "limit_window_seconds": 18_000
                        },
                        "secondary_window": {
                            "used_percent": 40,
                            "reset_at": 1_781_702_800,
                            "limit_window_seconds": 604_800
                        }
                    }
                },
                {
                    "limit_name": "Model Pool",
                    "metered_feature": "model_pool",
                    "rate_limit": {
                        "primary_window": {
                            "used_percent": 50,
                            "reset_at": 1_781_098_000,
                            "limit_window_seconds": 18_000
                        }
                    }
                }
            ]
        }))
        .expect("decode usage");

        let rate_limit = usage.rate_limit.as_ref().expect("rate limit");
        let windows = codex_usage_windows(rate_limit, &usage.additional_rate_limits);
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
                ("5h", "5h", 10),
                ("7d", "Week", 20),
                ("codex-spark", "Spark", 30),
                ("codex-spark-weekly", "Spark Wk", 40),
                ("codex-model-pool", "Model Pool", 50),
            ]
        );
    }

    #[test]
    fn ignores_malformed_codex_additional_rate_limits() {
        let usage = serde_json::from_value::<CodexUsageResponse>(serde_json::json!({
            "rate_limit": {
                "primary_window": {
                    "used_percent": 10,
                    "reset_at": 1_781_098_000
                }
            },
            "additional_rate_limits": ["garbage", 1, true]
        }))
        .expect("decode usage");

        assert!(usage.additional_rate_limits.is_empty());
    }
}
