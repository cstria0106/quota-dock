use crate::app::renderer::{Scene, UiObject};
use crate::app::status::waiting_scene;
use crate::app::text;
use crate::app::ui::{color, Color, TextAlign, UI_WIDTH};
use crate::network::{UsageProvider, UsageSnapshot, UsageWindow};

pub fn usage_scene(snapshot: &UsageSnapshot, selected_provider: usize) -> Scene {
    if snapshot.providers.is_empty() {
        return waiting_scene();
    }

    let selected_provider = selected_provider.min(snapshot.providers.len() - 1);
    let provider = &snapshot.providers[selected_provider];
    let primary = focus_window(provider);
    let primary_percent = primary.map(|window| window.used_percent).unwrap_or(0);
    let accent = color_for(primary_percent);
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

    push_window_bar(
        &mut scene,
        216,
        92,
        text::WINDOW_5H,
        provider,
        "5h",
        color::TEAL,
        210,
    );
    push_window_bar(
        &mut scene,
        216,
        164,
        text::WINDOW_7D,
        provider,
        "7d",
        color::LAVENDER,
        210,
    );

    push_provider_strip(&mut scene, snapshot, selected_provider);
    scene
}

fn push_window_bar(
    scene: &mut Scene,
    x: i32,
    y: i32,
    label: &str,
    provider: &UsageProvider,
    kind: &str,
    fallback: Color,
    bar_width: i32,
) {
    let window = provider
        .windows
        .iter()
        .find(|window| window.kind.eq_ignore_ascii_case(kind));
    let percent = window.map(|window| window.used_percent).unwrap_or(0);
    let fill_color = if percent == 0 {
        fallback
    } else {
        color_for(percent)
    };
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
    let count = snapshot.providers.len().min(3);
    if count == 0 {
        return;
    }

    let card_width = 86;
    let gap = 8;
    let start_x = (UI_WIDTH as i32 - (count as i32 * card_width + (count as i32 - 1) * gap)) / 2;

    for index in 0..count {
        let provider = &snapshot.providers[index];
        let x = start_x + index as i32 * (card_width + gap);
        let percent = focus_window(provider)
            .map(|window| window.used_percent)
            .unwrap_or(0);
        let card_color = if index == selected_provider {
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
            color_for(percent),
        ));
    }
}

fn focus_window(provider: &UsageProvider) -> Option<&UsageWindow> {
    provider
        .windows
        .iter()
        .max_by_key(|window| window.used_percent)
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

fn color_for(percent: u8) -> Color {
    match percent {
        0..=54 => color::MINT,
        55..=79 => color::AMBER,
        80..=100 => color::CORAL,
        _ => color::TEAL,
    }
}
