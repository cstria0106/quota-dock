use crate::app::renderer::{Scene, UiObject};
use crate::app::status::waiting_scene;
use crate::app::ui::{color, rgb565, Color, FontFace, TextAlign, UiCanvas, UI_HEIGHT, UI_WIDTH};
use crate::network::{UsageProvider, UsageSnapshot, UsageWindow};

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
const PRIMARY_PROGRESS_OFFSET_Y: i32 = 3;
const SECONDARY_PROGRESS_OFFSET_Y: i32 = -1;
const RESET_SCALE: i32 = 2;
const PRIMARY_ART_SIZE: i32 = 96;

pub fn usage_scene(
    snapshot: &UsageSnapshot,
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
    let mut scene = Scene::new();
    push_usage_layout(
        &mut scene,
        provider,
        &windows,
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
    provider: &UsageProvider,
    windows: &[&UsageWindow],
    theme: &ThemeColors,
    current_unix: u64,
) {
    let x = SCREEN_PAD_X;
    let width = UI_WIDTH as i32 - SCREEN_PAD_X * 2;
    let screen_h = UI_HEIGHT as i32;
    let primary_h = PRIMARY_ART_SIZE + PRIMARY_ART_GUTTER_Y * 2;

    if windows.len() == 1 {
        let card_h = primary_h;
        let card_y = (screen_h - card_h) / 2;
        push_primary_panel(
            scene,
            x,
            card_y,
            width,
            card_h,
            provider,
            windows[0],
            theme,
            current_unix,
        );
        return;
    }

    let secondary_h = secondary_panel_height(&windows[1..], current_unix);
    let stack_h = primary_h + PANEL_GAP + secondary_h;
    let y = (screen_h - stack_h).max(0) / 2;

    push_primary_panel(
        scene,
        x,
        y,
        width,
        primary_h,
        provider,
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

    let secondary_w = (width - SECONDARY_CARD_GAP) / 2;
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
    provider: &UsageProvider,
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
    let art_y = y + (height - PRIMARY_ART_SIZE) / 2;
    let has_art = push_pixel_art(scene, provider, art_x, art_y, PRIMARY_ART_SIZE, theme);
    let body_x = if has_art {
        art_x + PRIMARY_ART_SIZE + PRIMARY_PANEL_GUTTER_X
    } else {
        x + PRIMARY_PANEL_GUTTER_X + ROUNDED_RECT_EDGE_INSET
    };
    let body_w = x + width - body_x - PRIMARY_PANEL_GUTTER_X;

    push_usage_content(
        scene,
        RectSpec::new(body_x, art_y, body_w, PRIMARY_ART_SIZE),
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
    let percent_font = if primary {
        FontFace::DEFAULT
    } else {
        FontFace::Galmuri7
    };
    let percent_scale = if primary { 3 } else { 2 };
    let reset_font = if primary {
        FontFace::DEFAULT
    } else {
        FontFace::Galmuri7
    };
    let reset_scale = RESET_SCALE;
    let progress_h = PROGRESS_H;

    let (pill_y, reset_y, progress_y) = if primary {
        let pill_y = rect.y - ROUNDED_RECT_EDGE_INSET;
        let reset_bottom_y = rect.y + rect.h;
        let reset_y = reset_text.as_ref().map(|reset_text| {
            align_text_ink_bottom(reset_bottom_y, reset_text.as_str(), reset_font, reset_scale)
        });
        let reset_line_y = reset_y.unwrap_or(reset_bottom_y);
        let progress_y =
            ((pill_y + pill_h + reset_line_y - progress_h) / 2).max(pill_y + pill_h + 4);
        (pill_y, reset_y, progress_y)
    } else {
        let reset_ink_h = reset_text
            .as_deref()
            .map(|reset_text| text_ink_height(reset_text, reset_font, reset_scale))
            .unwrap_or_default();
        let group_h =
            pill_h + SECONDARY_CONTENT_GAP + progress_h + SECONDARY_CONTENT_GAP + reset_ink_h;
        let group_y = rect.y + (rect.h - group_h).max(0) / 2;
        let pill_y = group_y;
        let progress_y = pill_y + pill_h + SECONDARY_CONTENT_GAP;
        let reset_bottom_y = progress_y + progress_h + SECONDARY_CONTENT_GAP + reset_ink_h;
        let reset_y = reset_text.as_ref().map(|reset_text| {
            align_text_ink_bottom(reset_bottom_y, reset_text.as_str(), reset_font, reset_scale)
        });
        (pill_y, reset_y, progress_y)
    };
    let progress_y = progress_y
        + if primary {
            PRIMARY_PROGRESS_OFFSET_Y
        } else {
            SECONDARY_PROGRESS_OFFSET_Y
        };
    let percent_y = if primary {
        align_text_ink_top(rect.y, percent_text.as_str(), percent_font, percent_scale)
    } else {
        align_text_ink_center(
            pill_y,
            pill_h,
            percent_text.as_str(),
            percent_font,
            percent_scale,
        )
    };

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

fn push_pixel_art(
    scene: &mut Scene,
    provider: &UsageProvider,
    x: i32,
    y: i32,
    size: i32,
    theme: &ThemeColors,
) -> bool {
    let Some(art) = provider.pixel_art.as_ref() else {
        return false;
    };
    let width = art
        .rows
        .iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap_or(0) as i32;
    let height = art.rows.len() as i32;
    let max_side = width.max(height);
    if max_side <= 0 {
        return false;
    }

    let pixel = (size / max_side).max(1);
    let drawn_w = width * pixel;
    let drawn_h = height * pixel;
    let start_x = x + (size - drawn_w) / 2;
    let start_y = y + (size - drawn_h) / 2;
    let art_color = parse_hex_color(art.color.as_str()).unwrap_or(theme.accent);
    scene.push(UiObject::pixel_art(
        start_x,
        start_y,
        pixel,
        art.rows.clone(),
        art_color,
    ));
    true
}

fn drawable_windows(provider: &UsageProvider) -> Vec<&UsageWindow> {
    provider
        .windows
        .iter()
        .filter(|window| !window.status.eq_ignore_ascii_case("error"))
        .take(3)
        .collect()
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

fn secondary_panel_height(windows: &[&UsageWindow], current_unix: u64) -> i32 {
    windows
        .iter()
        .map(|window| secondary_content_height(window, current_unix))
        .max()
        .unwrap_or_default()
        + SECONDARY_PANEL_GUTTER_Y * 2
}

fn secondary_content_height(window: &UsageWindow, current_unix: u64) -> i32 {
    let reset_ink_h = reset_label(window, current_unix)
        .as_deref()
        .map(|reset_text| text_ink_height(reset_text, FontFace::Galmuri7, RESET_SCALE))
        .unwrap_or_default();
    USAGE_PILL_H + SECONDARY_CONTENT_GAP + PROGRESS_H + SECONDARY_CONTENT_GAP + reset_ink_h
}

fn align_text_ink_top(container_y: i32, text: &str, font: FontFace, text_scale: i32) -> i32 {
    let Some((top, _)) = UiCanvas::text_ink_bounds_y(text, font, text_scale) else {
        return container_y;
    };
    container_y - top
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

fn text_ink_height(text: &str, font: FontFace, text_scale: i32) -> i32 {
    UiCanvas::text_ink_bounds_y(text, font, text_scale)
        .map(|(top, bottom)| bottom - top)
        .unwrap_or_else(|| UiCanvas::text_height_for(font, text_scale, 1))
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
