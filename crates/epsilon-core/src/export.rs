//! Export geometry, scale settings and limits shared by the PNG and SVG
//! exporters.
//!
//! Export is a projection of the document, never a capture of the live
//! viewport: the exported image is cropped to the scene's world bounds, so UI
//! panels, zoom and window size cannot influence it.

use std::fmt;

use crate::document::{Document, Element, ElementKind};
use crate::geometry::Rect;

/// Largest allowed scale factor for custom exports.
pub const MAX_EXPORT_SCALE: f32 = 32.0;
/// Largest allowed PNG width or height in pixels.
pub const MAX_EXPORT_DIMENSION: u32 = 16_384;
/// Largest allowed pixel count (64 million pixels, about 256 MiB of RGBA).
pub const MAX_EXPORT_PIXELS: u64 = 64 * 1024 * 1024;

/// Extra world units added around the scene for hand-drawn jitter, expressed in
/// output pixels and converted with the scale.
const ROUGH_MARGIN_PIXELS: f64 = 1.0;
/// Half-width of the arrowhead, which reaches outside the line's own bounds.
const ARROW_HEAD_MARGIN: f64 = 6.0;

/// Preset and custom export scale factors.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ExportScale {
    #[default]
    One,
    Two,
    Four,
    Custom(f32),
}

impl ExportScale {
    /// Presets offered by the export dialog.
    pub const PRESETS: [Self; 3] = [Self::One, Self::Two, Self::Four];

    /// The numeric scale factor.
    pub const fn factor(self) -> f32 {
        match self {
            Self::One => 1.0,
            Self::Two => 2.0,
            Self::Four => 4.0,
            Self::Custom(factor) => factor,
        }
    }
}

/// Options for raster and vector export.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportOptions {
    pub scale: ExportScale,
    /// `None` leaves the image transparent.
    pub background: Option<crate::document::Color>,
    /// Whether seeded hand-drawn jitter is applied, matching the canvas'
    /// clean-rendering setting.
    pub rough: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            scale: ExportScale::default(),
            background: None,
            rough: true,
        }
    }
}

impl ExportOptions {
    /// Exports the scene over its own canvas background.
    pub fn with_document_background(mut self, document: &Document) -> Self {
        self.background = Some(document.canvas.background);
        self
    }
}

/// Pixel dimensions and the world-space crop used to produce them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportSize {
    pub width: u32,
    pub height: u32,
    /// Crop rectangle in world coordinates, including the stroke margin.
    pub bounds: Rect,
    pub scale: f32,
}

/// Why an export could not be produced.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportError {
    /// The document has no elements to export.
    EmptyScene,
    /// The scale is zero, negative, non-finite or above [`MAX_EXPORT_SCALE`].
    InvalidScale { scale: f32 },
    /// The requested image exceeds the dimension or pixel-count limit.
    TooLarge {
        width: u32,
        height: u32,
        pixels: u64,
    },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyScene => write!(f, "there is nothing to export"),
            Self::InvalidScale { scale } => write!(
                f,
                "export scale {scale} is invalid; use a value above 0 and at most {MAX_EXPORT_SCALE}"
            ),
            Self::TooLarge {
                width,
                height,
                pixels,
            } => write!(
                f,
                "the export would be {width}×{height} pixels ({pixels} total), above the {MAX_EXPORT_DIMENSION}-pixel side or {MAX_EXPORT_PIXELS}-pixel total limit"
            ),
        }
    }
}

impl std::error::Error for ExportError {}

/// Union of the elements' rotated world bounds, without stroke allowance.
pub fn content_bounds(document: &Document) -> Option<Rect> {
    document
        .elements
        .iter()
        .map(Element::world_bounds)
        .reduce(|union, bounds| union.union(&bounds))
}

/// World-space crop for an export at `scale`.
///
/// Each element's bounds are expanded by half its stroke width, the hand-drawn
/// jitter margin and, for arrows, the arrowhead width.
pub fn export_bounds(document: &Document, scale: f64) -> Option<Rect> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    document
        .elements
        .iter()
        .map(|element| {
            let mut margin =
                element.style.stroke_width.max(0.0) / 2.0 + ROUGH_MARGIN_PIXELS / scale;
            if element.kind == ElementKind::Arrow {
                margin += ARROW_HEAD_MARGIN;
            }
            element.world_bounds().expand(margin)
        })
        .reduce(|union, bounds| union.union(&bounds))
}

