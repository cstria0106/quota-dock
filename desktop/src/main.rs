mod app;
mod firmware;
mod settings;
mod sync;
mod worker;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 680.0])
            .with_min_inner_size([760.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "QuotaDock",
        options,
        Box::new(|cc| Ok(Box::new(app::QuotaDockApp::new(cc)))),
    )
}
