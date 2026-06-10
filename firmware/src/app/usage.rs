use crate::app::renderer::{Scene, UiObject};
use crate::app::status::waiting_scene;
use crate::app::text;
use crate::app::ui::{color, rgb565, Color, TextAlign, UI_WIDTH};
use crate::network::{UsageProvider, UsageSnapshot, UsageWindow};

pub fn usage_scene(snapshot: &UsageSnapshot, selected_provider: usize) -> Scene {
    let Some(selected_provider) = normalize_selected_provider(snapshot, selected_provider) else {
        return waiting_scene();
    };

    let provider = &snapshot.providers[selected_provider];
    let primary = focus_window(provider);
    let primary_percent = primary.map(|window| window.used_percent).unwrap_or(0);
    let accent = provider_color(provider, primary_percent);
    let percent_text = format!("{primary_percent}%");
    let source = status_label(provider, primary);
    let width = UI_WIDTH as i32;
    let mut scene = Scene::new();

    scene.push(UiObject::text(
        24,
        18,
        260,
        provider.label.as_str(),
        1,
        accent,
        TextAlign::Left,
    ));
    if !source.is_empty() {
        scene.push(UiObject::text(
            width - 62,
            20,
            38,
            source,
            1,
            color::MUTED,
            TextAlign::Right,
        ));
    }
    scene.push(UiObject::text(
        24,
        96,
        164,
        percent_text,
        3,
        color::TEXT,
        TextAlign::Center,
    ));

    push_window_bar(&mut scene, 216, 92, text::WINDOW_5H, provider, "5h", 210);
    push_window_bar(&mut scene, 216, 164, text::WINDOW_7D, provider, "7d", 210);

    push_provider_strip(&mut scene, snapshot, selected_provider);
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

fn push_window_bar(
    scene: &mut Scene,
    x: i32,
    y: i32,
    label: &str,
    provider: &UsageProvider,
    kind: &str,
    bar_width: i32,
) {
    let window = provider
        .windows
        .iter()
        .find(|window| window.kind.eq_ignore_ascii_case(kind));
    let percent = window.map(|window| window.used_percent).unwrap_or(0);
    let fill_color = provider_color(provider, percent);
    let percent_text = format!("{percent}%");

    scene.push(UiObject::text(
        x,
        y - 25,
        56,
        label,
        1,
        color::TEXT,
        TextAlign::Left,
    ));
    scene.push(UiObject::text(
        x + bar_width - 56,
        y - 22,
        56,
        percent_text,
        1,
        color::TEXT,
        TextAlign::Right,
    ));
    scene.push(UiObject::meter(
        x,
        y,
        bar_width,
        29,
        percent,
        fill_color,
        color::PANEL,
    ));
}

fn push_provider_strip(scene: &mut Scene, snapshot: &UsageSnapshot, selected_provider: usize) {
    let visible_providers: heapless::Vec<usize, 3> = snapshot
        .providers
        .iter()
        .enumerate()
        .filter_map(|(index, provider)| is_drawable_provider(provider).then_some(index))
        .take(3)
        .collect();
    let count = visible_providers.len();
    if count <= 1 {
        return;
    }

    let card_width = 86;
    let gap = 8;
    let start_x = (UI_WIDTH as i32 - (count as i32 * card_width + (count as i32 - 1) * gap)) / 2;

    for (card_index, provider_index) in visible_providers.iter().enumerate() {
        let provider = &snapshot.providers[*provider_index];
        let x = start_x + card_index as i32 * (card_width + gap);
        let percent = focus_window(provider)
            .map(|window| window.used_percent)
            .unwrap_or(0);
        let card_color = if *provider_index == selected_provider {
            color::PANEL_DIM
        } else {
            color::PANEL
        };

        scene.push(UiObject::rect(x, 222, card_width, 42, card_color));
        scene.push(UiObject::text(
            x + 7,
            230,
            card_width - 14,
            provider.id.as_str(),
            1,
            color::TEXT,
            TextAlign::Center,
        ));
        scene.push(UiObject::meter_fill(
            x + 7,
            252,
            card_width - 14,
            6,
            percent,
            provider_color(provider, percent),
        ));
    }
}

fn focus_window(provider: &UsageProvider) -> Option<&UsageWindow> {
    provider
        .windows
        .iter()
        .max_by_key(|window| window.used_percent)
}

fn is_drawable_provider(provider: &UsageProvider) -> bool {
    !provider.source.eq_ignore_ascii_case("unavailable")
        && provider
            .windows
            .iter()
            .any(|window| !window.status.eq_ignore_ascii_case("error"))
}

fn status_label(provider: &UsageProvider, window: Option<&UsageWindow>) -> &'static str {
    if window
        .map(|window| window.status.eq_ignore_ascii_case("error"))
        .unwrap_or(false)
    {
        return text::STATUS_ERR;
    }
    if provider.source.eq_ignore_ascii_case("local-estimate") {
        return text::STATUS_EST;
    }
    ""
}

fn provider_color(provider: &UsageProvider, percent: u8) -> Color {
    provider
        .theme_color
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or_else(|| usage_color_for(percent))
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

fn usage_color_for(percent: u8) -> Color {
    match percent {
        0..=54 => color::MINT,
        55..=79 => color::AMBER,
        80..=100 => color::CORAL,
        _ => color::TEAL,
    }
}
