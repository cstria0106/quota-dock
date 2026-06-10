use std::error::Error;
use std::path::Path;

use image::imageops::FilterType;
use image::{ImageBuffer, ImageReader, Rgb, RgbImage};

pub struct ImageArrayOptions<'a> {
    pub output_name: &'a str,
    pub width: u32,
    pub height: u32,
    pub background: Rgb<u8>,
}

pub fn encode_image_file_as_rgb565_array(
    source_image: &Path,
    options: &ImageArrayOptions<'_>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let image = ImageReader::open(source_image)?.decode()?.to_rgb8();
    Ok(encode_rgb_image_as_rgb565_array(&image, options))
}

pub fn encode_rgb_image_as_rgb565_array(
    image: &RgbImage,
    options: &ImageArrayOptions<'_>,
) -> String {
    let (scaled_width, scaled_height) =
        scaled_size(image.width(), image.height(), options.width, options.height);
    let resized = image::imageops::resize(image, scaled_width, scaled_height, FilterType::Lanczos3);

    let mut canvas = ImageBuffer::from_pixel(options.width, options.height, options.background);
    let x = (options.width - resized.width()) / 2;
    let y = (options.height - resized.height()) / 2;
    image::imageops::replace(&mut canvas, &resized, x.into(), y.into());

    let pixels = canvas
        .pixels()
        .map(|pixel| rgb565_le(pixel[0], pixel[1], pixel[2]))
        .collect::<Vec<_>>();

    let mut generated = format!("pub static {}: &[u16] = &[\n", options.output_name);
    for chunk in pixels.chunks(12) {
        generated.push_str("    ");
        for pixel in chunk {
            generated.push_str(&format!("0x{pixel:04X}, "));
        }
        generated.push('\n');
    }
    generated.push_str("];\n");
    generated
}

pub fn scaled_size(
    source_width: u32,
    source_height: u32,
    max_width: u32,
    max_height: u32,
) -> (u32, u32) {
    let width_ratio = max_width as f32 / source_width as f32;
    let height_ratio = max_height as f32 / source_height as f32;
    let ratio = width_ratio.min(height_ratio).min(1.0);

    (
        (source_width as f32 * ratio).round() as u32,
        (source_height as f32 * ratio).round() as u32,
    )
}

pub fn rgb565_le(red: u8, green: u8, blue: u8) -> u16 {
    let value = (((red as u16) & 0xF8) << 8) | (((green as u16) & 0xFC) << 3) | (blue as u16 >> 3);
    ((value & 0x00FF) << 8) | (value >> 8)
}
