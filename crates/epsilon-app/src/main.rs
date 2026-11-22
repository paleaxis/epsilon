//! Epsilon — a native Rust whiteboard desktop application.

mod app;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Epsilon")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Epsilon",
        options,
        Box::new(|cc| Ok(Box::new(app::EpsilonApp::new(cc)))),
    )
}
