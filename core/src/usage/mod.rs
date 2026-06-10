pub mod claude;
pub mod codex;

mod local;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub(crate) const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
pub enum ProviderSelection {
    All,
    Codex,
    Claude,
}

impl ProviderSelection {
    fn includes(self, provider_id: &str) -> bool {
        match self {
            ProviderSelection::All => true,
            ProviderSelection::Codex => provider_id.eq_ignore_ascii_case(codex::PROVIDER_ID),
            ProviderSelection::Claude => provider_id.eq_ignore_ascii_case(claude::PROVIDER_ID),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageSnapshot {
    pub providers: Vec<UsageProvider>,
    pub updated_at: String,
    pub updated_at_unix: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageProvider {
    pub id: String,
    pub label: String,
    pub theme_color: Option<String>,
    pub theme: Option<UsageTheme>,
    pub pixel_art: Option<UsagePixelArt>,
    pub source: String,
    pub account: Option<String>,
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageTheme {
    pub accent: String,
    pub panel: String,
    pub panel_soft: String,
    pub primary_panel: String,
    pub primary_panel_soft: String,
    pub track: String,
    pub pill: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsagePixelArt {
    pub color: String,
    pub rows: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageWindow {
    pub kind: String,
    pub label: String,
    pub used_percent: u8,
    pub resets_at: Option<String>,
    pub resets_at_unix: Option<u64>,
    pub status: String,
}

pub trait UsageCollector: Send + Sync {
    fn id(&self) -> &'static str;
    fn collect(&self) -> UsageProvider;
}

#[derive(Default)]
pub struct UsageRegistry {
    collectors: Vec<Box<dyn UsageCollector>>,
}

impl UsageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default_providers() -> Self {
        let mut registry = Self::new();
        codex::register(&mut registry);
        claude::register(&mut registry);
        registry
    }

    pub fn register<C>(&mut self, collector: C)
    where
        C: UsageCollector + 'static,
    {
        self.collectors.push(Box::new(collector));
    }

    pub fn collect_snapshot(&self, selection: ProviderSelection) -> UsageSnapshot {
        let updated_at_unix = unix_now();
        let providers = self
            .collectors
            .iter()
            .filter(|collector| selection.includes(collector.id()))
            .map(|collector| collector.collect())
            .collect();

        UsageSnapshot {
            providers,
            updated_at: updated_label(updated_at_unix),
            updated_at_unix,
        }
    }
}

pub fn collect_snapshot(selection: ProviderSelection) -> UsageSnapshot {
    UsageRegistry::with_default_providers().collect_snapshot(selection)
}

pub(crate) fn read_json<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let contents =
        fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    serde_json::from_str(&contents).map_err(|err| format!("parse {}: {err}", path.display()))
}

pub(crate) fn window(
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
        resets_at_unix: resets_at.as_deref().and_then(reset_unix),
        resets_at,
        status: status.to_string(),
    }
}

pub(crate) fn percent_from_value(value: &serde_json::Value) -> Option<u8> {
    let raw = value.as_f64()?;
    let percent = if raw <= 1.0 { raw * 100.0 } else { raw };
    Some(percent.round().clamp(0.0, 100.0) as u8)
}

pub(crate) fn clamp_percent_i64(value: i64) -> u8 {
    value.clamp(0, 100) as u8
}

pub(crate) fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| home_dir().join(".codex"))
}

pub(crate) fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn updated_label(updated_at_unix: u64) -> String {
    format!("UPDATED {updated_at_unix}")
}

fn reset_unix(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Some(timestamp) = value.strip_prefix("unix:") {
        return timestamp.parse::<u64>().ok();
    }
    parse_rfc3339_unix(value)
}

fn parse_rfc3339_unix(value: &str) -> Option<u64> {
    if value.len() < 20 {
        return None;
    }
    let year = value.get(0..4)?.parse::<i32>().ok()?;
    let month = value.get(5..7)?.parse::<u32>().ok()?;
    let day = value.get(8..10)?.parse::<u32>().ok()?;
    let hour = value.get(11..13)?.parse::<u32>().ok()?;
    let minute = value.get(14..16)?.parse::<u32>().ok()?;
    let second = value.get(17..19)?.parse::<u32>().ok()?;
    if value.get(4..5)? != "-"
        || value.get(7..8)? != "-"
        || !matches!(value.get(10..11)?, "T" | "t" | " ")
        || value.get(13..14)? != ":"
        || value.get(16..17)? != ":"
    {
        return None;
    }
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let offset_start = value[19..]
        .find(|character| matches!(character, 'Z' | 'z' | '+' | '-'))
        .map(|index| 19 + index)?;
    let offset = value.get(offset_start..)?;
    let offset_seconds = if offset.starts_with(['Z', 'z']) {
        0
    } else {
        let sign = if offset.starts_with('+') { 1 } else { -1 };
        let hours = offset.get(1..3)?.parse::<i64>().ok()?;
        let minutes = offset.get(4..6)?.parse::<i64>().ok()?;
        if offset.get(3..4)? != ":" || hours > 23 || minutes > 59 {
            return None;
        }
        sign * (hours * 3_600 + minutes * 60)
    };

    let days = days_from_civil(year, month, day)?;
    let unix = days
        .checked_mul(86_400)?
        .checked_add(hour as i64 * 3_600 + minute as i64 * 60 + second.min(59) as i64)?
        .checked_sub(offset_seconds)?;
    u64::try_from(unix).ok()
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = year as i64 - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i64;
    let day = day as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticCollector {
        id: &'static str,
    }

    impl UsageCollector for StaticCollector {
        fn id(&self) -> &'static str {
            self.id
        }

        fn collect(&self) -> UsageProvider {
            UsageProvider {
                id: self.id.to_string(),
                label: self.id.to_string(),
                theme_color: None,
                theme: None,
                pixel_art: None,
                source: "test".to_string(),
                account: None,
                plan: None,
                windows: Vec::new(),
            }
        }
    }

    #[test]
    fn registry_filters_registered_collectors() {
        let mut registry = UsageRegistry::new();
        registry.register(StaticCollector {
            id: codex::PROVIDER_ID,
        });
        registry.register(StaticCollector {
            id: claude::PROVIDER_ID,
        });

        let snapshot = registry.collect_snapshot(ProviderSelection::Codex);

        assert_eq!(snapshot.providers.len(), 1);
        assert_eq!(snapshot.providers[0].id, codex::PROVIDER_ID);
    }

    #[test]
    fn parses_unix_reset_labels() {
        assert_eq!(reset_unix("unix:1781098000"), Some(1_781_098_000));
    }

    #[test]
    fn parses_rfc3339_reset_labels() {
        assert_eq!(reset_unix("2026-06-10T12:30:45Z"), Some(1_781_094_645));
        assert_eq!(reset_unix("2026-06-10T21:30:45+09:00"), Some(1_781_094_645));
    }
}
