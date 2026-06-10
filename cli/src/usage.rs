use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const FIVE_HOURS: Duration = Duration::from_secs(5 * 60 * 60);
const SEVEN_DAYS: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_JSONL_FILES: usize = 4_000;

#[derive(Clone, Copy, Debug)]
pub enum ProviderSelection {
    All,
    Codex,
    Claude,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageSnapshot {
    pub providers: Vec<UsageProvider>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageProvider {
    pub id: String,
    pub label: String,
    pub source: String,
    pub account: Option<String>,
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageWindow {
    pub kind: String,
    pub label: String,
    pub used_percent: u8,
    pub resets_at: Option<String>,
    pub status: String,
}

pub fn collect_snapshot(selection: ProviderSelection) -> UsageSnapshot {
    let mut providers = Vec::new();
    if matches!(selection, ProviderSelection::All | ProviderSelection::Codex) {
        providers.push(collect_codex_provider());
    }
    if matches!(
        selection,
        ProviderSelection::All | ProviderSelection::Claude
    ) {
        providers.push(collect_claude_provider());
    }

    UsageSnapshot {
        providers,
        updated_at: updated_label(),
    }
}

fn collect_codex_provider() -> UsageProvider {
    match fetch_codex_oauth_provider() {
        Ok(provider) => provider,
        Err(err) => local_estimate_provider("CODEX", "CODEX", codex_log_roots(), err),
    }
}

fn collect_claude_provider() -> UsageProvider {
    match fetch_claude_oauth_provider() {
        Ok(provider) => provider,
        Err(err) => local_estimate_provider("CLAUDE", "CLAUDE", claude_log_roots(), err),
    }
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
    if let Some(account_id) = auth.tokens.and_then(|tokens| tokens.account_id) {
        if !account_id.trim().is_empty() {
            request = request.header("ChatGPT-Account-Id", account_id);
        }
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
            "5H",
            clamp_percent_i64(primary.used_percent),
            Some(format!("unix:{}", primary.reset_at)),
            "live",
        ));
    }
    if let Some(secondary) = rate_limit.secondary_window {
        windows.push(window(
            "7d",
            "7D",
            clamp_percent_i64(secondary.used_percent),
            Some(format!("unix:{}", secondary.reset_at)),
            "live",
        ));
    }
    if windows.is_empty() {
        return Err("Codex usage response has no quota windows".to_string());
    }

    Ok(UsageProvider {
        id: "CODEX".to_string(),
        label: "CODEX".to_string(),
        source: "oauth".to_string(),
        account: None,
        plan: usage.plan_type.map(|plan| plan.to_ascii_uppercase()),
        windows,
    })
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
    push_claude_window(&mut windows, &usage, "five_hour", "5h", "5H");
    push_claude_window(&mut windows, &usage, "seven_day", "7d", "7D");
    push_claude_window(
        &mut windows,
        &usage,
        "seven_day_sonnet",
        "7d-sonnet",
        "SONNET",
    );
    push_claude_window(&mut windows, &usage, "seven_day_opus", "7d-opus", "OPUS");
    if windows.is_empty() {
        return Err("Claude usage response has no quota windows".to_string());
    }

    Ok(UsageProvider {
        id: "CLAUDE".to_string(),
        label: "CLAUDE".to_string(),
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
    windows: &mut Vec<UsageWindow>,
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

fn local_estimate_provider(
    id: &str,
    label: &str,
    roots: Vec<PathBuf>,
    live_error: String,
) -> UsageProvider {
    let totals = scan_local_usage(&roots);
    if totals.files == 0 {
        return UsageProvider {
            id: id.to_string(),
            label: label.to_string(),
            source: "unavailable".to_string(),
            account: None,
            plan: Some(short_error(&live_error)),
            windows: vec![
                window("5h", "5H", 0, None, "error"),
                window("7d", "7D", 0, None, "error"),
            ],
        };
    }

    UsageProvider {
        id: id.to_string(),
        label: label.to_string(),
        source: "local-estimate".to_string(),
        account: None,
        plan: Some(format!("{} files", totals.files)),
        windows: vec![
            window(
                "5h",
                "5H",
                estimate_percent(totals.five_hour_tokens, 1_000_000),
                Some("rolling".to_string()),
                "estimated",
            ),
            window(
                "7d",
                "7D",
                estimate_percent(totals.seven_day_tokens, 6_000_000),
                Some("rolling".to_string()),
                "estimated",
            ),
        ],
    }
}

fn scan_local_usage(roots: &[PathBuf]) -> LocalUsageTotals {
    let mut files = Vec::new();
    for root in roots {
        collect_jsonl_files(root, &mut files);
        if files.len() >= MAX_JSONL_FILES {
            break;
        }
    }

    let mut totals = LocalUsageTotals::default();
    for path in files {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let Ok(age) = modified.elapsed() else {
            continue;
        };
        if age > SEVEN_DAYS {
            continue;
        }

        let tokens = scan_jsonl_tokens(&path);
        totals.files += 1;
        totals.seven_day_tokens = totals.seven_day_tokens.saturating_add(tokens);
        if age <= FIVE_HOURS {
            totals.five_hour_tokens = totals.five_hour_tokens.saturating_add(tokens);
        }
    }
    totals
}

fn collect_jsonl_files(root: &Path, files: &mut Vec<PathBuf>) {
    if files.len() >= MAX_JSONL_FILES || !root.exists() {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_JSONL_FILES {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, files);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("jsonl"))
            .unwrap_or(false)
        {
            files.push(path);
        }
    }
}

fn scan_jsonl_tokens(path: &Path) -> u64 {
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let reader = BufReader::new(file);
    let mut total = 0_u64;
    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            total = total.saturating_add(token_sum(&value));
        }
    }
    total
}

fn token_sum(value: &Value) -> u64 {
    match value {
        Value::Object(object) => {
            let component_sum = [
                "input_tokens",
                "output_tokens",
                "cache_creation_input_tokens",
                "cache_read_input_tokens",
                "cached_input_tokens",
                "reasoning_tokens",
            ]
            .iter()
            .filter_map(|key| object.get(*key).and_then(Value::as_u64))
            .sum::<u64>();
            if component_sum > 0 {
                return component_sum;
            }

            if let Some(total) = object
                .get("total_tokens")
                .or_else(|| object.get("total_token_count"))
                .and_then(Value::as_u64)
            {
                return total;
            }

            object.values().map(token_sum).sum()
        }
        Value::Array(values) => values.iter().map(token_sum).sum(),
        _ => 0,
    }
}

fn codex_log_roots() -> Vec<PathBuf> {
    let root = codex_home();
    vec![root.join("sessions"), root.join("archived_sessions")]
}

fn claude_log_roots() -> Vec<PathBuf> {
    if let Ok(value) = env::var("CLAUDE_CONFIG_DIR") {
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

fn codex_home() -> PathBuf {
    env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| home_dir().join(".codex"))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn read_json<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let contents =
        fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    serde_json::from_str(&contents).map_err(|err| format!("parse {}: {err}", path.display()))
}

fn window(
    kind: &str,
    label: &str,
    used_percent: u8,
    resets_at: Option<String>,
    status: &str,
) -> UsageWindow {
    UsageWindow {
        kind: kind.to_string(),
        label: label.to_string(),
        used_percent,
        resets_at,
        status: status.to_string(),
    }
}

fn estimate_percent(tokens: u64, denominator: u64) -> u8 {
    if tokens == 0 {
        return 0;
    }
    ((tokens.saturating_mul(100) / denominator).max(1).min(100)) as u8
}

fn percent_from_value(value: &Value) -> Option<u8> {
    let raw = value.as_f64()?;
    let percent = if raw <= 1.0 { raw * 100.0 } else { raw };
    Some(percent.round().clamp(0.0, 100.0) as u8)
}

fn clamp_percent_i64(value: i64) -> u8 {
    value.clamp(0, 100) as u8
}

fn short_error(error: &str) -> String {
    error
        .split(':')
        .next()
        .unwrap_or(error)
        .chars()
        .take(18)
        .collect()
}

fn updated_label() -> String {
    format!("UPDATED {}", unix_now())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Default)]
struct LocalUsageTotals {
    files: usize,
    five_hour_tokens: u64,
    seven_day_tokens: u64,
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
