use std::sync::Arc;

use crate::app::renderer::{Scene, UiObject};
use crate::app::status::waiting_scene;
use crate::app::ui::{
    color, rgb565, Color, FontFace, Rect, TextAlign, UiCanvas, UI_HEIGHT, UI_WIDTH,
};
use crate::network::{UsagePixelArt, UsageProvider, UsageSnapshot, UsageWindow};
use heapless::Vec as FixedVec;

const SCREEN_PAD_X: i32 = 20;
const PANEL_GAP: i32 = 8;
const SECONDARY_CARD_GAP: i32 = 7;
const CARD_RADIUS: i32 = 10;
const ROUNDED_RECT_EDGE_INSET: i32 = 1;
const ROUNDED_METER_LEFT_INSET: i32 = 1;
const PRIMARY_PANEL_GUTTER_X: i32 = 24;
const PRIMARY_ART_GUTTER_Y: i32 = 24;
const SECONDARY_PANEL_GUTTER_X: i32 = 24;
const SECONDARY_PANEL_GUTTER_Y: i32 = 16;
const SECONDARY_CONTENT_GAP: i32 = 7;
const USAGE_PILL_H: i32 = 26;
const USAGE_PILL_RADIUS: i32 = 6;
const USAGE_PILL_TEXT_PAD_X: i32 = 5;
const PROGRESS_H: i32 = 12;
const PROGRESS_RADIUS: i32 = 3;
const RESET_SCALE: i32 = 2;
const PRIMARY_CONTENT_MIN_H: i32 = 96;
const PRIMARY_ART_SIZE: i32 = 96;
const MAX_PROVIDER_IMAGE_SIDE: usize = PRIMARY_ART_SIZE as usize;
const MAX_CACHED_PROVIDER_IMAGES: usize = 3;

#[derive(Default)]
pub struct ProviderImageCache {
    entries: Vec<CachedProviderImage>,
}

#[derive(Clone)]
struct CachedProviderImage {
    provider_id: String,
    art: PackedPixelArt,
}

#[derive(Clone, PartialEq)]
struct PackedPixelArt {
    width: i32,
    height: i32,
    cells: Arc<[u8]>,
    palette: Arc<[Color]>,
}

pub fn cache_provider_images(snapshot: &mut UsageSnapshot, cache: &mut ProviderImageCache) {
    for provider in &mut snapshot.providers {
        let Some(pixel_art) = provider.pixel_art.take() else {
            continue;
        };
        if let Some(art) = PackedPixelArt::from_wire(&pixel_art) {
            cache.upsert(provider.id.as_str(), art);
        }
    }
}

impl ProviderImageCache {
    fn get(&self, provider_id: &str) -> Option<&PackedPixelArt> {
        self.entries
            .iter()
            .find(|entry| entry.provider_id.eq_ignore_ascii_case(provider_id))
            .map(|entry| &entry.art)
    }

    fn upsert(&mut self, provider_id: &str, art: PackedPixelArt) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.provider_id.eq_ignore_ascii_case(provider_id))
        {
            entry.art = art;
            return;
        }

        self.entries.push(CachedProviderImage {
            provider_id: provider_id.to_string(),
            art,
        });
        if self.entries.len() > MAX_CACHED_PROVIDER_IMAGES {
            self.entries.remove(0);
        }
    }
}

impl PackedPixelArt {
    fn from_wire(art: &UsagePixelArt) -> Option<Self> {
        let palette = art
            .palette
            .iter()
            .map(|color| parse_hex_color(color.as_str()))
            .collect::<Option<Vec<_>>>()?;
        if palette.is_empty() {
            return None;
        }
        let palette_len = palette.len();

        let width = art.rows.iter().map(|row| row.chars().count()).max()?;
        let height = art.rows.len();
        if width == 0
            || height == 0
            || width > MAX_PROVIDER_IMAGE_SIDE
            || height > MAX_PROVIDER_IMAGE_SIDE
        {
            return None;
        }

        let mut cells = Vec::with_capacity(width * height);
        for row in &art.rows {
            let mut row_width = 0;
            for cell in row.chars() {
                cells.push(palette_index(cell, palette_len).unwrap_or_default());
                row_width += 1;
            }
            if row_width > width {
                return None;
            }
            cells.resize(cells.len() + width - row_width, 0);
        }

        Some(Self {
            width: width as i32,
            height: height as i32,
            cells: cells.into(),
            palette: palette.into(),
        })
    }
}

