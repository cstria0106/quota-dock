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

mod font_7 {
    include!(concat!(env!("OUT_DIR"), "/generated_font_7.rs"));
}

mod font_9 {
    include!(concat!(env!("OUT_DIR"), "/generated_font_9.rs"));
}

mod font_11 {
    include!(concat!(env!("OUT_DIR"), "/generated_font_11.rs"));
}

pub mod color {
    use super::{rgb565, Color};

    pub const BG: Color = rgb565(16, 18, 24);
    pub const BG_DOT: Color = rgb565(22, 25, 33);
    pub const PANEL_DIM: Color = rgb565(45, 48, 58);
    pub const TEXT: Color = rgb565(240, 238, 226);
    pub const MUTED: Color = rgb565(147, 151, 163);
    pub const MINT: Color = rgb565(64, 215, 164);
    pub const TEAL: Color = rgb565(54, 178, 202);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontFace {
    Galmuri7,
    Galmuri9,
    Galmuri11,
}

impl FontFace {
    pub const DEFAULT: Self = Self::Galmuri9;
}

struct BitmapGlyphRef {
    width: u8,
    height: u8,
    x_offset: i8,
    y_offset: i8,
    advance: u8,
    bitmap_len: u16,
    bitmap: &'static [u8],
}

fn glyph(font: FontFace, ch: char) -> Option<BitmapGlyphRef> {
    match font {
        FontFace::Galmuri7 => font_7::glyph(ch).map(|glyph| {
            let start = glyph.bitmap_offset as usize;
            let end = start + glyph.bitmap_len as usize;
            BitmapGlyphRef {
                width: glyph.width,
                height: glyph.height,
                x_offset: glyph.x_offset,
                y_offset: glyph.y_offset,
                advance: glyph.advance,
                bitmap_len: glyph.bitmap_len,
                bitmap: &font_7::BITMAP[start..end],
            }
        }),
        FontFace::Galmuri9 => font_9::glyph(ch).map(|glyph| {
            let start = glyph.bitmap_offset as usize;
            let end = start + glyph.bitmap_len as usize;
            BitmapGlyphRef {
                width: glyph.width,
                height: glyph.height,
                x_offset: glyph.x_offset,
                y_offset: glyph.y_offset,
                advance: glyph.advance,
                bitmap_len: glyph.bitmap_len,
                bitmap: &font_9::BITMAP[start..end],
            }
        }),
        FontFace::Galmuri11 => font_11::glyph(ch).map(|glyph| {
            let start = glyph.bitmap_offset as usize;
            let end = start + glyph.bitmap_len as usize;
            BitmapGlyphRef {
                width: glyph.width,
                height: glyph.height,
                x_offset: glyph.x_offset,
                y_offset: glyph.y_offset,
                advance: glyph.advance,
                bitmap_len: glyph.bitmap_len,
                bitmap: &font_11::BITMAP[start..end],
            }
        }),
    }
}

fn line_height(font: FontFace) -> i32 {
    match font {
        FontFace::Galmuri7 => font_7::LINE_HEIGHT as i32,
        FontFace::Galmuri9 => font_9::LINE_HEIGHT as i32,
        FontFace::Galmuri11 => font_11::LINE_HEIGHT as i32,
    }
}

fn font_size(font: FontFace) -> i32 {
    match font {
        FontFace::Galmuri7 => font_7::FONT_SIZE as i32,
        FontFace::Galmuri9 => font_9::FONT_SIZE as i32,
        FontFace::Galmuri11 => font_11::FONT_SIZE as i32,
    }
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

    pub fn text_height_for(font: FontFace, scale: i32, lines: usize) -> i32 {
        line_height(font) * scale.max(1) * lines.max(1) as i32
    }

    pub fn text_ink_bounds_y(text: &str, font: FontFace, scale: i32) -> Option<(i32, i32)> {
        let scale = scale.max(1);
        let mut line_y = 0;
        let mut min_y: Option<i32> = None;
        let mut max_y: Option<i32> = None;

        for line in text.split('\n') {
            for ch in line.chars() {
                let Some(glyph) = glyph(font, ch).or_else(|| glyph(font, '?')) else {
                    continue;
                };
                if glyph.width == 0 || glyph.height == 0 || glyph.bitmap_len == 0 {
                    continue;
                }

                let top = line_y + glyph.y_offset as i32 * scale;
                let bottom = top + glyph.height as i32 * scale;
                min_y = Some(min_y.map_or(top, |value| value.min(top)));
                max_y = Some(max_y.map_or(bottom, |value| value.max(bottom)));
            }
            line_y += line_height(font) * scale;
        }

        min_y.zip(max_y)
    }

    pub fn text_with_font(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        text: &str,
        font: FontFace,
        scale: i32,
        color: Color,
        align: TextAlign,
    ) {
        let scale = scale.max(1);
        let mut line_y = y;
        for line in text.split('\n') {
            let line_width = Self::text_line_width(line, font, scale);
            let line_x = match align {
                TextAlign::Left => x,
                TextAlign::Center => x + (width - line_width) / 2,
            };
            self.text_line(line_x, line_y, line, font, scale, color);
            line_y += line_height(font) * scale;
        }
    }

    fn text_line(&mut self, x: i32, y: i32, text: &str, font: FontFace, scale: i32, color: Color) {
        let mut cursor = x;
        for ch in text.chars() {
            self.glyph(cursor, y, ch, font, scale, color);
            cursor += Self::glyph_advance(ch, font) * scale;
            if cursor > UI_WIDTH as i32 - 2 {
                break;
            }
        }
    }

    fn glyph(&mut self, x: i32, y: i32, ch: char, font: FontFace, scale: i32, color: Color) {
        let Some(glyph) = glyph(font, ch).or_else(|| glyph(font, '?')) else {
            return;
        };
        if glyph.width == 0 || glyph.height == 0 || glyph.bitmap_len == 0 {
            return;
        }

        let origin_x = x + glyph.x_offset as i32 * scale;
        let origin_y = y + glyph.y_offset as i32 * scale;

        for row in 0..glyph.height as usize {
            for col in 0..glyph.width as usize {
                let bit_index = row * glyph.width as usize + col;
                let byte = glyph.bitmap[bit_index / 8];
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

    fn text_line_width(text: &str, font: FontFace, scale: i32) -> i32 {
        text.chars()
            .map(|ch| Self::glyph_advance(ch, font))
            .sum::<i32>()
            * scale
    }

    fn glyph_advance(ch: char, font: FontFace) -> i32 {
        glyph(font, ch)
            .or_else(|| glyph(font, '?'))
            .map(|glyph| glyph.advance as i32)
            .unwrap_or(font_size(font) / 2)
    }

    pub fn rounded_meter_fill(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        percent: u8,
        radius: i32,
        fill_color: Color,
        track_color: Color,
    ) {
        self.rounded_rect(x, y, w, h, radius, track_color);
        let fill_width = if percent == 0 {
            0
        } else {
            ((w * percent.min(100) as i32) / 100).max(4).min(w)
        };
        if fill_width > 0 {
            self.rounded_rect(x, y, fill_width, h, radius.min(fill_width / 2), fill_color);
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

    pub fn rounded_rect(&mut self, x: i32, y: i32, w: i32, h: i32, radius: i32, color: Color) {
        let radius = radius.max(0).min(w.min(h) / 2);
        if radius <= 0 {
            self.rect(x, y, w, h, color);
            return;
        }

        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(UI_WIDTH as i32).max(0);
        let y1 = (y + h).min(UI_HEIGHT as i32).max(0);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let left_center = x + radius;
        let right_center = x + w - radius - 1;
        let top_center = y + radius;
        let bottom_center = y + h - radius - 1;
        let threshold = radius * radius - radius;

        for py in y0..y1 {
            for px in x0..x1 {
                let cx = if px < left_center {
                    left_center
                } else if px > right_center {
                    right_center
                } else {
                    px
                };
                let cy = if py < top_center {
                    top_center
                } else if py > bottom_center {
                    bottom_center
                } else {
                    py
                };
                let dx = px - cx;
                let dy = py - cy;
                if dx * dx + dy * dy <= threshold {
                    self.set_pixel(px, py, color);
                }
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
