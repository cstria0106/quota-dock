pub mod claude;
pub mod codex;

mod local;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use image::{RgbaImage, imageops};
use serde::{Deserialize, Serialize};

pub(crate) const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const PIXEL_ART_SIZE: u32 = 96;
const MAX_PIXEL_ART_COLORS: usize = 61;
const PIXEL_ART_ALPHA_THRESHOLD: u8 = 96;
const PIXEL_ART_PALETTE_CHARS: &[u8] =
    b"123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

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
pub struct UsageProviderUpdate {
    pub provider: UsageProvider,
    pub updated_at: String,
    pub updated_at_unix: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncPayload {
    pub visible_provider_ids: Vec<String>,
    pub providers: Vec<ProviderSync>,
    pub updated_at: String,
    pub updated_at_unix: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderSync {
    pub id: String,
    pub usage: Option<UsageProvider>,
    pub image_id: Option<u32>,
    pub pixel_art: Option<UsagePixelArt>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncResponse {
    pub ok: bool,
    pub missing_images: Vec<String>,
    pub message: String,
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
    pub palette: Vec<String>,
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

pub fn collect_configured_snapshot(
    config_path: &Path,
    selection: ProviderSelection,
    include_images: bool,
) -> Result<UsageSnapshot, String> {
    let mut snapshot = collect_snapshot(selection);
    apply_configured_images(&mut snapshot, config_path, include_images)?;
    Ok(snapshot)
}

pub fn apply_configured_images(
    snapshot: &mut UsageSnapshot,
    config_path: &Path,
    include_images: bool,
) -> Result<(), String> {
    if !include_images {
        strip_provider_images(snapshot);
        return Ok(());
    }

    if !config_path.is_file() {
        return Ok(());
    }
    let Some(usage_config) = crate::config::read_config_file(config_path)?.usage else {
        return Ok(());
    };
    if usage_config.images.is_empty() {
        return Ok(());
    }
    attach_provider_images(snapshot, &usage_config.images, config_path)
}

pub fn validate_provider_image(path: &Path) -> Result<UsagePixelArt, String> {
    pixel_art_from_image(path)
}

pub fn provider_image_id(pixel_art: &UsagePixelArt) -> Result<u32, String> {
    let body = postcard::to_allocvec(pixel_art).map_err(|err| err.to_string())?;
    Ok(fnv1a32(&body))
}

pub fn attach_provider_images(
    snapshot: &mut UsageSnapshot,
    image_paths: &BTreeMap<String, PathBuf>,
    config_path: &Path,
) -> Result<(), String> {
    for provider in &mut snapshot.providers {
        let Some(path) = provider_image_path(provider, image_paths) else {
            continue;
        };
        let path = resolve_config_path(config_path, path);
        provider.pixel_art = Some(pixel_art_from_image(&path)?);
    }
    Ok(())
}

pub fn strip_provider_images(snapshot: &mut UsageSnapshot) {
    for provider in &mut snapshot.providers {
        provider.pixel_art = None;
    }
}

fn provider_image_path<'a>(
    provider: &UsageProvider,
    image_paths: &'a BTreeMap<String, PathBuf>,
) -> Option<&'a PathBuf> {
    image_paths
        .iter()
        .find(|(key, _)| {
            key.eq_ignore_ascii_case(provider.id.as_str())
                || key.eq_ignore_ascii_case(provider.label.as_str())
        })
        .map(|(_, path)| path)
}

fn resolve_config_path(config_path: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    if let Ok(suffix) = path.strip_prefix("~") {
        return home_dir().join(suffix);
    }
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(path)
}

fn pixel_art_from_image(path: &Path) -> Result<UsagePixelArt, String> {
    let mut image = image::ImageReader::open(path)
        .map_err(|err| format!("open provider image {}: {err}", path.display()))?
        .decode()
        .map_err(|err| format!("decode provider image {}: {err}", path.display()))?
        .to_rgba8();
    clear_transparent_rgb(&mut image);
    let image = crop_visible(image)
        .ok_or_else(|| format!("provider image {} has no visible pixels", path.display()))?;
    let mut image = fit_image(image);
    clear_transparent_rgb(&mut image);
    let pixels = visible_pixels(&image);
    if pixels.is_empty() {
        return Err(format!(
            "provider image {} has no visible pixels after resize",
            path.display()
        ));
    }

    let palette = build_palette(&pixels);
    let mut rows = Vec::with_capacity(PIXEL_ART_SIZE as usize);
    for y in 0..PIXEL_ART_SIZE {
        let mut row = String::with_capacity(PIXEL_ART_SIZE as usize);
        for x in 0..PIXEL_ART_SIZE {
            let pixel = image.get_pixel(x, y);
            if pixel[3] < PIXEL_ART_ALPHA_THRESHOLD {
                row.push('0');
            } else {
                let index = nearest_palette_color(&palette, [pixel[0], pixel[1], pixel[2]]);
                row.push(PIXEL_ART_PALETTE_CHARS[index] as char);
            }
        }
        rows.push(row);
    }

    Ok(UsagePixelArt {
        palette: palette
            .iter()
            .map(|[red, green, blue]| format!("#{red:02X}{green:02X}{blue:02X}"))
            .collect(),
        rows,
    })
}

fn fnv1a32(bytes: &[u8]) -> u32 {
    const FNV_OFFSET_BASIS: u32 = 0x811c9dc5;
    const FNV_PRIME: u32 = 0x01000193;

    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

fn clear_transparent_rgb(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        if pixel[3] < PIXEL_ART_ALPHA_THRESHOLD {
            *pixel = image::Rgba([0, 0, 0, 0]);
        }
    }
}

fn crop_visible(image: RgbaImage) -> Option<RgbaImage> {
    let (width, height) = image.dimensions();
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..height {
        for x in 0..width {
            if image.get_pixel(x, y)[3] >= PIXEL_ART_ALPHA_THRESHOLD {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    (min_x <= max_x && min_y <= max_y).then(|| {
        imageops::crop_imm(&image, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1).to_image()
    })
}

fn fit_image(image: RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let scale = (PIXEL_ART_SIZE as f32 / width as f32).min(PIXEL_ART_SIZE as f32 / height as f32);
    let resized_w = ((width as f32 * scale).round() as u32).clamp(1, PIXEL_ART_SIZE);
    let resized_h = ((height as f32 * scale).round() as u32).clamp(1, PIXEL_ART_SIZE);
    let resized = imageops::resize(&image, resized_w, resized_h, imageops::FilterType::Lanczos3);
    let mut output = RgbaImage::new(PIXEL_ART_SIZE, PIXEL_ART_SIZE);
    imageops::overlay(
        &mut output,
        &resized,
        i64::from((PIXEL_ART_SIZE - resized_w) / 2),
        i64::from((PIXEL_ART_SIZE - resized_h) / 2),
    );
    output
}

fn visible_pixels(image: &RgbaImage) -> Vec<[u8; 3]> {
    image
        .pixels()
        .filter(|pixel| pixel[3] >= PIXEL_ART_ALPHA_THRESHOLD)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}

fn build_palette(pixels: &[[u8; 3]]) -> Vec<[u8; 3]> {
    let mut buckets = BTreeMap::<u16, Bucket>::new();
    for [red, green, blue] in pixels {
        let key = (u16::from(red >> 4) << 8) | (u16::from(green >> 4) << 4) | u16::from(blue >> 4);
        let bucket = buckets.entry(key).or_default();
        bucket.count += 1;
        bucket.red += u32::from(*red);
        bucket.green += u32::from(*green);
        bucket.blue += u32::from(*blue);
    }

    let mut buckets = buckets.into_values().collect::<Vec<_>>();
    buckets.sort_by_key(|bucket| std::cmp::Reverse(bucket.count));
    buckets
        .into_iter()
        .take(MAX_PIXEL_ART_COLORS)
        .map(|bucket| {
            [
                (bucket.red / bucket.count) as u8,
                (bucket.green / bucket.count) as u8,
                (bucket.blue / bucket.count) as u8,
            ]
        })
        .collect()
}

fn nearest_palette_color(palette: &[[u8; 3]], color: [u8; 3]) -> usize {
    palette
        .iter()
        .enumerate()
        .min_by_key(|(_, candidate)| color_distance(**candidate, color))
        .map(|(index, _)| index)
        .unwrap_or_default()
}

fn color_distance(left: [u8; 3], right: [u8; 3]) -> u32 {
    let red = i32::from(left[0]) - i32::from(right[0]);
    let green = i32::from(left[1]) - i32::from(right[1]);
    let blue = i32::from(left[2]) - i32::from(right[2]);
    (red * red + green * green + blue * blue) as u32
}

#[derive(Default)]
struct Bucket {
    count: u32,
    red: u32,
    green: u32,
    blue: u32,
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
        .find(['Z', 'z', '+', '-'])
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
    fn converts_provider_image_to_palette_pixel_art() {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("create temp dir");
        let image_path = dir.join("provider.png");
        let mut image = RgbaImage::new(4, 4);
        for x in 1..3 {
            image.put_pixel(x, 1, image::Rgba([0x12, 0x34, 0x56, 0xff]));
        }
        image.save(&image_path).expect("save image");

        let art = pixel_art_from_image(&image_path).expect("convert image");

        assert_eq!(art.rows.len(), PIXEL_ART_SIZE as usize);
        assert!(
            art.rows
                .iter()
                .all(|row| row.len() == PIXEL_ART_SIZE as usize)
        );
        assert_eq!(art.palette, vec!["#123456".to_string()]);
        assert!(art.rows.iter().any(|row| row.contains('1')));
        assert!(art.rows.iter().any(|row| row.contains('0')));

        fs::remove_file(image_path).expect("remove image");
        fs::remove_dir(dir).expect("remove temp dir");
    }

    #[test]
    fn ignores_transparent_provider_image_rgb() {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("create temp dir");
        let image_path = dir.join("transparent-provider.png");
        let mut image = RgbaImage::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                image.put_pixel(x, y, image::Rgba([0, 255, 0, 0]));
            }
        }
        for y in 2..6 {
            for x in 2..6 {
                image.put_pixel(x, y, image::Rgba([0x12, 0x34, 0x56, 0xff]));
            }
        }
        image.save(&image_path).expect("save image");

        let art = pixel_art_from_image(&image_path).expect("convert image");

        assert_eq!(art.palette, vec!["#123456".to_string()]);

        fs::remove_file(image_path).expect("remove image");
        fs::remove_dir(dir).expect("remove temp dir");
    }

    #[test]
    fn provider_image_id_tracks_wire_art_content() {
        let art = UsagePixelArt {
            palette: vec!["#123456".to_string()],
            rows: vec!["10".to_string(), "01".to_string()],
        };
        let same_art = art.clone();
        let changed_art = UsagePixelArt {
            rows: vec!["11".to_string(), "01".to_string()],
            ..art.clone()
        };

        assert_eq!(
            provider_image_id(&art).expect("image id"),
            provider_image_id(&same_art).expect("same image id")
        );
        assert_ne!(
            provider_image_id(&art).expect("image id"),
            provider_image_id(&changed_art).expect("changed image id")
        );
    }

    #[test]
    fn applies_and_strips_configured_provider_images() {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("config.toml");
        let image_path = dir.join("provider.png");
        let mut image = RgbaImage::new(4, 4);
        image.put_pixel(1, 1, image::Rgba([0x12, 0x34, 0x56, 0xff]));
        image.save(&image_path).expect("save image");
        fs::write(&config_path, "[usage.images]\ncodex = \"provider.png\"\n")
            .expect("write config");
        let mut snapshot = UsageSnapshot {
            providers: vec![UsageProvider {
                id: codex::PROVIDER_ID.to_string(),
                label: "CODEX".to_string(),
                theme_color: None,
                theme: None,
                pixel_art: None,
                source: "test".to_string(),
                account: None,
                plan: None,
                windows: Vec::new(),
            }],
            updated_at: "test".to_string(),
            updated_at_unix: 1,
        };

        apply_configured_images(&mut snapshot, &config_path, true).expect("attach image");
        assert!(snapshot.providers[0].pixel_art.is_some());

        apply_configured_images(&mut snapshot, &config_path, false).expect("strip image");
        assert!(snapshot.providers[0].pixel_art.is_none());

        fs::remove_file(image_path).expect("remove image");
        fs::remove_file(config_path).expect("remove config");
        fs::remove_dir(dir).expect("remove temp dir");
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

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("quota-dock-usage-test-{nanos}"))
    }
}
