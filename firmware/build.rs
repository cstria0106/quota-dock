#[path = "build_support/bitmap_font_generator.rs"]
mod bitmap_font_generator;

use std::path::PathBuf;

use bitmap_font_generator::{generate_bitmap_font, BitmapFontOptions};

const FONT_SIZE: f32 = 12.0;
const FONT_SOURCE: &str = "assets/fonts/Galmuri11.ttf";
const TEXT_SOURCE: &str = "src/app/text.rs";
const GENERATED_FONT: &str = "generated_font.rs";

fn main() {
    embuild::espidf::sysenv::output();
    println!("cargo:rerun-if-changed={FONT_SOURCE}");
    println!("cargo:rerun-if-changed={TEXT_SOURCE}");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("out dir"));
    let options = BitmapFontOptions {
        font_size: FONT_SIZE,
        font_source: &manifest_dir.join(FONT_SOURCE),
        text_source: &manifest_dir.join(TEXT_SOURCE),
        output: &out_dir.join(GENERATED_FONT),
    };

    if let Err(err) = generate_bitmap_font(&options) {
        panic!("generate bitmap font: {err}");
    }
}