/// Validates the scale and computes the integer pixel size of the export.
pub fn export_size(
    document: &Document,
    options: &ExportOptions,
) -> Result<ExportSize, ExportError> {
    let scale = options.scale.factor();
    if !scale.is_finite() || scale <= 0.0 || scale > MAX_EXPORT_SCALE {
        return Err(ExportError::InvalidScale { scale });
    }
    let scale = f64::from(scale);
    let bounds = export_bounds(document, scale).ok_or(ExportError::EmptyScene)?;

    // The epsilon keeps exact integer sizes from rounding up because of
    // floating-point noise in the margin arithmetic.
    let width = (bounds.width() * scale - 1e-9).ceil().max(1.0);
    let height = (bounds.height() * scale - 1e-9).ceil().max(1.0);
    let pixels = width * height;
    if width > f64::from(MAX_EXPORT_DIMENSION)
        || height > f64::from(MAX_EXPORT_DIMENSION)
        || pixels > MAX_EXPORT_PIXELS as f64
    {
        return Err(ExportError::TooLarge {
            width: width.min(f64::from(u32::MAX)) as u32,
            height: height.min(f64::from(u32::MAX)) as u32,
            pixels: pixels.min(u64::MAX as f64) as u64,
        });
    }

    Ok(ExportSize {
        // Both values are within MAX_EXPORT_DIMENSION and at least 1.
        width: width as u32,
        height: height as u32,
        bounds,
        scale: scale as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Color, Element, ElementKind};

    fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Element {
        let mut element = Element::new(ElementKind::Rectangle, x, y, width, height);
        element.style.stroke_width = 0.0;
        element
    }

    #[test]
    fn content_bounds_union_rotated_elements() {
        let mut document = Document::new();
        document.add(rectangle(0.0, 0.0, 100.0, 50.0));
        let mut rotated = rectangle(200.0, 100.0, 40.0, 20.0);
        rotated.angle = std::f64::consts::FRAC_PI_4;
        document.add(rotated);

        let bounds = content_bounds(&document).expect("bounds");
        assert!(bounds.min.x <= 0.0);
        assert!(bounds.min.y <= 0.0);
        assert!(bounds.max.x >= 200.0);
        assert!(bounds.max.y >= 100.0);
        // 45° rotation grows the 40×20 rectangle to 60/√2 in both axes.
        let rotated_bounds = document.elements[1].world_bounds();
        assert!((rotated_bounds.width() - 60.0 / 2.0_f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn content_bounds_are_empty_without_elements() {
        assert_eq!(content_bounds(&Document::new()), None);
        assert_eq!(export_bounds(&Document::new(), 1.0), None);
    }

    #[test]
    fn export_bounds_include_stroke_and_arrowhead_margins() {
        let mut document = Document::new();
        let mut element = rectangle(10.0, 20.0, 100.0, 50.0);
        element.style.stroke_width = 4.0;
        document.add(element);

        let bounds = export_bounds(&document, 2.0).expect("bounds");
        // Half the stroke plus one output pixel (0.5 world units at 2×).
        assert!((bounds.min.x - (10.0 - 2.0 - 0.5)).abs() < 1e-9);
        assert!((bounds.max.x - (110.0 + 2.0 + 0.5)).abs() < 1e-9);

        document.add(Element::arrow(
            crate::geometry::Point::new(0.0, 0.0),
            crate::geometry::Point::new(30.0, 0.0),
        ));
        let with_arrow = export_bounds(&document, 2.0).expect("bounds");
        assert!(with_arrow.min.x <= -6.0);
        assert!(with_arrow.min.y <= -6.5);
    }

    #[test]
    fn export_size_applies_preset_and_custom_scales() {
        let mut document = Document::new();
        document.add(rectangle(0.0, 0.0, 100.0, 50.0));

        // The margin is one output pixel per side, so its world width shrinks
        // as the scale grows.
        for (scale, expected_width, expected_height) in [
            (ExportScale::One, 102, 52),
            (ExportScale::Two, 202, 102),
            (ExportScale::Four, 402, 202),
            (ExportScale::Custom(1.5), 152, 77),
        ] {
            let options = ExportOptions {
                scale,
                ..ExportOptions::default()
            };
            let size = export_size(&document, &options).expect("size");
            assert_eq!((size.width, size.height), (expected_width, expected_height));
            assert_eq!(size.scale, scale.factor());
            assert!(size.bounds.width() >= 100.0);
            assert!(size.bounds.height() >= 50.0);
        }
    }

    #[test]
    fn export_size_rejects_invalid_scales() {
        let mut document = Document::new();
        document.add(rectangle(0.0, 0.0, 10.0, 10.0));

        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, 64.0] {
            let options = ExportOptions {
                scale: ExportScale::Custom(scale),
                ..ExportOptions::default()
            };
            assert!(matches!(
                export_size(&document, &options),
                Err(ExportError::InvalidScale { .. })
            ));
        }
    }

    #[test]
    fn export_size_rejects_empty_scenes() {
        let error = export_size(&Document::new(), &ExportOptions::default()).unwrap_err();
        assert_eq!(error, ExportError::EmptyScene);
    }

    #[test]
    fn export_size_is_bounded_for_large_scenes() {
        let mut document = Document::new();
        document.add(rectangle(0.0, 0.0, 5_000.0, 100.0));
        let options = ExportOptions {
            scale: ExportScale::Four,
            ..ExportOptions::default()
        };
        assert!(matches!(
            export_size(&document, &options),
            Err(ExportError::TooLarge { .. })
        ));

        let mut square = Document::new();
        square.add(rectangle(0.0, 0.0, 4_000.0, 4_000.0));
        assert!(matches!(
            export_size(&square, &options),
            Err(ExportError::TooLarge { .. })
        ));
    }

    #[test]
    fn options_default_to_transparent_background() {
        let options = ExportOptions::default();
        assert_eq!(options.background, None);
        assert_eq!(options.scale, ExportScale::One);
        assert!(options.rough);

        let mut document = Document::new();
        document.canvas.background = Color::rgb(10, 20, 30);
        assert_eq!(
            ExportOptions::default()
                .with_document_background(&document)
                .background,
            Some(Color::rgb(10, 20, 30))
        );
    }
}
