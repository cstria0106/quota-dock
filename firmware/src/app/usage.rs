use crate::app::text;
use crate::app::status::draw_waiting;
use crate::app::ui::{color, Color, TextAlign, UiCanvas};
use crate::drivers::display::{EspResult, Sh8601};
use crate::network::{UsageProvider, UsageSnapshot, UsageWindow};

pub fn draw_usage_snapshot(
    panel: &Sh8601,
    snapshot: &UsageSnapshot,
    selected_provider: usize,
) -> EspResult {
    if snapshot.providers.is_empty() {
        return draw_waiting(panel);
    }

    let selected_provider = selected_provider.min(snapshot.providers.len() - 1);
    let provider = &snapshot.providers[selected_provider];
    let primary = focus_window(provider);
    let primary_percent = primary.map(|window| window.used_percent).unwrap_or(0);
    let accent = color_for(primary_percent);
    let percent_text = format!("{primary_percent}%");
    let source = status_label(provider, primary);

    panel.draw_rows(|output, y, rows| {
        let mut ui = UiCanvas::new(output, y, rows);
        ui.dotted_background();
        let width = ui.width();
        ui.text(
            24,
            18,
            260,
            provider.label.as_str(),
            1,
            accent,
            TextAlign::Left,
        );
        if !source.is_empty() {
            ui.text(width - 62, 20, 38, source, 1, color::MUTED, TextAlign::Right);
        }

        ui.text(
            24,
            96,
            164,
            percent_text.as_str(),
            3,
            color::TEXT,
            TextAlign::Center,
        );

        draw_window_bar(
            &mut ui,
            216,
            92,
            text::WINDOW_5H,
            provider,
            "5h",
            color::TEAL,
            210,
        );
        draw_window_bar(
            &mut ui,
            216,
            164,
            text::WINDOW_7D,
            provider,
            "7d",
            color::LAVENDER,
            210,
        );

        draw_provider_strip(&mut ui, snapshot, selected_provider);
    })
}

fn draw_window_bar(
    ui: &mut UiCanvas<'_>,
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

    ui.text(x, y - 25, 56, label, 1, color::TEXT, TextAlign::Left);
    ui.text(
        x + bar_width - 56,
        y - 22,
        56,
        percent_text.as_str(),
        1,
        color::TEXT,
        TextAlign::Right,
    );
    ui.meter_shell(x, y, bar_width, 29, color::PANEL);
    ui.meter_fill(x + 4, y + 4, bar_width - 8, 21, percent, fill_color);
}

fn draw_provider_strip(ui: &mut UiCanvas<'_>, snapshot: &UsageSnapshot, selected_provider: usize) {
    let count = snapshot.providers.len().min(3);
    if count == 0 {
        return;
    }

    let card_width = 86;
    let gap = 8;
    let start_x =
        (ui.width() - (count as i32 * card_width + (count as i32 - 1) * gap)) / 2;

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

        ui.rect(x, 222, card_width, 42, card_color);
        ui.text(
            x + 7,
            230,
            card_width - 14,
            provider.id.as_str(),
            1,
            color::TEXT,
            TextAlign::Center,
        );
        ui.meter_fill(x + 7, 252, card_width - 14, 6, percent, color_for(percent));
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