fn palette_index(cell: char, palette_len: usize) -> Option<u8> {
    let index = match cell {
        '1'..='9' => cell as usize - '1' as usize,
        'A'..='Z' => cell as usize - 'A' as usize + 9,
        'a'..='z' => cell as usize - 'a' as usize + 35,
        _ => return None,
    };
    (index < palette_len).then_some(index as u8 + 1)
}

pub fn usage_scene(
    snapshot: &UsageSnapshot,
    image_cache: &ProviderImageCache,
    selected_provider: usize,
    elapsed_since_update_secs: u64,
) -> Scene {
    let Some(selected_provider) = normalize_selected_provider(snapshot, selected_provider) else {
        return waiting_scene();
    };

    let provider = &snapshot.providers[selected_provider];
    let windows = drawable_windows(provider);
    if windows.is_empty() {
        return waiting_scene();
    }

    let theme = provider_theme(provider);
    let pixel_art = image_cache.get(provider.id.as_str());
    let mut scene = Scene::new();
    push_usage_layout(
        &mut scene,
        pixel_art,
        windows.as_slice(),
        &theme,
        snapshot
            .updated_at_unix
            .saturating_add(elapsed_since_update_secs),
    );
    scene
}

pub fn normalize_selected_provider(
    snapshot: &UsageSnapshot,
    selected_provider: usize,
) -> Option<usize> {
    snapshot
        .providers
        .get(selected_provider)
        .filter(|provider| is_drawable_provider(provider))
        .map(|_| selected_provider)
        .or_else(|| snapshot.providers.iter().position(is_drawable_provider))
}

pub fn next_provider_index(snapshot: &UsageSnapshot, selected_provider: usize) -> Option<usize> {
    let current = normalize_selected_provider(snapshot, selected_provider)?;
    let mut drawable_indices = snapshot
        .providers
        .iter()
        .enumerate()
        .filter_map(|(index, provider)| is_drawable_provider(provider).then_some(index));

    let first = drawable_indices.next()?;
    let second = drawable_indices.next()?;
    if drawable_indices.next().is_none() {
        return Some(if current == first { second } else { first });
    }

    Some(
        snapshot
            .providers
            .iter()
            .enumerate()
            .skip(current + 1)
            .chain(snapshot.providers.iter().enumerate())
            .find_map(|(index, provider)| is_drawable_provider(provider).then_some(index))
            .unwrap_or(current),
    )
}

fn push_usage_layout(
    scene: &mut Scene,
    pixel_art: Option<&PackedPixelArt>,
    windows: &[&UsageWindow],
    theme: &ThemeColors,
    current_unix: u64,
) {
    let x = SCREEN_PAD_X;
    let width = UI_WIDTH as i32 - SCREEN_PAD_X * 2;
    let screen_h = UI_HEIGHT as i32;
    let primary_h = primary_panel_height(pixel_art, windows[0]);

    if windows.len() == 1 {
        let card_h = primary_h;
        push_primary_panel(
            scene,
            x,
            single_panel_y(card_h, screen_h),
            width,
            card_h,
            pixel_art,
            windows[0],
            theme,
            current_unix,
        );
        return;
    }

    let secondary_h = secondary_panel_height(&windows[1..]);
    let y = stack_y(primary_h, secondary_h, screen_h);

    push_primary_panel(
        scene,
        x,
        y,
        width,
        primary_h,
        pixel_art,
        windows[0],
        theme,
        current_unix,
    );

    let secondary_y = y + primary_h + PANEL_GAP;
    if windows.len() == 2 {
        push_usage_card(
            scene,
            RectSpec::new(x, secondary_y, width, secondary_h),
            windows[1],
            theme,
            current_unix,
        );
        return;
    }

    let secondary_w = secondary_card_width(width);
    push_usage_card(
        scene,
        RectSpec::new(x, secondary_y, secondary_w, secondary_h),
        windows[1],
        theme,
        current_unix,
    );
    push_usage_card(
        scene,
        RectSpec::new(
            x + secondary_w + SECONDARY_CARD_GAP,
            secondary_y,
            width - secondary_w - SECONDARY_CARD_GAP,
            secondary_h,
        ),
        windows[2],
        theme,
        current_unix,
    );
}

