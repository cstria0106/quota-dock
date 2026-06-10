use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use super::{UsageProvider, window};

const FIVE_HOURS: Duration = Duration::from_secs(5 * 60 * 60);
const SEVEN_DAYS: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_JSONL_FILES: usize = 4_000;

pub(super) fn estimate_provider(
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

fn estimate_percent(tokens: u64, denominator: u64) -> u8 {
    if tokens == 0 {
        return 0;
    }
    (tokens.saturating_mul(100) / denominator).clamp(1, 100) as u8
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

#[derive(Default)]
struct LocalUsageTotals {
    files: usize,
    five_hour_tokens: u64,
    seven_day_tokens: u64,
}
