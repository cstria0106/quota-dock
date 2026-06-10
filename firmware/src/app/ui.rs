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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn full() -> Self {
        Self {
            x: 0,
            y: 0,
            w: UI_WIDTH as i32,
            h: UI_HEIGHT as i32,
        }
    }

    pub fn union(self, other: Self) -> Self {
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = (self.x + self.w).max(other.x + other.w);
        let y1 = (self.y + self.h).max(other.y + other.h);
        Self::new(x0, y0, x1 - x0, y1 - y0).clamp_to_screen()
    }

    pub fn expand(self, amount: i32) -> Self {
        Self::new(
            self.x - amount,
            self.y - amount,
            self.w + amount * 2,
            self.h + amount * 2,
        )
        .clamp_to_screen()
    }

    pub fn clamp_to_screen(self) -> Self {
        let x0 = self.x.max(0).min(UI_WIDTH as i32);
        let y0 = self.y.max(0).min(UI_HEIGHT as i32);
        let x1 = (self.x + self.w).max(0).min(UI_WIDTH as i32);
        let y1 = (self.y + self.h).max(0).min(UI_HEIGHT as i32);
        Self::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }

    pub fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn intersects(self, other: Self) -> bool {
        let self_x1 = self.x + self.w;
        let self_y1 = self.y + self.h;
        let other_x1 = other.x + other.w;
        let other_y1 = other.y + other.h;
        self.x < other_x1 && self_x1 > other.x && self.y < other_y1 && self_y1 > other.y
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalArea {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

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
    pub const SHINE: Color = rgb565(255, 245, 202);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

pub struct UiCanvas<'a> {
    output: &'a mut [u16],
    physical_x_start: usize,
    physical_width: usize,
    y_start: usize,
    rows: usize,
}

impl<'a> UiCanvas<'a> {
    pub fn new_area(
        output: &'a mut [u16],
        physical_x_start: usize,
        y_start: usize,
        physical_width: usize,
        rows: usize,
    ) -> Self {
        Self {
            output,
            physical_x_start,
            physical_width,
            y_start,
            rows,
        }
    }

    pub fn dotted_background(&mut self) {
        for row in 0..self.rows {
            let physical_y = self.y_start + row;
            for x_offset in 0..self.physical_width {
                let physical_x = self.physical_x_start + x_offset;
                let (x, y) = physical_to_logical(physical_x, physical_y);
                let dot = (x + y) % 18 == 0;
                self.output[row * self.physical_width + x_offset] =
                    if dot { color::BG_DOT } else { color::BG };
            }
        }
    }

    pub fn text_height(scale: i32, lines: usize) -> i32 {
        font::LINE_HEIGHT as i32 * scale.max(1) * lines.max(1) as i32
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
        let radius = radius.max(1);
        let threshold = radius * radius - radius;

        for y in (cy - radius)..=(cy + radius) {
            let dy = y - cy;
            for x in (cx - radius)..=(cx + radius) {
                let dx = x - cx;
                if dx * dx + dy * dy <= threshold {
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
        if physical_x < self.physical_x_start
            || physical_x >= self.physical_x_start + self.physical_width
        {
            return;
        }

        let row = physical_y - self.y_start;
        let col = physical_x - self.physical_x_start;
        self.output[row * self.physical_width + col] = color;
    }
}

pub fn logical_rect_to_physical_area(rect: Rect) -> Option<PhysicalArea> {
    let rect = rect.clamp_to_screen();
    if rect.is_empty() {
        return None;
    }

    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.w - 1;
    let y1 = rect.y + rect.h - 1;
    let points = [
        logical_to_physical(x0, y0)?,
        logical_to_physical(x1, y0)?,
        logical_to_physical(x0, y1)?,
        logical_to_physical(x1, y1)?,
    ];

    let min_x = points.iter().map(|(x, _)| *x).min()?;
    let max_x = points.iter().map(|(x, _)| *x).max()?;
    let min_y = points.iter().map(|(_, y)| *y).min()?;
    let max_y = points.iter().map(|(_, y)| *y).max()?;
    Some(PhysicalArea {
        x: min_x,
        y: min_y,
        w: max_x - min_x + 1,
        h: max_y - min_y + 1,
    })
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

pub const fn rgb565(red: u8, green: u8, blue: u8) -> Color {
    let value = (((red as u16) & 0xF8) << 8) | (((green as u16) & 0xFC) << 3) | (blue as u16 >> 3);
    ((value & 0x00FF) << 8) | (value >> 8)
}