fn push_primary_panel(
    scene: &mut Scene,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pixel_art: Option<&PackedPixelArt>,
    window: &UsageWindow,
    theme: &ThemeColors,
    current_unix: u64,
) {
    scene.push(UiObject::rounded_rect(
        x,
        y,
        width,
        height,
        CARD_RADIUS + 2,
        theme.primary_panel,
    ));

    let art_x = x + PRIMARY_PANEL_GUTTER_X + ROUNDED_RECT_EDGE_INSET;
    let art_y = y + centered_offset(height, PRIMARY_ART_SIZE);
    let has_art = pixel_art.is_some();
    if let Some(pixel_art) = pixel_art {
        push_pixel_art(scene, pixel_art, art_x, art_y, PRIMARY_ART_SIZE);
    }

    let body_x = if has_art {
        art_x + PRIMARY_ART_SIZE + PRIMARY_PANEL_GUTTER_X
    } else {
        x + PRIMARY_PANEL_GUTTER_X
    };
    let body_w = x + width - body_x - PRIMARY_PANEL_GUTTER_X;
    let body_h = primary_content_height();
    let body_y = y + centered_offset(height, body_h);

    push_usage_content(
        scene,
        RectSpec::new(body_x, body_y, body_w, body_h),
        window,
        theme,
        true,
        current_unix,
    );
}

fn push_usage_card(
    scene: &mut Scene,
    rect: RectSpec,
    window: &UsageWindow,
    theme: &ThemeColors,
    current_unix: u64,
) {
    scene.push(UiObject::rounded_rect(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        CARD_RADIUS,
        theme.primary_panel,
    ));
    push_usage_content(
        scene,
        rect.inset_x(
            SECONDARY_PANEL_GUTTER_X + ROUNDED_RECT_EDGE_INSET,
            SECONDARY_PANEL_GUTTER_X,
        ),
        window,
        theme,
        false,
        current_unix,
    );
}

fn push_usage_content(
    scene: &mut Scene,
    rect: RectSpec,
    window: &UsageWindow,
    theme: &ThemeColors,
    primary: bool,
    current_unix: u64,
) {
    let percent_text = format!("{}%", window.used_percent);
    let reset_text = reset_label(window, current_unix);
    let pill_scale = 2;
    let pill_font = FontFace::Galmuri7;
    let pill_h = USAGE_PILL_H;
    let pill_w = pill_width(window.label.as_str(), rect.w, pill_scale);
    let percent_font = percent_font(primary);
    let percent_scale = percent_scale(primary);
    let reset_font = reset_font(primary);
    let reset_scale = RESET_SCALE;
    let reset_slot_h = reset_slot_height(primary);
    let progress_h = PROGRESS_H;
    let content_h = if primary {
        primary_content_height()
    } else {
        secondary_content_height()
    };
    let content_y = rect.y + centered_offset(rect.h, content_h);
    let top_slot_h = top_slot_height(primary);
    let top_slot_y = content_y;
    let pill_y = top_slot_y + centered_offset(top_slot_h, pill_h);
    let reset_slot_y = content_y + content_h - reset_slot_h;
    let reset_bottom_y = reset_slot_y + reset_slot_h;
    let reset_y = reset_text.as_ref().map(|reset_text| {
        align_text_ink_bottom(reset_bottom_y, reset_text.as_str(), reset_font, reset_scale)
    });
    let progress_y = centered_between(top_slot_y + top_slot_h, reset_slot_y, progress_h);
    let percent_y = align_text_ink_center(
        top_slot_y,
        top_slot_h,
        percent_text.as_str(),
        percent_font,
        percent_scale,
    );

    scene.push(UiObject::text_with_font(
        rect.x,
        percent_y,
        118,
        percent_text,
        percent_font,
        percent_scale,
        color::TEXT,
        TextAlign::Left,
    ));
    scene.push(UiObject::rounded_rect(
        rect.x + rect.w - pill_w,
        pill_y,
        pill_w,
        pill_h,
        USAGE_PILL_RADIUS,
        theme.pill,
    ));
    scene.push(UiObject::text_with_font(
        rect.x + rect.w - pill_w + USAGE_PILL_TEXT_PAD_X,
        align_text_ink_center(pill_y, pill_h, window.label.as_str(), pill_font, pill_scale),
        pill_w - USAGE_PILL_TEXT_PAD_X * 2,
        window.label.as_str(),
        pill_font,
        pill_scale,
        color::TEXT,
        TextAlign::Center,
    ));
    scene.push(UiObject::rounded_meter_fill(
        rect.x - ROUNDED_METER_LEFT_INSET,
        progress_y,
        rect.w + ROUNDED_METER_LEFT_INSET,
        progress_h,
        window.used_percent,
        PROGRESS_RADIUS,
        theme.accent,
        theme.track,
    ));

    if let (Some(reset_text), Some(reset_y)) = (reset_text, reset_y) {
        scene.push(UiObject::text_with_font(
            rect.x,
            reset_y,
            rect.w,
            reset_text,
            reset_font,
            reset_scale,
            color::MUTED,
            TextAlign::Left,
        ));
    }
}

