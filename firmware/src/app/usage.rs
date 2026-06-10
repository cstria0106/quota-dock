use crate::app::text;
use crate::app::ui::{color, Color, Mood, TextAlign, UiCanvas};
use crate::drivers::display::{EspResult, Sh8601, LCD_H_RES};
use crate::network::{NetworkStatus, UsageProvider, UsageSnapshot, UsageWindow};

pub fn draw_waiting(panel: &Sh8601) -> EspResult {
    panel.draw_rows(|output, y, rows| {
        let mut ui = UiCanvas::new(output, y, rows);
        ui.dotted_background();
        ui.text(
            0,
            24,
            LCD_H_RES as i32,
            text::APP_TITLE,
            1,
            color::TEXT,
            TextAlign::Center,
        );
        ui.face(LCD_H_RES as i32 / 2, 152, 54, color::MINT, Mood::Calm);
        ui.text(
            0,
            246,
            LCD_H_RES as i32,
            text::WAITING,
            2,
            color::TEXT,
            TextAlign::Center,
        );
        ui.text(
            0,
            304,
            LCD_H_RES as i32,
            text::PUSH_USAGE_FROM_CLI,
            1,
            color::MUTED,
            TextAlign::Center,
        );
        ui.meter_shell(30, 350, 220, 26, color::PANEL_DIM);
    })
}

pub fn draw_network_status(panel: &Sh8601, status: &NetworkStatus) -> EspResult {
    let (title, detail, accent, mood) = if !status.has_credentials {
        (
            text::SETUP_WIFI,
            text::RUN_CLI_PROVISION,
            color::AMBER,
            Mood::Busy,
        )
    } else if status.connected {
        (
            text::WIFI_READY,
            status.ip.as_deref().unwrap_or(text::NO_IP),
            color::MINT,
            Mood::Calm,
        )
    } else {
        (text::WIFI_WAIT, text::CONNECTING, color::TEAL, Mood::Busy)
    };

    panel.draw_rows(|output, y, rows| {
        let mut ui = UiCanvas::new(output, y, rows);
        ui.dotted_background();
        ui.text(
            0,
            24,
            LCD_H_RES as i32,
            text::APP_TITLE,
            1,
            color::TEXT,
            TextAlign::Center,
        );
        ui.face(LCD_H_RES as i32 / 2, 142, 50, accent, mood);
        ui.text(
            0,
            234,
            LCD_H_RES as i32,
            title,
            2,
            color::TEXT,
            TextAlign::Center,
        );
        ui.text(
            0,
            298,
            LCD_H_RES as i32,
            detail,
            1,
            color::MUTED,
            TextAlign::Center,
        );
        ui.text(
            0,
            340,
            LCD_H_RES as i32,
            text::WAITING_FOR_QUOTA,
            1,
            color::MUTED,
            TextAlign::Center,
        );
        ui.meter_shell(30, 386, 220, 26, color::PANEL_DIM);
    })
}

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
    let mood = mood_for(primary_percent);
    let accent = color_for(primary_percent);
    let percent_text = format!("{primary_percent}%");
    let source = status_label(provider, primary);

    panel.draw_rows(|output, y, rows| {
        let mut ui = UiCanvas::new(output, y, rows);
        ui.dotted_background();
        ui.text(
            28,
            18,
            220,
            text::APP_TITLE,
            1,
            color::TEXT,
            TextAlign::Left,
        );
        ui.text(
            24,
            54,
            180,
            provider.label.as_str(),
            1,
            accent,
            TextAlign::Left,
        );
        ui.text(208, 58, 46, source, 1, color::MUTED, TextAlign::Left);

        ui.face(70, 139, 43, accent, mood);
        ui.text(
            122,
            110,
            130,
            percent_text.as_str(),
            3,
            color::TEXT,
            TextAlign::Center,
        );
        ui.text(
            122,
            158,
            130,
            text::USED,
            1,
            color::MUTED,
            TextAlign::Center,
        );

        draw_window_bar(
            &mut ui,
            26,
            222,
            text::WINDOW_5H,
            provider,
            "5h",
            color::TEAL,
        );
        draw_window_bar(
            &mut ui,
            26,
            286,
            text::WINDOW_7D,
            provider,
            "7d",
            color::LAVENDER,
        );

        draw_provider_strip(&mut ui, snapshot, selected_provider);
        ui.text(
            38,
            424,
            220,
            snapshot.updated_at.as_str(),
            1,
            color::MUTED,
            TextAlign::Left,
        );
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
        x + 172,
        y - 22,
        56,
        percent_text.as_str(),
        1,
        color::TEXT,
        TextAlign::Right,
    );
    ui.meter_shell(x, y, 228, 29, color::PANEL);
    ui.meter_fill(x + 4, y + 4, 220, 21, percent, fill_color);
}

fn draw_provider_strip(ui: &mut UiCanvas<'_>, snapshot: &UsageSnapshot, selected_provider: usize) {
    let count = snapshot.providers.len().min(3);
    if count == 0 {
        return;
    }

    let card_width = 74;
    let gap = 8;
    let start_x = (LCD_H_RES as i32 - (count as i32 * card_width + (count as i32 - 1) * gap)) / 2;

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

        ui.rect(x, 356, card_width, 48, card_color);
        ui.text(
            x + 7,
            365,
            card_width - 14,
            provider.id.as_str(),
            1,
            color::TEXT,
            TextAlign::Center,
        );
        ui.meter_fill(x + 7, 388, card_width - 14, 7, percent, color_for(percent));
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
    text::STATUS_LIVE
}

fn color_for(percent: u8) -> Color {
    match percent {
        0..=54 => color::MINT,
        55..=79 => color::AMBER,
        80..=100 => color::CORAL,
        _ => color::TEAL,
    }
}

fn mood_for(percent: u8) -> Mood {
    match percent {
        0..=54 => Mood::Calm,
        55..=79 => Mood::Busy,
        _ => Mood::Hot,
    }
}
