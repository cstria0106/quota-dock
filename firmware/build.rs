#[path = "build_support/bitmap_font_generator.rs"]
mod bitmap_font_generator;

use std::path::PathBuf;

use bitmap_font_generator::{generate_bitmap_font, BitmapFontOptions};

const FONTS: &[(&str, &str, f32, &str)] = &[
    (
        "Galmuri7",
        "assets/fonts/Galmuri7.ttf",
        8.0,
        "generated_font_7.rs",
    ),
    (
        "Galmuri9",
        "assets/fonts/Galmuri9.ttf",
        10.0,
        "generated_font_9.rs",
    ),
];
const TEXT_SOURCE: &str = "src/app/text.rs";

fn main() {
    embuild::espidf::sysenv::output();
    for &(_, font_source, _, _) in FONTS {
        println!("cargo:rerun-if-changed={font_source}");
    }
    println!("cargo:rerun-if-changed={TEXT_SOURCE}");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("out dir"));

    for &(font_name, font_source, font_size, output) in FONTS {
        let options = BitmapFontOptions {
            font_name,
            font_size,
            font_source: &manifest_dir.join(font_source),
            text_source: &manifest_dir.join(TEXT_SOURCE),
            output: &out_dir.join(output),
        };

        if let Err(err) = generate_bitmap_font(&options) {
            panic!("generate bitmap font {font_source}: {err}");
        }
    }
}
