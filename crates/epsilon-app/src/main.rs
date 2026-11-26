//! Epsilon — a native Rust whiteboard desktop application.

mod app;
mod clipboard_image;
mod export;
mod layout;
mod logging;
mod notices;
mod svg;
mod theme;
mod tools;

/// Log directory next to eframe's settings (`…/epsilon/logs`). Keeping it
/// inside the app data folder means nothing leaves the machine.
const LOG_DIR: &str = "logs";

fn main() -> eframe::Result {
    logging::install_panic_hook();
    if let Some(dir) = eframe::storage_dir("Epsilon") {
        logging::init(&dir.join(LOG_DIR));
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Epsilon")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size(layout::MIN_WINDOW_SIZE),
        // Restore the last window geometry from eframe's persistence store.
        persist_window: true,
        ..Default::default()
    };

    eframe::run_native(
        "Epsilon",
        options,
        Box::new(|cc| Ok(Box::new(app::EpsilonApp::new(cc)))),
    )
}
