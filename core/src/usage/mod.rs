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
        let providers = self
            .collectors
            .iter()
            .filter(|collector| selection.includes(collector.id()))
            .map(|collector| collector.collect())
            .collect();

        UsageSnapshot {
            providers,
            updated_at: updated_label(),
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

fn updated_label() -> String {
    format!("UPDATED {}", unix_now())
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
}
