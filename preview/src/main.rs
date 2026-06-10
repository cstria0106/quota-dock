use std::fs;
use std::io;
use std::path::PathBuf;

pub mod drivers {
    pub mod display {
        use std::cell::RefCell;

        pub const LCD_H_RES: usize = 280;
        pub const LCD_V_RES: usize = 456;
        pub type EspResult<T = ()> = Result<T, i32>;

        pub struct Sh8601 {
            pixels: RefCell<Vec<u16>>,
        }

        impl Sh8601 {
            pub fn new() -> Self {
                Self {
                    pixels: RefCell::new(vec![0; LCD_H_RES * LCD_V_RES]),
                }
            }

            pub fn pixels(&self) -> Vec<u16> {
                self.pixels.borrow().clone()
            }

            pub fn draw_area<F>(
                &self,
                x: usize,
                y: usize,
                width: usize,
                rows: usize,
                mut draw: F,
            ) -> EspResult
            where
                F: FnMut(&mut [u16], usize, usize, usize, usize),
            {
                let mut output = vec![0; width * rows];
                draw(&mut output, x, y, width, rows);

                let mut pixels = self.pixels.borrow_mut();
                for row in 0..rows {
                    let dst = (y + row) * LCD_H_RES + x;
                    let src = row * width;
                    pixels[dst..dst + width].copy_from_slice(&output[src..src + width]);
                }
                Ok(())
            }
        }
    }
}

pub mod network {
    #[derive(Clone, Debug)]
    pub struct NetworkStatus {
        pub has_credentials: bool,
        pub connected: bool,
        pub ip: Option<String>,
    }

    #[derive(Clone, Debug)]
    pub struct UsageSnapshot {
        pub providers: Vec<UsageProvider>,
        pub updated_at: String,
        pub updated_at_unix: u64,
    }

    #[derive(Clone, Debug)]
    pub struct UsageProvider {
        pub id: String,
        pub label: String,
        pub theme_color: Option<String>,
        pub theme: Option<UsageTheme>,
        pub pixel_art: Option<UsagePixelArt>,
        pub source: String,
        pub account: Option<String>,
        pub plan: Option<String>,
        pub windows: Vec<UsageWindow>,
    }

    #[derive(Clone, Debug)]
    pub struct UsageTheme {
        pub accent: String,
        pub panel: String,
        pub panel_soft: String,
        pub primary_panel: String,
        pub primary_panel_soft: String,
        pub track: String,
        pub pill: String,
    }

    #[derive(Clone, Debug)]
    pub struct UsagePixelArt {
        pub palette: Vec<String>,
        pub rows: Vec<String>,
    }

    #[derive(Clone, Debug)]
    pub struct UsageWindow {
        pub kind: String,
        pub label: String,
        pub used_percent: u8,
        pub resets_at: Option<String>,
        pub resets_at_unix: Option<u64>,
        pub status: String,
    }
}

#[path = "../../firmware/src/app/renderer.rs"]
pub mod renderer_impl;
#[path = "../../firmware/src/app/status.rs"]
pub mod status_impl;
#[path = "../../firmware/src/app/text.rs"]
pub mod text_impl;
#[path = "../../firmware/src/app/ui.rs"]
pub mod ui_impl;
#[path = "../../firmware/src/app/usage.rs"]
pub mod usage_impl;

pub mod app {
    pub use crate::renderer_impl as renderer;
    pub use crate::status_impl as status;
    pub use crate::text_impl as text;
    pub use crate::ui_impl as ui;
    pub use crate::usage_impl as usage;
}

use app::renderer::Renderer;
use app::ui::{UI_HEIGHT, UI_WIDTH};
use app::usage::{cache_provider_images, usage_scene, ProviderImageCache};
use drivers::display::{Sh8601, LCD_H_RES, LCD_V_RES};
use network::{UsagePixelArt, UsageProvider, UsageSnapshot, UsageTheme, UsageWindow};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let variant = std::env::args().nth(1);
    let mut snapshot = sample_snapshot();
    let mut image_cache = ProviderImageCache::default();
    cache_provider_images(&mut snapshot, &mut image_cache);
    fs::create_dir_all("target/previews")?;

    if let Some(variant) = variant {
        render_provider(&snapshot, &image_cache, &variant)?;
    } else {
        for provider in &snapshot.providers {
            render_provider(&snapshot, &image_cache, provider.id.as_str())?;
        }
    }
    Ok(())
}

fn render_provider(
    snapshot: &UsageSnapshot,
    image_cache: &ProviderImageCache,
    provider_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let selected_provider = snapshot
        .providers
        .iter()
        .position(|provider| provider.id.eq_ignore_ascii_case(provider_id))
        .ok_or_else(|| format!("unknown provider: {provider_id}"))?;

    let scene = usage_scene(snapshot, image_cache, selected_provider, 0);
    let panel = Sh8601::new();
    let mut renderer = Renderer::new();
    renderer.set_scene(scene);
    renderer
        .tick(&panel)
        .map_err(|err| format!("render: {err}"))?;

    let output = PathBuf::from("target/previews").join(format!(
        "usage-{}.png",
        snapshot.providers[selected_provider].id
    ));
    write_logical_png(&output, &panel.pixels())?;
    println!("{}", output.display());
    Ok(())
}

