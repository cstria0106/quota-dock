use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;

use fontdue::{Font, FontSettings};

pub struct BitmapFontOptions<'a> {
    pub font_name: &'a str,
    pub font_size: f32,
    pub font_source: &'a Path,
    pub text_source: &'a Path,
    pub output: &'a Path,
}

pub fn generate_bitmap_font(
    options: &BitmapFontOptions<'_>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let source = fs::read_to_string(options.text_source)?;
    let chars = collect_font_chars(&source, options.font_name);
    let font_bytes = fs::read(options.font_source)?;
    let font = Font::from_bytes(font_bytes, FontSettings::default())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    fs::write(
        options.output,
        encode_bitmap_font(&font, &chars, options.font_size),
    )?;
    Ok(())
}

fn collect_font_chars(source: &str, font_name: &str) -> BTreeSet<char> {
    let mut chars = BTreeSet::new();
    collect_macro_string_chars(source, "ui_font_chars!", &mut chars);
    collect_font_macro_string_chars(source, "ui_font_chars_for!", font_name, &mut chars);
    collect_font_macro_string_chars(source, "ui_text!", font_name, &mut chars);
    chars
}

fn collect_macro_string_chars(source: &str, macro_name: &str, chars: &mut BTreeSet<char>) {
    let mut remaining = source;
    while let Some(index) = remaining.find(macro_name) {
        remaining = &remaining[index + macro_name.len()..];
        let Some(open_index) = remaining.find('(') else {
            break;
        };
        remaining = &remaining[open_index + 1..];
        let Some((literal, next_index)) = parse_next_string_literal(remaining) else {
            continue;
        };
        chars.extend(literal.chars());
        remaining = &remaining[next_index..];
    }
}

fn collect_font_macro_string_chars(
    source: &str,
    macro_name: &str,
    font_name: &str,
    chars: &mut BTreeSet<char>,
) {
    let mut remaining = source;
    while let Some(index) = remaining.find(macro_name) {
        remaining = &remaining[index + macro_name.len()..];
        let Some(open_index) = remaining.find('(') else {
            break;
        };
        remaining = &remaining[open_index + 1..];
        let trimmed = remaining.trim_start();
        let Some((macro_font_name, after_font_index)) = parse_ident(trimmed) else {
            continue;
        };
        let after_font = trimmed[after_font_index..].trim_start();
        let Some(after_comma) = after_font.strip_prefix(',') else {
            continue;
        };
        let Some((literal, next_index)) = parse_next_string_literal(after_comma) else {
            continue;
        };
        if macro_font_name == font_name {
            chars.extend(literal.chars());
        }
        remaining = &after_comma[next_index..];
    }
}

fn parse_ident(source: &str) -> Option<(&str, usize)> {
    let mut end = 0;
    for (index, ch) in source.char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    (end > 0).then_some((&source[..end], end))
}

fn parse_next_string_literal(source: &str) -> Option<(String, usize)> {
    let start = source.find('"')?;
    let mut literal = String::new();
    let mut escaped = false;
    for (index, ch) in source[start + 1..].char_indices() {
        if escaped {
            literal.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                other => other,
            });
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some((literal, start + 1 + index + ch.len_utf8())),
            other => literal.push(other),
        }
    }
    None
}

fn encode_bitmap_font(font: &Font, chars: &BTreeSet<char>, font_size: f32) -> String {
    let line_metrics = font
        .horizontal_line_metrics(font_size)
        .expect("bitmap font should provide horizontal line metrics");
    let ascent = line_metrics.ascent.ceil() as i32;
    let line_height = (line_metrics.ascent - line_metrics.descent + line_metrics.line_gap)
        .ceil()
        .max(font_size) as i32;

    let mut bitmap = Vec::new();
    let mut glyph_entries = Vec::new();
    for ch in chars {
        let (metrics, pixels) = font.rasterize(*ch, font_size);
        let bitmap_offset = bitmap.len();
        let packed = pack_bitmap(&pixels);
        let bitmap_len = packed.len();
        bitmap.extend(packed);

        let y_offset = ascent - metrics.ymin - metrics.height as i32;
        let advance = metrics.advance_width.ceil().max(1.0) as u8;
        glyph_entries.push(GeneratedGlyph {
            ch: *ch,
            width: metrics.width as u8,
            height: metrics.height as u8,
            x_offset: metrics.xmin as i8,
            y_offset: y_offset as i8,
            advance,
            bitmap_offset: bitmap_offset as u32,
            bitmap_len: bitmap_len as u16,
        });
    }

    let mut generated = String::new();
    generated.push_str("#[derive(Clone, Copy)]\n");
    generated.push_str("pub struct BitmapGlyph {\n");
    generated.push_str("    pub ch: char,\n");
    generated.push_str("    pub width: u8,\n");
    generated.push_str("    pub height: u8,\n");
    generated.push_str("    pub x_offset: i8,\n");
    generated.push_str("    pub y_offset: i8,\n");
    generated.push_str("    pub advance: u8,\n");
    generated.push_str("    pub bitmap_offset: u32,\n");
    generated.push_str("    pub bitmap_len: u16,\n");
    generated.push_str("}\n\n");
    generated.push_str(&format!("pub const FONT_SIZE: u8 = {};\n", font_size as u8));
    generated.push_str(&format!("pub const LINE_HEIGHT: u8 = {line_height};\n\n"));
    generated.push_str("pub static GLYPHS: &[BitmapGlyph] = &[\n");
    for glyph in &glyph_entries {
        generated.push_str(&format!(
            "    BitmapGlyph {{ ch: {:?}, width: {}, height: {}, x_offset: {}, y_offset: {}, advance: {}, bitmap_offset: {}, bitmap_len: {} }},\n",
            glyph.ch,
            glyph.width,
            glyph.height,
            glyph.x_offset,
            glyph.y_offset,
            glyph.advance,
            glyph.bitmap_offset,
            glyph.bitmap_len,
        ));
    }
    generated.push_str("];\n\n");
    generated.push_str("pub static BITMAP: &[u8] = &[\n");
    for chunk in bitmap.chunks(16) {
        generated.push_str("    ");
        for byte in chunk {
            generated.push_str(&format!("0x{byte:02X}, "));
        }
        generated.push('\n');
    }
    generated.push_str("];\n\n");
    generated.push_str(
        "pub fn glyph(ch: char) -> Option<&'static BitmapGlyph> {\n    GLYPHS.binary_search_by_key(&ch, |glyph| glyph.ch).ok().map(|index| &GLYPHS[index])\n}\n",
    );
    generated
}

fn pack_bitmap(pixels: &[u8]) -> Vec<u8> {
    let mut packed = vec![0_u8; pixels.len().div_ceil(8)];
    for (index, pixel) in pixels.iter().enumerate() {
        if *pixel >= 128 {
            packed[index / 8] |= 1 << (7 - index % 8);
        }
    }
    packed
}

struct GeneratedGlyph {
    ch: char,
    width: u8,
    height: u8,
    x_offset: i8,
    y_offset: i8,
    advance: u8,
    bitmap_offset: u32,
    bitmap_len: u16,
}
