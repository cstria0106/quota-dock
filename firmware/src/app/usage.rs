use crate::drivers::display::{EspResult, Sh8601, LCD_H_RES};
use crate::network::{UsageProvider, UsageSnapshot, UsageWindow};

const BG: u16 = rgb565(16, 18, 24);
const BG_DOT: u16 = rgb565(22, 25, 33);
const PANEL: u16 = rgb565(32, 35, 45);
const PANEL_DIM: u16 = rgb565(45, 48, 58);
const TEXT: u16 = rgb565(240, 238, 226);
const MUTED: u16 = rgb565(147, 151, 163);
const MINT: u16 = rgb565(64, 215, 164);
const TEAL: u16 = rgb565(54, 178, 202);
const AMBER: u16 = rgb565(248, 190, 76);
const CORAL: u16 = rgb565(241, 93, 86);
const LAVENDER: u16 = rgb565(166, 142, 245);
const INK: u16 = rgb565(35, 31, 32);

pub fn draw_waiting(panel: &Sh8601) -> EspResult {
    panel.draw_rows(|output, y, rows| {
        fill_background(output, y, rows);
        draw_text(output, y, rows, 31, 24, "AGENT QUOTA", 2, TEXT);
        draw_face(
            output,
            y,
            rows,
            LCD_H_RES as i32 / 2,
            152,
            54,
            MINT,
            Mood::Calm,
        );
        draw_text(output, y, rows, 68, 246, "WAITING", 3, TEXT);
        draw_text(output, y, rows, 42, 304, "PUSH USAGE FROM CLI", 1, MUTED);
        draw_meter_shell(output, y, rows, 30, 350, 220, 26, PANEL_DIM);
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
        fill_background(output, y, rows);
        draw_text(output, y, rows, 28, 18, "AGENT QUOTA", 2, TEXT);
        draw_text(output, y, rows, 24, 54, provider.label.as_str(), 2, accent);
        draw_text(output, y, rows, 208, 58, source, 1, MUTED);

        draw_face(output, y, rows, 70, 139, 43, accent, mood);
        draw_text(output, y, rows, 142, 110, percent_text.as_str(), 4, TEXT);
        draw_text(output, y, rows, 148, 158, "USED", 2, MUTED);

        draw_window_bar(output, y, rows, 26, 222, "5H", provider, "5h", TEAL);
        draw_window_bar(output, y, rows, 26, 286, "7D", provider, "7d", LAVENDER);

        draw_provider_strip(output, y, rows, snapshot, selected_provider);
        draw_text(
            output,
            y,
            rows,
            38,
            424,
            snapshot.updated_at.as_str(),
            1,
            MUTED,
        );
    })
}

fn fill_background(output: &mut [u16], y_start: usize, rows: usize) {
    for row in 0..rows {
        let y = y_start + row;
        for x in 0..LCD_H_RES {
            let dot = (x + y) % 18 == 0;
            output[row * LCD_H_RES + x] = if dot { BG_DOT } else { BG };
        }
    }
}

fn draw_window_bar(
    output: &mut [u16],
    y_start: usize,
    rows: usize,
    x: i32,
    y: i32,
    label: &str,
    provider: &UsageProvider,
    kind: &str,
    fallback: u16,
) {
    let window = provider
        .windows
        .iter()
        .find(|window| window.kind.eq_ignore_ascii_case(kind));
    let percent = window.map(|window| window.used_percent).unwrap_or(0);
    let color = if percent == 0 {
        fallback
    } else {
        color_for(percent)
    };
    let percent_text = format!("{percent}%");

    draw_text(output, y_start, rows, x, y - 25, label, 2, TEXT);
    draw_text(
        output,
        y_start,
        rows,
        x + 172,
        y - 22,
        percent_text.as_str(),
        2,
        TEXT,
    );
    draw_meter_shell(output, y_start, rows, x, y, 228, 29, PANEL);
    draw_meter_fill(output, y_start, rows, x + 4, y + 4, 220, 21, percent, color);
}

fn draw_provider_strip(
    output: &mut [u16],
    y_start: usize,
    rows: usize,
    snapshot: &UsageSnapshot,
    selected_provider: usize,
) {
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
            PANEL_DIM
        } else {
            PANEL
        };
        fill_rect(output, y_start, rows, x, 356, card_width, 48, card_color);
        draw_text(
            output,
            y_start,
            rows,
            x + 7,
            365,
            provider.id.as_str(),
            1,
            TEXT,
        );
        draw_meter_fill(
            output,
            y_start,
            rows,
            x + 7,
            388,
            card_width - 14,
            7,
            percent,
            color_for(percent),
        );
    }
}

fn draw_meter_shell(
    output: &mut [u16],
    y_start: usize,
    rows: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u16,
) {
    fill_rect(output, y_start, rows, x, y, w, h, color);
    fill_rect(output, y_start, rows, x + 2, y + 2, w - 4, h - 4, BG);
}

fn draw_meter_fill(
    output: &mut [u16],
    y_start: usize,
    rows: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    percent: u8,
    color: u16,
) {
    let fill_width = (w * percent.min(100) as i32) / 100;
    fill_rect(output, y_start, rows, x, y, w, h, PANEL_DIM);
    if fill_width > 0 {
        fill_rect(output, y_start, rows, x, y, fill_width, h, color);
        let shine = (h / 3).max(1);
        fill_rect(
            output,
            y_start,
            rows,
            x,
            y,
            fill_width,
            shine,
            rgb565(255, 245, 202),
        );
    }
}

