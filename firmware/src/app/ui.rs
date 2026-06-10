use crate::drivers::display::{LCD_H_RES as PHYSICAL_H_RES, LCD_V_RES as PHYSICAL_V_RES};

pub type Color = u16;

const UI_LANDSCAPE_CLOCKWISE: bool = true;
pub const UI_WIDTH: usize = if UI_LANDSCAPE_CLOCKWISE {
    PHYSICAL_V_RES
} else {
    PHYSICAL_H_RES
};
pub const UI_HEIGHT: usize = if UI_LANDSCAPE_CLOCKWISE {
    PHYSICAL_H_RES
} else {
    PHYSICAL_V_RES
};

mod font {
    include!(concat!(env!("OUT_DIR"), "/generated_font.rs"));
}

pub mod color {
    use super::{rgb565, Color};

    pub const BG: Color = rgb565(16, 18, 24);
    pub const BG_DOT: Color = rgb565(22, 25, 33);
    pub const PANEL: Color = rgb565(32, 35, 45);
    pub const PANEL_DIM: Color = rgb565(45, 48, 58);
    pub const TEXT: Color = rgb565(240, 238, 226);
    pub const MUTED: Color = rgb565(147, 151, 163);
    pub const MINT: Color = rgb565(64, 215, 164);
    pub const TEAL: Color = rgb565(54, 178, 202);
    pub const AMBER: Color = rgb565(248, 190, 76);
    pub const CORAL: Color = rgb565(241, 93, 86);
    pub const LAVENDER: Color = rgb565(166, 142, 245);
    pub const SHINE: Color = rgb565(255, 245, 202);
}

#[derive(Clone, Copy)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

pub struct UiCanvas<'a> {
    output: &'a mut [u16],
    y_start: usize,
    rows: usize,
}

impl<'a> UiCanvas<'a> {
    pub fn new(output: &'a mut [u16], y_start: usize, rows: usize) -> Self {
        Self {
            output,
            y_start,
            rows,
        }
    }

    pub fn dotted_background(&mut self) {
        for row in 0..self.rows {
            let physical_y = self.y_start + row;
            for physical_x in 0..PHYSICAL_H_RES {
                let (x, y) = physical_to_logical(physical_x, physical_y);
                let dot = (x + y) % 18 == 0;
                self.output[row * PHYSICAL_H_RES + physical_x] =
                    if dot { color::BG_DOT } else { color::BG };
            }
        }
    }

    pub fn width(&self) -> i32 {
        UI_WIDTH as i32
    }

    pub fn height(&self) -> i32 {
        UI_HEIGHT as i32
    }

    pub fn text(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        text: &str,
        scale: i32,
        color: Color,
        align: TextAlign,
    ) {
        let scale = scale.max(1);
        let mut line_y = y;
        for line in text.split('\n') {
            let line_width = Self::text_line_width(line, scale);
            let line_x = match align {
                TextAlign::Left => x,
                TextAlign::Center => x + (width - line_width) / 2,
                TextAlign::Right => x + width - line_width,
            };
            self.text_line(line_x, line_y, line, scale, color);
            line_y += font::LINE_HEIGHT as i32 * scale;
        }
    }

    fn text_line(&mut self, x: i32, y: i32, text: &str, scale: i32, color: Color) {
        let mut cursor = x;
        for ch in text.chars() {
            self.glyph(cursor, y, ch, scale, color);
            cursor += Self::glyph_advance(ch) * scale;
            if cursor > UI_WIDTH as i32 - 2 {
                break;
            }
        }
    }

    fn glyph(&mut self, x: i32, y: i32, ch: char, scale: i32, color: Color) {
        let Some(glyph) = font::glyph(ch).or_else(|| font::glyph('?')) else {
            return;
        };
        if glyph.width == 0 || glyph.height == 0 || glyph.bitmap_len == 0 {
            return;
        }

        let start = glyph.bitmap_offset as usize;
        let end = start + glyph.bitmap_len as usize;
        let bitmap = &font::BITMAP[start..end];
        let origin_x = x + glyph.x_offset as i32 * scale;
        let origin_y = y + glyph.y_offset as i32 * scale;

        for row in 0..glyph.height as usize {
            for col in 0..glyph.width as usize {
                let bit_index = row * glyph.width as usize + col;
                let byte = bitmap[bit_index / 8];
                let mask = 1 << (7 - bit_index % 8);
                if byte & mask != 0 {
                    self.rect(
                        origin_x + col as i32 * scale,
                        origin_y + row as i32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
    }

    fn text_line_width(text: &str, scale: i32) -> i32 {
        text.chars().map(Self::glyph_advance).sum::<i32>() * scale
    }

    fn glyph_advance(ch: char) -> i32 {
        font::glyph(ch)
            .or_else(|| font::glyph('?'))
            .map(|glyph| glyph.advance as i32)
            .unwrap_or(font::FONT_SIZE as i32 / 2)
    }

    pub fn meter_shell(&mut self, x: i32, y: i32, w: i32, h: i32, border_color: Color) {
        self.rect(x, y, w, h, border_color);
        self.rect(x + 2, y + 2, w - 4, h - 4, color::BG);
    }

    pub fn meter_fill(&mut self, x: i32, y: i32, w: i32, h: i32, percent: u8, fill_color: Color) {
        let fill_width = (w * percent.min(100) as i32) / 100;
        self.rect(x, y, w, h, color::PANEL_DIM);
        if fill_width > 0 {
            self.rect(x, y, fill_width, h, fill_color);
            self.rect(x, y, fill_width, (h / 3).max(1), color::SHINE);
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(UI_WIDTH as i32).max(0);
        let y1 = (y + h).min(UI_HEIGHT as i32).max(0);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        for py in y0..y1 {
            for px in x0..x1 {
                self.set_pixel(px, py, color);
            }
        }
    }

    pub fn circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color) {
        let r2 = radius * radius;
        let y0 = (cy - radius).max(0);
        let y1 = (cy + radius).min(UI_HEIGHT as i32 - 1);
        for y in y0..=y1 {
            let dy = y - cy;
            for x in (cx - radius).max(0)..=(cx + radius).min(UI_WIDTH as i32 - 1) {
                let dx = x - cx;
                if dx * dx + dy * dy <= r2 {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: Color) {
        let Some((physical_x, physical_y)) = logical_to_physical(x, y) else {
            return;
        };
        if physical_y < self.y_start || physical_y >= self.y_start + self.rows {
            return;
        }

        let row = physical_y - self.y_start;
        self.output[row * PHYSICAL_H_RES + physical_x] = color;
    }
}

fn logical_to_physical(x: i32, y: i32) -> Option<(usize, usize)> {
    if x < 0 || y < 0 || x >= UI_WIDTH as i32 || y >= UI_HEIGHT as i32 {
        return None;
    }

    let x = x as usize;
    let y = y as usize;
    if UI_LANDSCAPE_CLOCKWISE {
        Some((y, PHYSICAL_V_RES - 1 - x))
    } else {
        Some((x, y))
    }
}

fn physical_to_logical(physical_x: usize, physical_y: usize) -> (usize, usize) {
    if UI_LANDSCAPE_CLOCKWISE {
        (PHYSICAL_V_RES - 1 - physical_y, physical_x)
    } else {
        (physical_x, physical_y)
    }
}

const fn rgb565(red: u8, green: u8, blue: u8) -> Color {
    let value = (((red as u16) & 0xF8) << 8) | (((green as u16) & 0xFC) << 3) | (blue as u16 >> 3);
    ((value & 0x00FF) << 8) | (value >> 8)
}
