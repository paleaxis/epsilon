//! Copy rendered documents to the system clipboard as PNG images.
//!
//! Task 6.5 needs a real image on the desktop clipboard, not just the in-app
//! element payload. The document is rendered through the same deterministic
//! `tiny-skia` pipeline as file export, then handed to `arboard` as straight
//! RGBA8; `arboard` advertises and encodes it as `image/png` for other
//! applications. Using `arboard` directly (rather than egui's fire-and-forget
//! `Context::copy_image`) is what makes clipboard failures reportable.

use std::fmt;

use arboard::{Clipboard, ImageData};
use epsilon_core::document::Document;
use epsilon_core::export::{ExportOptions, ExportSize};

use crate::export::{self, PngError};

/// Why the scene could not be placed on the system clipboard.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardImageError {
    /// The document could not be rendered (empty scene, invalid scale, too
    /// large, or a rasterization failure).
    Render(PngError),
    /// No clipboard backend was available, or the platform rejected the image.
    Clipboard(String),
}

impl fmt::Display for ClipboardImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render(error) => write!(f, "{error}"),
            Self::Clipboard(error) => write!(f, "the system clipboard is unavailable: {error}"),
        }
    }
}

impl std::error::Error for ClipboardImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render(error) => Some(error),
            Self::Clipboard(_) => None,
        }
    }
}

/// The app-owned clipboard backend.
///
/// Keeping the `arboard` clipboard alive for the session lets X11 clipboard
/// managers or the Wayland data-control source keep serving the copied image
/// after the copy command returns; creating and dropping it per copy can lose
/// the content before another application asks for it.
#[derive(Default)]
pub struct ClipboardImage {
    clipboard: Option<Clipboard>,
}

impl ClipboardImage {
    /// Renders `document` and copies it to the system clipboard as a PNG image.
    ///
    /// Returns the exported image dimensions so the caller can report what was
    /// copied. An empty document fails before the clipboard is touched.
    pub fn copy(
        &mut self,
        document: &Document,
        options: &ExportOptions,
    ) -> Result<ExportSize, ClipboardImageError> {
        let image = export::render_rgba(document, options).map_err(ClipboardImageError::Render)?;
        if self.clipboard.is_none() {
            self.clipboard = Some(
                Clipboard::new()
                    .map_err(|error| ClipboardImageError::Clipboard(error.to_string()))?,
            );
        }
        let clipboard = self.clipboard.as_mut().ok_or_else(|| {
            ClipboardImageError::Clipboard("the clipboard backend was not initialized".to_owned())
        })?;
        let data = ImageData {
            width: image.size.width as usize,
            height: image.size.height as usize,
            bytes: image.bytes.into(),
        };
        clipboard
            .set_image(data)
            .map_err(|error| ClipboardImageError::Clipboard(error.to_string()))?;
        Ok(image.size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epsilon_core::export::ExportError;

    #[test]
    fn render_errors_keep_the_export_message() {
        let mut clipboard = ClipboardImage::default();
        let error = clipboard
            .copy(&Document::new(), &ExportOptions::default())
            .unwrap_err();
        assert_eq!(
            error,
            ClipboardImageError::Render(PngError::Layout(ExportError::EmptyScene))
        );
        assert_eq!(error.to_string(), "there is nothing to export");
    }

    #[test]
    fn clipboard_failures_are_explained() {
        let error = ClipboardImageError::Clipboard("no display".to_owned());
        assert_eq!(
            error.to_string(),
            "the system clipboard is unavailable: no display"
        );
    }
}