fn draw_face(
    output: &mut [u16],
    y_start: usize,
    rows: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    color: u16,
    mood: Mood,
) {
    fill_circle(output, y_start, rows, cx, cy, radius, color);
    fill_circle(output, y_start, rows, cx - 18, cy - 9, 6, INK);
    fill_circle(output, y_start, rows, cx + 18, cy - 9, 6, INK);

    match mood {
        Mood::Calm => {
            fill_rect(output, y_start, rows, cx - 18, cy + 16, 36, 5, INK);
            fill_rect(output, y_start, rows, cx - 12, cy + 21, 24, 5, INK);
        }
        Mood::Busy => {
            fill_rect(output, y_start, rows, cx - 22, cy + 17, 44, 5, INK);
        }
        Mood::Hot => {
            fill_rect(output, y_start, rows, cx - 19, cy + 16, 38, 7, INK);
            fill_circle(
                output,
                y_start,
                rows,
                cx + 34,
                cy - 28,
                7,
                rgb565(128, 225, 255),
            );
        }
    }
}

fn draw_text(
    output: &mut [u16],
    y_start: usize,
    rows: usize,
    x: i32,
    y: i32,
    text: &str,
    scale: i32,
    color: u16,
) {
    let mut cursor = x;
    for ch in text.chars() {
        if ch == ' ' {
            cursor += 4 * scale;
            continue;
        }
        let glyph = glyph(ch.to_ascii_uppercase());
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    fill_rect(
                        output,
                        y_start,
                        rows,
                        cursor + col * scale,
                        y + row as i32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cursor += 6 * scale;
        if cursor > LCD_H_RES as i32 - 2 {
            break;
        }
    }
}

fn fill_rect(
    output: &mut [u16],
    y_start: usize,
    rows: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u16,
) {
    let x0 = x.max(0) as usize;
    let y0 = y.max(y_start as i32) as usize;
    let x1 = (x + w).min(LCD_H_RES as i32).max(0) as usize;
    let y1 = (y + h).min((y_start + rows) as i32).max(0) as usize;
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    for py in y0..y1 {
        let row = py - y_start;
        for px in x0..x1 {
            output[row * LCD_H_RES + px] = color;
        }
    }
}

fn fill_circle(
    output: &mut [u16],
    y_start: usize,
    rows: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    color: u16,
) {
    let r2 = radius * radius;
    let y0 = (cy - radius).max(y_start as i32);
    let y1 = (cy + radius).min((y_start + rows) as i32 - 1);
    for y in y0..=y1 {
        let dy = y - cy;
        for x in (cx - radius).max(0)..=(cx + radius).min(LCD_H_RES as i32 - 1) {
            let dx = x - cx;
            if dx * dx + dy * dy <= r2 {
                output[(y as usize - y_start) * LCD_H_RES + x as usize] = color;
            }
        }
    }
}

fn focus_window(provider: &UsageProvider) -> Option<&UsageWindow> {
    provider
        .windows
        .iter()
        .max_by_key(|window| window.used_percent)
}

fn status_label<'a>(provider: &'a UsageProvider, window: Option<&'a UsageWindow>) -> &'static str {
    if window
        .map(|window| window.status.eq_ignore_ascii_case("error"))
        .unwrap_or(false)
    {
        return "ERR";
    }
    if provider.source.eq_ignore_ascii_case("local-estimate") {
        return "EST";
    }
    "LIVE"
}

fn color_for(percent: u8) -> u16 {
    match percent {
        0..=54 => MINT,
        55..=79 => AMBER,
        80..=100 => CORAL,
        _ => TEAL,
    }
}

fn mood_for(percent: u8) -> Mood {
    match percent {
        0..=54 => Mood::Calm,
        55..=79 => Mood::Busy,
        _ => Mood::Hot,
    }
}

fn glyph(ch: char) -> [u8; 7] {
    match ch {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0F, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0F],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0F, 0x10, 0x10, 0x13, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F],
        'J' => [0x1F, 0x02, 0x02, 0x02, 0x12, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x0A, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        'X' => [0x11, 0x0A, 0x04, 0x04, 0x04, 0x0A, 0x11],
        'Y' => [0x11, 0x0A, 0x04, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x10, 0x1E, 0x01, 0x01, 0x1E],
        '6' => [0x07, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x1C],
        '%' => [0x19, 0x19, 0x02, 0x04, 0x08, 0x13, 0x13],
        ':' => [0x00, 0x04, 0x04, 0x00, 0x04, 0x04, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        _ => [0x1F, 0x11, 0x15, 0x15, 0x15, 0x11, 0x1F],
    }
}

#[derive(Clone, Copy)]
enum Mood {
    Calm,
    Busy,
    Hot,
}

const fn rgb565(red: u8, green: u8, blue: u8) -> u16 {
    let value = (((red as u16) & 0xF8) << 8) | (((green as u16) & 0xFC) << 3) | (blue as u16 >> 3);
    ((value & 0x00FF) << 8) | (value >> 8)
}