fn sample_snapshot() -> UsageSnapshot {
    UsageSnapshot {
        updated_at: "UPDATED 1781090000".to_string(),
        updated_at_unix: 1_781_090_000,
        providers: vec![
            provider(
                "codex",
                "CODEX",
                theme(
                    "#3B82F6", "#101823", "#162338", "#111C2D", "#1A3154", "#263141", "#263246",
                ),
                Some(filled_art("#3B82F6", 96)),
                vec![
                    window("5h", "5h", 29, 1_781_098_520),
                    window("7d", "Week", 4, 1_781_695_240),
                ],
            ),
            provider(
                "claude",
                "CLAUDE",
                theme(
                    "#D97757", "#1D1714", "#2A1E18", "#231912", "#3A251A", "#3A2B25", "#3B2B25",
                ),
                Some(filled_art("#D97757", 96)),
                vec![
                    window("5h", "5h", 61, 1_781_093_880),
                    window("7d", "Week", 18, 1_781_349_440),
                    window("7d-opus", "Opus", 6, 1_781_349_440),
                ],
            ),
            provider(
                "opencode",
                "OPENCODE",
                theme(
                    "#18A77A", "#101B17", "#172820", "#112119", "#1A3A2C", "#25352F", "#243A31",
                ),
                Some(two_tone_art("#18A77A", "#9AF0C8", 96)),
                vec![
                    window("5h", "5h", 36, 1_781_105_120),
                    window("7d", "Week", 22, 1_781_550_240),
                    window("month", "Month", 73, 1_782_647_080),
                ],
            ),
            provider(
                "plain",
                "PLAIN",
                theme(
                    "#7C8CA5", "#141820", "#1B202B", "#151C28", "#202838", "#2A303B", "#2D3442",
                ),
                None,
                vec![
                    window("5h", "5h", 42, 1_781_103_300),
                    window("7d", "Week", 13, 1_781_621_700),
                ],
            ),
        ],
    }
}

fn provider(
    id: &str,
    label: &str,
    theme: UsageTheme,
    pixel_art: Option<UsagePixelArt>,
    windows: Vec<UsageWindow>,
) -> UsageProvider {
    UsageProvider {
        id: id.to_string(),
        label: label.to_string(),
        theme_color: Some(theme.accent.clone()),
        theme: Some(theme),
        pixel_art,
        source: "preview".to_string(),
        account: None,
        plan: None,
        windows,
    }
}

fn window(kind: &str, label: &str, used_percent: u8, resets_at_unix: u64) -> UsageWindow {
    UsageWindow {
        kind: kind.to_string(),
        label: label.to_string(),
        used_percent,
        resets_at: Some(format!("unix:{resets_at_unix}")),
        resets_at_unix: Some(resets_at_unix),
        status: "live".to_string(),
    }
}

fn theme(
    accent: &str,
    panel: &str,
    panel_soft: &str,
    primary_panel: &str,
    primary_panel_soft: &str,
    track: &str,
    pill: &str,
) -> UsageTheme {
    UsageTheme {
        accent: accent.to_string(),
        panel: panel.to_string(),
        panel_soft: panel_soft.to_string(),
        primary_panel: primary_panel.to_string(),
        primary_panel_soft: primary_panel_soft.to_string(),
        track: track.to_string(),
        pill: pill.to_string(),
    }
}

fn filled_art(color: &str, size: usize) -> UsagePixelArt {
    UsagePixelArt {
        palette: vec![color.to_string()],
        rows: vec!["1".repeat(size); size],
    }
}

fn two_tone_art(primary: &str, secondary: &str, size: usize) -> UsagePixelArt {
    let rows = (0..size)
        .map(|y| {
            (0..size)
                .map(|x| {
                    if x < 6 || y < 6 || x + 6 >= size || y + 6 >= size {
                        '2'
                    } else {
                        '1'
                    }
                })
                .collect()
        })
        .collect();

    UsagePixelArt {
        palette: vec![primary.to_string(), secondary.to_string()],
        rows,
    }
}

fn write_logical_png(path: &PathBuf, physical_pixels: &[u16]) -> io::Result<()> {
    let width = UI_WIDTH;
    let height = UI_HEIGHT;
    let mut image = Vec::with_capacity(width * height * 3);
    for logical_y in 0..height {
        for logical_x in 0..width {
            let physical_x = logical_y;
            let physical_y = LCD_V_RES - 1 - logical_x;
            let pixel = physical_pixels[physical_y * LCD_H_RES + physical_x];
            let (red, green, blue) = rgb565_to_rgb888(pixel);
            image.extend([red, green, blue]);
        }
    }
    let mut file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(&mut file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&image).map_err(io::Error::other)
}

fn rgb565_to_rgb888(value: u16) -> (u8, u8, u8) {
    let raw = ((value & 0x00ff) << 8) | (value >> 8);
    let red = ((raw >> 11) & 0x1f) as u8;
    let green = ((raw >> 5) & 0x3f) as u8;
    let blue = (raw & 0x1f) as u8;
    (
        (red << 3) | (red >> 2),
        (green << 2) | (green >> 4),
        (blue << 3) | (blue >> 2),
    )
}
