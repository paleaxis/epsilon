//! Epsilon — a native Rust whiteboard desktop application.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

mod app;
mod clipboard_image;
mod export;
mod icon;
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

    // A path passed by a launcher or file manager is opened once the app is
    // built; options are ignored so `epsilon --some-flag` still starts.
    let startup_document = startup_document(std::env::args_os().skip(1));

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Epsilon")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size(layout::MIN_WINDOW_SIZE)
            .with_icon(Arc::new(icon::app_icon())),
        // Restore the last window geometry from eframe's persistence store.
        persist_window: true,
        ..Default::default()
    };

    eframe::run_native(
        "Epsilon",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::EpsilonApp::with_startup_file(
                cc,
                startup_document,
            )))
        }),
    )
}

/// Returns the first positional argument, treated as a document or Excalidraw
/// scene to open at startup. Options (arguments starting with `-`) are skipped.
fn startup_document(args: impl IntoIterator<Item = OsString>) -> Option<PathBuf> {
    args.into_iter()
        .find(|arg| {
            let text = arg.to_string_lossy();
            !text.is_empty() && !text.starts_with('-')
        })
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn first_positional_argument_is_the_document() {
        assert_eq!(
            startup_document(args(&["drawing.epsilon"])),
            Some(PathBuf::from("drawing.epsilon"))
        );
    }

    #[test]
    fn options_are_skipped() {
        assert_eq!(
            startup_document(args(&["--verbose", "scene.excalidraw"])),
            Some(PathBuf::from("scene.excalidraw"))
        );
        assert_eq!(startup_document(args(&["--help"])), None);
    }

    #[test]
    fn no_arguments_means_no_document() {
        assert_eq!(startup_document(args(&[])), None);
        assert_eq!(startup_document(args(&[""])), None);
    }
}