fn push_pixel_art(scene: &mut Scene, art: &PackedPixelArt, x: i32, y: i32, size: i32) -> bool {
    let max_side = art.width.max(art.height);
    let pixel = (size / max_side).max(1);
    let drawn_w = art.width * pixel;
    let drawn_h = art.height * pixel;
    let start_x = x + (size - drawn_w) / 2;
    let start_y = y + (size - drawn_h) / 2;
    scene.push(UiObject::pixel_art(
        Rect::new(x, y, size, size),
        start_x,
        start_y,
        pixel,
        art.width,
        art.height,
        art.cells.clone(),
        art.palette.clone(),
    ));
    true
}

fn drawable_windows(provider: &UsageProvider) -> FixedVec<&UsageWindow, 3> {
    let mut windows = FixedVec::new();
    for window in &provider.windows {
        if window.status.eq_ignore_ascii_case("error") {
            continue;
        }
        if windows.push(window).is_err() {
            break;
        }
    }
    windows
}

fn is_drawable_provider(provider: &UsageProvider) -> bool {
    !provider.source.eq_ignore_ascii_case("unavailable")
        && provider
            .windows
            .iter()
            .any(|window| !window.status.eq_ignore_ascii_case("error"))
}

fn reset_label(window: &UsageWindow, current_unix: u64) -> Option<String> {
    if window
        .resets_at
        .as_deref()
        .map(str::trim)
        .map(|reset| reset.eq_ignore_ascii_case("rolling"))
        .unwrap_or(false)
    {
        return Some("Rolling window".to_string());
    }

    let timestamp = window.resets_at_unix.or_else(|| {
        window
            .resets_at
            .as_deref()
            .map(str::trim)
            .and_then(|reset| reset.strip_prefix("unix:"))
            .and_then(|timestamp| timestamp.parse::<u64>().ok())
    })?;
    if current_unix == 0 {
        return None;
    }
    if timestamp <= current_unix {
        return Some("Resets soon".to_string());
    }

    let remaining = timestamp - current_unix;
    let days = remaining / 86_400;
    let hours = (remaining % 86_400) / 3_600;
    let minutes = (remaining % 3_600) / 60;

    if days > 0 {
        Some(format!("Resets in {days}d {hours}h"))
    } else if hours > 0 {
        Some(format!("Resets in {hours}h {minutes}m"))
    } else {
        Some(format!("Resets in {minutes}m"))
    }
}

fn pill_width(label: &str, max_width: i32, scale: i32) -> i32 {
    let width = label.chars().count() as i32 * 10 * scale + USAGE_PILL_TEXT_PAD_X * 2;
    width.clamp(48, max_width / 2)
}

fn single_panel_y(card_h: i32, screen_h: i32) -> i32 {
    centered_offset(screen_h, card_h)
}

fn stack_y(primary_h: i32, secondary_h: i32, screen_h: i32) -> i32 {
    centered_offset(screen_h, primary_h + PANEL_GAP + secondary_h)
}

fn secondary_card_width(width: i32) -> i32 {
    (width - SECONDARY_CARD_GAP) / 2
}

