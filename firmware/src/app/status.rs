use crate::app::text;
use crate::app::ui::{color, TextAlign, UiCanvas};
use crate::drivers::display::{EspResult, Sh8601};
use crate::network::NetworkStatus;

pub fn draw_waiting(panel: &Sh8601) -> EspResult {
    panel.draw_rows(|output, y, rows| {
        let mut ui = UiCanvas::new(output, y, rows);
        ui.dotted_background();
        draw_usage_wait(&mut ui, None);
    })
}

pub fn draw_network_status(
    panel: &Sh8601,
    status: &NetworkStatus,
    loading_frame: u8,
) -> EspResult {
    panel.draw_rows(|output, y, rows| {
        let mut ui = UiCanvas::new(output, y, rows);
        ui.dotted_background();

        if !status.has_credentials {
            draw_wifi_setup(&mut ui);
        } else if status.connected {
            draw_usage_wait(&mut ui, status.ip.as_deref());
        } else {
            draw_wifi_connecting(&mut ui, loading_frame);
        }
    })
}

pub fn network_status_is_animating(status: &NetworkStatus) -> bool {
    status.has_credentials && !status.connected
}

fn draw_wifi_setup(ui: &mut UiCanvas<'_>) {
    ui.text(
        0,
        ui.height() / 2 - 16,
        ui.width(),
        text::SETUP_WIFI,
        2,
        color::TEXT,
        TextAlign::Center,
    );
}

fn draw_wifi_connecting(ui: &mut UiCanvas<'_>, loading_frame: u8) {
    ui.text(
        0,
        88,
        ui.width(),
        text::CONNECTING_WIFI,
        2,
        color::TEXT,
        TextAlign::Center,
    );
    draw_loading_dots(ui, loading_frame);
}

fn draw_usage_wait(ui: &mut UiCanvas<'_>, ip: Option<&str>) {
    ui.text(
        0,
        78,
        ui.width(),
        text::WAITING_FOR_USAGE,
        2,
        color::TEXT,
        TextAlign::Center,
    );
    ui.text(
        0,
        148,
        ui.width(),
        ip.unwrap_or(text::NO_IP),
        2,
        color::MINT,
        TextAlign::Center,
    );
}

fn draw_loading_dots(ui: &mut UiCanvas<'_>, loading_frame: u8) {
    let base_x = ui.width() / 2 - 34;
    let base_y = 168;
    let wave_offsets = [0, -5, -9, -12, -9, -5, 0, 5, 9, 12, 9, 5];

    for index in 0..3 {
        let phase = (loading_frame as usize + index * 3) % wave_offsets.len();
        let x = base_x + index as i32 * 34;
        let y = base_y + wave_offsets[phase];
        ui.circle(x, y, 7, color::TEAL);
    }
}