fn secondary_panel_height(_windows: &[&UsageWindow]) -> i32 {
    secondary_content_height() + SECONDARY_PANEL_GUTTER_Y * 2
}

fn primary_panel_height(_pixel_art: Option<&PackedPixelArt>, _window: &UsageWindow) -> i32 {
    primary_content_height().max(PRIMARY_ART_SIZE) + PRIMARY_ART_GUTTER_Y * 2
}

fn primary_content_height() -> i32 {
    metric_content_height(true).max(PRIMARY_CONTENT_MIN_H)
}

fn secondary_content_height() -> i32 {
    metric_content_height(false)
}

fn metric_content_height(primary: bool) -> i32 {
    top_slot_height(primary)
        + SECONDARY_CONTENT_GAP
        + PROGRESS_H
        + SECONDARY_CONTENT_GAP
        + reset_slot_height(primary)
}

fn top_slot_height(primary: bool) -> i32 {
    UiCanvas::text_height_for(percent_font(primary), percent_scale(primary), 1).max(USAGE_PILL_H)
}

fn percent_font(primary: bool) -> FontFace {
    if primary {
        FontFace::DEFAULT
    } else {
        FontFace::Galmuri7
    }
}

fn percent_scale(primary: bool) -> i32 {
    if primary {
        3
    } else {
        2
    }
}

fn reset_font(primary: bool) -> FontFace {
    if primary {
        FontFace::DEFAULT
    } else {
        FontFace::Galmuri7
    }
}

fn reset_slot_height(primary: bool) -> i32 {
    UiCanvas::text_height_for(reset_font(primary), RESET_SCALE, 1)
}

fn centered_offset(container: i32, child: i32) -> i32 {
    (container - child).max(0) / 2
}

fn centered_between(top: i32, bottom: i32, height: i32) -> i32 {
    top + centered_offset(bottom - top, height)
}

fn align_text_ink_bottom(
    container_bottom: i32,
    text: &str,
    font: FontFace,
    text_scale: i32,
) -> i32 {
    let Some((_, bottom)) = UiCanvas::text_ink_bounds_y(text, font, text_scale) else {
        return container_bottom - UiCanvas::text_height_for(font, text_scale, 1);
    };
    container_bottom - bottom
}

fn align_text_ink_center(
    container_y: i32,
    container_h: i32,
    text: &str,
    font: FontFace,
    text_scale: i32,
) -> i32 {
    let Some((top, bottom)) = UiCanvas::text_ink_bounds_y(text, font, text_scale) else {
        return container_y + (container_h - UiCanvas::text_height_for(font, text_scale, 1)) / 2;
    };
    container_y + (container_h - (bottom - top)) / 2 - top
}

fn provider_theme(provider: &UsageProvider) -> ThemeColors {
    let accent = provider
        .theme
        .as_ref()
        .and_then(|theme| parse_hex_color(theme.accent.as_str()))
        .or_else(|| provider.theme_color.as_deref().and_then(parse_hex_color))
        .unwrap_or(color::TEAL);

    ThemeColors {
        accent,
        primary_panel: provider
            .theme
            .as_ref()
            .and_then(|theme| parse_hex_color(theme.primary_panel.as_str()))
            .unwrap_or(color::PANEL_DIM),
        track: provider
            .theme
            .as_ref()
            .and_then(|theme| parse_hex_color(theme.track.as_str()))
            .unwrap_or(color::PANEL_DIM),
        pill: provider
            .theme
            .as_ref()
            .and_then(|theme| parse_hex_color(theme.pill.as_str()))
            .unwrap_or(color::PANEL_DIM),
    }
}

fn parse_hex_color(value: &str) -> Option<Color> {
    let hex = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if hex.len() != 6 {
        return None;
    }

    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(rgb565(red, green, blue))
}

struct ThemeColors {
    accent: Color,
    primary_panel: Color,
    track: Color,
    pill: Color,
}

#[derive(Clone, Copy)]
struct RectSpec {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl RectSpec {
    const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    const fn inset_x(self, left: i32, right: i32) -> Self {
        Self {
            x: self.x + left,
            y: self.y,
            w: self.w - left - right,
            h: self.h,
        }
    }
}
