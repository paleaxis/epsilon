//! Deterministic CPU rasterization of documents to PNG (Stage 6).
//!
//! The renderer paints document geometry directly with `tiny-skia` paths and
//! `fontdue` glyph coverage. It never captures the live viewport, so exports
//! are independent of UI panels, window size and current zoom.

use std::fmt;

use eframe::egui;
use epsilon_core::document::{
    Color, Document, Element, ElementKind, FillStyle, StrokeStyle, TextAlign,
};
use epsilon_core::export::{self, ExportError, ExportOptions, ExportSize};
use epsilon_core::geometry::{Point, Rect, arrowhead_points, rotate_point, rough_polyline};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

use crate::app::{ARROW_HEAD_HALF_WIDTH_PIXELS, ARROW_HEAD_LENGTH_PIXELS, ROUGH_AMPLITUDE_PIXELS};

/// Hatch line spacing in world units, matching the canvas renderer.
const HATCH_SPACING: f64 = 8.0;
/// Dash and gap lengths in world units.
const DASH_LENGTH: f64 = 8.0;
const DASH_GAP: f64 = 6.0;
const DOT_LENGTH: f64 = 1.0;
const DOT_GAP: f64 = 4.0;
/// Samples used to approximate an ellipse.
const ELLIPSE_SEGMENTS: usize = 64;
/// Upper bound on hatch lines per fill, so huge shapes stay bounded.
const MAX_HATCH_LINES: usize = 4_000;

/// A rendered PNG and the dimensions it was produced at.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPng {
    pub bytes: Vec<u8>,
    pub size: ExportSize,
}

/// Straight (non-premultiplied) RGBA8 pixels and their dimensions, ready for
/// the system clipboard.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedRgba {
    pub bytes: Vec<u8>,
    pub size: ExportSize,
}

/// Why a PNG could not be rendered.
#[derive(Debug, Clone, PartialEq)]
pub enum PngError {
    /// The scene is empty, the scale is invalid or the image is too large.
    Layout(ExportError),
    /// The embedded font could not be read or parsed.
    Font(String),
    /// Allocation or PNG encoding failed.
    Raster(String),
}

impl From<ExportError> for PngError {
    fn from(error: ExportError) -> Self {
        Self::Layout(error)
    }
}

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout(error) => write!(f, "{error}"),
            Self::Font(error) => write!(f, "text could not be rendered: {error}"),
            Self::Raster(error) => write!(f, "image could not be encoded: {error}"),
        }
    }
}

impl std::error::Error for PngError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Layout(error) => Some(error),
            _ => None,
        }
    }
}

/// Projects world coordinates into the exported image.
#[derive(Debug, Clone, Copy)]
struct Projection {
    origin: Point,
    scale: f64,
}

impl Projection {
    fn point(self, point: Point) -> Point {
        Point::new(
            (point.x - self.origin.x) * self.scale,
            (point.y - self.origin.y) * self.scale,
        )
    }
}

/// Renders the document to a PNG cropped to its scene bounds.
pub fn render_png(document: &Document, options: &ExportOptions) -> Result<RenderedPng, PngError> {
    let (pixmap, size) = render_pixmap(document, options)?;
    let bytes = pixmap
        .encode_png()
        .map_err(|error| PngError::Raster(error.to_string()))?;
    Ok(RenderedPng { bytes, size })
}

/// Renders the document to straight RGBA8 pixels for the system clipboard.
///
/// Clipboard APIs take unmultiplied alpha, while tiny-skia stores
/// premultiplied pixels, so the buffer is converted once here instead of
/// leaking that detail into the clipboard integration.
pub fn render_rgba(document: &Document, options: &ExportOptions) -> Result<RenderedRgba, PngError> {
    let (pixmap, size) = render_pixmap(document, options)?;
    Ok(RenderedRgba {
        bytes: unpremultiply(pixmap.data()),
        size,
    })
}

fn render_pixmap(
    document: &Document,
    options: &ExportOptions,
) -> Result<(Pixmap, ExportSize), PngError> {
    let size = export::export_size(document, options)?;
    let mut pixmap = Pixmap::new(size.width, size.height).ok_or_else(|| {
        PngError::Raster(format!(
            "could not allocate a {}×{} image",
            size.width, size.height
        ))
    })?;
    if let Some(background) = options.background.and_then(|color| skia_color(color, 1.0)) {
        pixmap.fill(background);
    }

    let font = fontdue::Font::from_bytes(embedded_font_bytes()?, fontdue::FontSettings::default())
        .map_err(|error| PngError::Font(error.to_owned()))?;

    let projection = Projection {
        origin: size.bounds.min,
        scale: f64::from(size.scale),
    };
    for element in document.paint_order() {
        paint_element(&mut pixmap, &font, element, projection, options.rough);
    }

    Ok((pixmap, size))
}

/// Converts premultiplied RGBA8 to straight RGBA8, preserving pixel order.
fn unpremultiply(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for pixel in data.as_chunks::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        for channel in &pixel[..3] {
            let straight = (u32::from(*channel) * 255 + alpha / 2) / alpha;
            out.push(straight.min(255) as u8);
        }
        out.push(pixel[3]);
    }
    out
}

/// The font bytes embedded in the application UI, used for exported text.
pub fn embedded_font_bytes() -> Result<Vec<u8>, PngError> {
    let definitions = egui::FontDefinitions::default();
    let data = definitions
        .font_data
        .get("Ubuntu-Light")
        .ok_or_else(|| PngError::Font("the embedded Ubuntu font is unavailable".to_owned()))?;
    Ok(data.font.as_ref().as_ref().to_vec())
}

fn skia_color(color: Color, opacity: f64) -> Option<tiny_skia::Color> {
    let alpha = (f64::from(color.a) * opacity.clamp(0.0, 1.0)).round() as u8;
    (alpha > 0).then(|| tiny_skia::Color::from_rgba8(color.r, color.g, color.b, alpha))
}

fn solid_paint(color: tiny_skia::Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    paint
}

fn stroke_style(width: f32) -> Stroke {
    Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    }
}

fn polyline_path(points: &[Point], closed: bool) -> Option<Path> {
    let (first, rest) = points.split_first()?;
    let mut builder = PathBuilder::new();
    builder.move_to(first.x as f32, first.y as f32);
    for point in rest {
        builder.line_to(point.x as f32, point.y as f32);
    }
    if closed {
        builder.close();
    }
    builder.finish()
}

fn rounded_rect_path(rect: tiny_skia::Rect, radius: f32) -> Option<Path> {
    let radius = radius.max(0.0).min(rect.width().min(rect.height()) / 2.0);
    if radius <= 0.0 {
        return Some(PathBuilder::from_rect(rect));
    }
    let (left, top, right, bottom) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let mut builder = PathBuilder::new();
    builder.move_to(left + radius, top);
    builder.line_to(right - radius, top);
    builder.quad_to(right, top, right, top + radius);
    builder.line_to(right, bottom - radius);
    builder.quad_to(right, bottom, right - radius, bottom);
    builder.line_to(left + radius, bottom);
    builder.quad_to(left, bottom, left, bottom - radius);
    builder.line_to(left, top + radius);
    builder.quad_to(left, top, left + radius, top);
    builder.close();
    builder.finish()
}

fn projected_rect(bounds: Rect, projection: Projection) -> Option<tiny_skia::Rect> {
    let min = projection.point(bounds.min);
    let max = projection.point(bounds.max);
    tiny_skia::Rect::from_ltrb(min.x as f32, min.y as f32, max.x as f32, max.y as f32)
}

fn fill_polygon(
    pixmap: &mut Pixmap,
    points: &[Point],
    style: FillStyle,
    color: tiny_skia::Color,
    scale: f64,
) {
    if points.len() < 3 {
        return;
    }
    match style {
        FillStyle::Solid => {
            if let Some(path) = polyline_path(points, true) {
                pixmap.fill_path(
                    &path,
                    &solid_paint(color),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        FillStyle::Hatch | FillStyle::CrossHatch => {
            let cross = style == FillStyle::CrossHatch;
            let segments = hatch_segments(points, HATCH_SPACING * scale, cross);
            let mut builder = PathBuilder::new();
            let mut any = false;
            for (start, end) in segments {
                builder.move_to(start.x as f32, start.y as f32);
                builder.line_to(end.x as f32, end.y as f32);
                any = true;
            }
            if any && let Some(path) = builder.finish() {
                pixmap.stroke_path(
                    &path,
                    &solid_paint(color),
                    &stroke_style(scale as f32),
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

/// Scanline intersections of a polygon with parallel hatch lines.
fn hatch_segments(polygon: &[Point], spacing: f64, cross: bool) -> Vec<(Point, Point)> {
    if polygon.len() < 3 || !spacing.is_finite() || spacing <= 0.0 {
        return Vec::new();
    }
    let mut segments = Vec::new();
    for sign in [1.0, -1.0] {
        if sign < 0.0 && !cross {
            break;
        }
        let values: Vec<f64> = polygon
            .iter()
            .map(|point| point.y + sign * point.x)
            .collect();
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !min.is_finite() || !max.is_finite() {
            continue;
        }
        let count = (((max - min) / spacing).ceil() as usize).min(MAX_HATCH_LINES);
        for index in 0..=count {
            let offset = min + index as f64 * spacing;
            let mut hits = Vec::new();
            for edge in 0..polygon.len() {
                let previous = polygon[(edge + polygon.len() - 1) % polygon.len()];
                let current = polygon[edge];
                let previous_value = previous.y + sign * previous.x;
                let current_value = current.y + sign * current.x;
                let denominator = current_value - previous_value;
                if denominator.abs() <= f64::EPSILON {
                    continue;
                }
                let t = (offset - previous_value) / denominator;
                if (0.0..=1.0).contains(&t) {
                    hits.push(previous + (current - previous) * t);
                }
            }
            hits.sort_by(|left, right| left.x.total_cmp(&right.x));
            for pair in hits.as_chunks::<2>().0 {
                segments.push((pair[0], pair[1]));
            }
        }
    }
    segments
}

/// Applies the element rotation, optional seeded jitter and export projection.
fn projected_points(
    element: &Element,
    local_points: impl IntoIterator<Item = Point>,
    closed: bool,
    rough: bool,
    projection: Projection,
) -> Vec<Point> {
    let center = element.bounds().center();
    let world: Vec<Point> = local_points
        .into_iter()
        .map(|point| rotate_point(point, center, element.angle))
        .collect();
    let world = if rough {
        rough_polyline(
            &world,
            element.seed,
            ROUGH_AMPLITUDE_PIXELS / projection.scale,
            closed,
        )
    } else {
        world
    };
    world
        .into_iter()
        .map(|point| projection.point(point))
        .collect()
}

fn stroke_polyline(
    pixmap: &mut Pixmap,
    points: &[Point],
    closed: bool,
    color: tiny_skia::Color,
    width: f32,
    style: StrokeStyle,
    scale: f64,
) {
    if points.len() < 2 || width <= 0.0 {
        return;
    }
    let stroke = stroke_style(width);
    match style {
        StrokeStyle::Solid => {
            if let Some(path) = polyline_path(points, closed) {
                pixmap.stroke_path(
                    &path,
                    &solid_paint(color),
                    &stroke,
                    Transform::identity(),
                    None,
                );
            }
        }
        StrokeStyle::Dashed | StrokeStyle::Dotted => {
            let (dash, gap) = match style {
                StrokeStyle::Dotted => (DOT_LENGTH * scale, DOT_GAP * scale),
                _ => (DASH_LENGTH * scale, DASH_GAP * scale),
            };
            if let Some(path) = dashed_path(points, closed, dash, gap) {
                pixmap.stroke_path(
                    &path,
                    &solid_paint(color),
                    &stroke,
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

/// Builds one path containing every dash sub-segment, so a dashed stroke is a
/// single rasterization pass.
fn dashed_path(points: &[Point], closed: bool, dash: f64, gap: f64) -> Option<Path> {
    if dash <= 0.0 {
        return polyline_path(points, closed);
    }
    let segment_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let mut builder = PathBuilder::new();
    for index in 0..segment_count {
        let start = points[index];
        let end = points[(index + 1) % points.len()];
        let delta = end - start;
        let length = delta.length();
        if length <= f64::EPSILON {
            continue;
        }
        let direction = delta * (1.0 / length);
        let mut offset = 0.0;
        while offset < length {
            let dash_end = (offset + dash).min(length);
            let from = start + direction * offset;
            let to = start + direction * dash_end;
            builder.move_to(from.x as f32, from.y as f32);
            builder.line_to(to.x as f32, to.y as f32);
            offset = dash_end + gap;
        }
    }
    builder.finish()
}

fn ellipse_local_points(bounds: Rect) -> Vec<Point> {
    let center = bounds.center();
    let radius = bounds.size() * 0.5;
    (0..ELLIPSE_SEGMENTS)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / ELLIPSE_SEGMENTS as f64;
            Point::new(
                center.x + angle.cos() * radius.x,
                center.y + angle.sin() * radius.y,
            )
        })
        .collect()
}

fn paint_element(
    pixmap: &mut Pixmap,
    font: &fontdue::Font,
    element: &Element,
    projection: Projection,
    rough: bool,
) {
    let opacity = element.style.opacity.clamp(0.0, 1.0);
    let stroke = skia_color(element.style.stroke, opacity);
    let fill = skia_color(element.style.fill, opacity);
    let scale = projection.scale;
    // Match the canvas' minimum on-screen stroke, scaled with the image.
    let stroke_width = (element.style.stroke_width.max(0.0) * scale).max(0.75 * scale) as f32;
    let roundness = (element.style.roundness.max(0.0) * scale) as f32;
    let axis_aligned = element.angle.abs() <= 1e-9;
    let clean = !rough;

    match element.kind {
        ElementKind::Rectangle => {
            let bounds = element.bounds();
            let corners = [
                bounds.min,
                Point::new(bounds.max.x, bounds.min.y),
                bounds.max,
                Point::new(bounds.min.x, bounds.max.y),
            ];
            let fill_points = projected_points(element, corners, true, false, projection);
            let rounded = (clean && axis_aligned && element.style.fill_style == FillStyle::Solid)
                .then(|| projected_rect(bounds, projection))
                .flatten()
                .and_then(|rect| rounded_rect_path(rect, roundness));
            if let Some(fill) = fill {
                if let Some(path) = rounded {
                    pixmap.fill_path(
                        &path,
                        &solid_paint(fill),
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                } else {
                    fill_polygon(pixmap, &fill_points, element.style.fill_style, fill, scale);
                }
            }
            let rounded =
                (clean && axis_aligned && element.style.stroke_style == StrokeStyle::Solid)
                    .then(|| projected_rect(bounds, projection))
                    .flatten()
                    .and_then(|rect| rounded_rect_path(rect, roundness));
            if let Some(stroke) = stroke {
                if let Some(path) = rounded {
                    pixmap.stroke_path(
                        &path,
                        &solid_paint(stroke),
                        &stroke_style(stroke_width),
                        Transform::identity(),
                        None,
                    );
                } else {
                    let stroke_points = projected_points(element, corners, true, rough, projection);
                    stroke_polyline(
                        pixmap,
                        &stroke_points,
                        true,
                        stroke,
                        stroke_width,
                        element.style.stroke_style,
                        scale,
                    );
                }
            }
        }
        ElementKind::Ellipse => {
            let bounds = element.bounds();
            let local_points = ellipse_local_points(bounds);
            let fill_points = projected_points(
                element,
                local_points.iter().copied(),
                true,
                false,
                projection,
            );
            let exact_oval = (clean && axis_aligned)
                .then(|| projected_rect(bounds, projection))
                .flatten()
                .and_then(|rect| {
                    let mut builder = PathBuilder::new();
                    builder.push_oval(rect);
                    builder.finish()
                });
            if let Some(fill) = fill {
                if let Some(path) = exact_oval.as_ref()
                    && element.style.fill_style == FillStyle::Solid
                {
                    pixmap.fill_path(
                        path,
                        &solid_paint(fill),
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                } else {
                    fill_polygon(pixmap, &fill_points, element.style.fill_style, fill, scale);
                }
            }
            if let Some(stroke) = stroke {
                if let Some(path) = exact_oval.as_ref()
                    && element.style.stroke_style == StrokeStyle::Solid
                {
                    pixmap.stroke_path(
                        path,
                        &solid_paint(stroke),
                        &stroke_style(stroke_width),
                        Transform::identity(),
                        None,
                    );
                } else {
                    let stroke_points = projected_points(
                        element,
                        local_points.iter().copied(),
                        true,
                        rough,
                        projection,
                    );
                    stroke_polyline(
                        pixmap,
                        &stroke_points,
                        true,
                        stroke,
                        stroke_width,
                        element.style.stroke_style,
                        scale,
                    );
                }
            }
        }
        ElementKind::Diamond => {
            let local_points = element.bounds().diamond_points();
            if let Some(fill) = fill {
                let fill_points = projected_points(element, local_points, true, false, projection);
                fill_polygon(pixmap, &fill_points, element.style.fill_style, fill, scale);
            }
            if let Some(stroke) = stroke {
                let stroke_points =
                    projected_points(element, local_points, true, rough, projection);
                stroke_polyline(
                    pixmap,
                    &stroke_points,
                    true,
                    stroke,
                    stroke_width,
                    element.style.stroke_style,
                    scale,
                );
            }
        }
        ElementKind::Line | ElementKind::Arrow | ElementKind::Freehand => {
            if element.points.len() < 2 {
                return;
            }
            let stroke_points = projected_points(
                element,
                element.points.iter().copied(),
                false,
                rough,
                projection,
            );
            if let Some(stroke) = stroke {
                stroke_polyline(
                    pixmap,
                    &stroke_points,
                    false,
                    stroke,
                    stroke_width,
                    element.style.stroke_style,
                    scale,
                );
                if element.kind == ElementKind::Arrow {
                    paint_arrowhead(pixmap, element, stroke, projection);
                }
            }
        }
        ElementKind::Text => {
            if let Some(color) = stroke {
                paint_text(pixmap, font, element, color, projection);
            }
        }
    }
}

fn paint_arrowhead(
    pixmap: &mut Pixmap,
    element: &Element,
    color: tiny_skia::Color,
    projection: Projection,
) {
    let [start, end] = element.points.as_slice() else {
        return;
    };
    let center = element.bounds().center();
    let start = rotate_point(*start, center, element.angle);
    let end = rotate_point(*end, center, element.angle);
    let Some(head) = arrowhead_points(
        start,
        end,
        ARROW_HEAD_LENGTH_PIXELS,
        ARROW_HEAD_HALF_WIDTH_PIXELS,
    ) else {
        return;
    };
    let points: Vec<Point> = head
        .into_iter()
        .map(|point| projection.point(point))
        .collect();
    fill_polygon(pixmap, &points, FillStyle::Solid, color, projection.scale);
}

fn measure(font: &fontdue::Font, text: &str, size: f32) -> f32 {
    text.chars()
        .map(|character| font.metrics(character, size).advance_width)
        .sum()
}

/// Greedy word wrapping with explicit newlines; the document model stores the
/// wrap width, so exports use the same width as the canvas.
fn layout_lines(font: &fontdue::Font, text: &str, size: f32, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if max_width <= 1.0 {
            lines.push(paragraph.to_owned());
            continue;
        }
        let mut line = String::new();
        let mut line_width = 0.0;
        for word in paragraph.split_whitespace() {
            let word_width = measure(font, word, size);
            let space_width = if line.is_empty() {
                0.0
            } else {
                measure(font, " ", size)
            };
            if !line.is_empty() && line_width + space_width + word_width > max_width {
                lines.push(std::mem::take(&mut line));
                line_width = 0.0;
            }
            if !line.is_empty() {
                line.push(' ');
                line_width += space_width;
            }
            line.push_str(word);
            line_width += word_width;
        }
        lines.push(line);
    }
    lines
}

fn paint_text(
    pixmap: &mut Pixmap,
    font: &fontdue::Font,
    element: &Element,
    color: tiny_skia::Color,
    projection: Projection,
) {
    if element.text.is_empty() {
        return;
    }
    let size = (element.font_size * projection.scale) as f32;
    if !size.is_finite() || size <= 0.5 {
        return;
    }
    let max_width = (element.width * projection.scale) as f32;
    let lines = layout_lines(font, &element.text, size, max_width);
    let line_metrics = font.horizontal_line_metrics(size);
    let ascent = line_metrics.map_or(size * 0.8, |metrics| metrics.ascent);
    let line_height = line_metrics
        .map(|metrics| metrics.new_line_size)
        .filter(|height| *height > 0.0)
        .unwrap_or(size * 1.25);
    let top = projection.point(Point::new(element.x, element.y));

    for (index, line) in lines.iter().enumerate() {
        let width = measure(font, line, size);
        let offset = match element.text_align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (max_width - width) / 2.0,
            TextAlign::Right => max_width - width,
        };
        let mut pen_x = top.x as f32 + offset;
        let baseline = top.y as f32 + ascent + index as f32 * line_height;
        for character in line.chars() {
            let (metrics, coverage) = font.rasterize(character, size);
            if metrics.width > 0 && metrics.height > 0 {
                let glyph_x = pen_x.round() as i32 + metrics.xmin;
                let glyph_y = baseline.round() as i32 - (metrics.ymin + metrics.height as i32);
                blend_glyph(
                    pixmap,
                    glyph_x,
                    glyph_y,
                    metrics.width,
                    metrics.height,
                    &coverage,
                    color,
                );
            }
            pen_x += metrics.advance_width;
        }
    }
}

/// Blends an 8-bit glyph coverage mask into tiny-skia's premultiplied RGBA
/// buffer, so transparent exports keep correct glyph alpha.
fn blend_glyph(
    pixmap: &mut Pixmap,
    origin_x: i32,
    origin_y: i32,
    width: usize,
    height: usize,
    coverage: &[u8],
    color: tiny_skia::Color,
) {
    let pixmap_width = pixmap.width() as i32;
    let pixmap_height = pixmap.height() as i32;
    // tiny-skia stores premultiplied RGBA as straight bytes.
    let red = (color.red() * 255.0).round() as u32;
    let green = (color.green() * 255.0).round() as u32;
    let blue = (color.blue() * 255.0).round() as u32;
    // `color` already carries the element opacity in its alpha channel.
    let base_alpha = f64::from(color.alpha());
    let row_stride = pixmap.width() as usize;
    let data = pixmap.data_mut();
    for row in 0..height {
        for column in 0..width {
            let coverage = u32::from(coverage[row * width + column]);
            if coverage == 0 {
                continue;
            }
            let x = origin_x + column as i32;
            let y = origin_y + row as i32;
            if x < 0 || y < 0 || x >= pixmap_width || y >= pixmap_height {
                continue;
            }
            let alpha = (f64::from(coverage) * base_alpha).round() as u32;
            if alpha == 0 {
                continue;
            }
            let inverse = 255 - alpha;
            let offset = (y as usize * row_stride + x as usize) * 4;
            for (channel, value) in [red, green, blue].into_iter().enumerate() {
                let source = value * alpha / 255;
                data[offset + channel] =
                    (source + u32::from(data[offset + channel]) * inverse / 255) as u8;
            }
            data[offset + 3] = (alpha + u32::from(data[offset + 3]) * inverse / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epsilon_core::document::{Element, ElementKind};

    fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Element {
        let mut element = Element::new(ElementKind::Rectangle, x, y, width, height);
        element.style.stroke_width = 0.0;
        element
    }

    fn render(document: &Document, options: &ExportOptions) -> RenderedPng {
        render_png(document, options).expect("render PNG")
    }

    fn decode(rendered: &RenderedPng) -> Pixmap {
        Pixmap::decode_png(&rendered.bytes).expect("decode PNG")
    }

    fn is_opaque_white(pixel: &tiny_skia::PremultipliedColorU8) -> bool {
        pixel.red() == 255 && pixel.green() == 255 && pixel.blue() == 255 && pixel.alpha() == 255
    }

    #[test]
    fn png_dimensions_follow_scene_bounds_and_scale() {
        let mut document = Document::new();
        document.add(rectangle(20.0, 30.0, 120.0, 60.0));

        let rendered = render(
            &document,
            &ExportOptions {
                scale: export::ExportScale::Two,
                rough: false,
                ..ExportOptions::default()
            },
        );
        // 120×60 plus the half-world-unit margin on each side, at 2×.
        assert_eq!((rendered.size.width, rendered.size.height), (242, 122));
        let decoded = decode(&rendered);
        assert_eq!((decoded.width(), decoded.height()), (242, 122));
    }

    #[test]
    fn exported_content_fills_the_cropped_scene() {
        let mut document = Document::new();
        let mut element = rectangle(1_000.0, 2_000.0, 100.0, 50.0);
        element.style.fill = Color::rgb(20, 40, 200);
        document.add(element);

        let rendered = render(
            &document,
            &ExportOptions {
                background: Some(Color::WHITE),
                rough: false,
                ..ExportOptions::default()
            },
        );
        // The crop ignores the document's absolute position, so a scene far
        // from the world origin produces the same small image.
        assert_eq!((rendered.size.width, rendered.size.height), (102, 52));
        let decoded = decode(&rendered);
        let mut min = (u32::MAX, u32::MAX);
        let mut max = (0, 0);
        for (index, pixel) in decoded.pixels().iter().enumerate() {
            if !is_opaque_white(pixel) {
                let x = index as u32 % decoded.width();
                let y = index as u32 / decoded.width();
                min = (min.0.min(x), min.1.min(y));
                max = (max.0.max(x), max.1.max(y));
            }
        }
        // One pixel of stroke margin around the 100×50 rectangle.
        assert!(min.0 <= 2 && min.1 <= 2, "content starts at {min:?}");
        assert!(max.0 >= 99 && max.1 >= 49, "content ends at {max:?}");
        assert!(max.0 <= 101 && max.1 <= 51, "content overflows to {max:?}");
    }

    #[test]
    fn transparent_background_keeps_corner_alpha_zero() {
        let mut document = Document::new();
        document.add(rectangle(10.0, 10.0, 10.0, 10.0));
        let rendered = render(
            &document,
            &ExportOptions {
                rough: false,
                ..ExportOptions::default()
            },
        );
        let decoded = decode(&rendered);
        // The unfilled interior stays transparent while the stroke is opaque.
        let center = (decoded.width() / 2, decoded.height() / 2);
        assert_eq!(
            decoded.pixel(center.0, center.1).expect("center").alpha(),
            0
        );
        assert!(decoded.pixels().iter().any(|pixel| pixel.alpha() > 0));
    }

    #[test]
    fn text_is_rasterized_into_the_export() {
        let mut document = Document::new();
        let mut text = Element::text(10.0, 10.0, 200.0, 48.0);
        text.text = "Epsilon PNG".to_owned();
        text.style.stroke = Color::BLACK;
        document.add(text);

        let rendered = render(
            &document,
            &ExportOptions {
                background: Some(Color::WHITE),
                rough: false,
                ..ExportOptions::default()
            },
        );
        let decoded = decode(&rendered);
        let inked = decoded
            .pixels()
            .iter()
            .filter(|pixel| !is_opaque_white(pixel))
            .count();
        assert!(inked > 200, "expected glyph pixels, found {inked}");
    }

    #[test]
    fn rendering_is_deterministic_for_hand_drawn_strokes() {
        let mut document = Document::new();
        let mut first = rectangle(0.0, 0.0, 80.0, 60.0);
        first.style.fill = Color::rgb(250, 200, 120);
        document.add(first);
        document.add(Element::arrow(Point::new(0.0, 0.0), Point::new(80.0, 60.0)));
        let options = ExportOptions {
            background: Some(Color::WHITE),
            ..ExportOptions::default()
        };
        let first = render(&document, &options);
        let second = render(&document, &options);
        assert_eq!(first.bytes, second.bytes);
    }

    #[test]
    fn rgba_export_is_straight_alpha_at_the_same_dimensions() {
        let mut document = Document::new();
        let mut element = rectangle(10.0, 10.0, 100.0, 100.0);
        element.style.fill = Color::rgb(200, 100, 50);
        element.style.opacity = 0.5;
        document.add(element);

        let rgba = render_rgba(&document, &ExportOptions::default()).expect("render RGBA");
        let png = render(&document, &ExportOptions::default());
        assert_eq!((rgba.size.width, rgba.size.height), (102, 102));
        assert_eq!(png.size, rgba.size);
        assert_eq!(rgba.bytes.len(), 102 * 102 * 4);

        // A transparent corner stays zero and the semi-transparent fill is
        // un-premultiplied back to its straight color and 50% alpha.
        let corner = &rgba.bytes[..4];
        assert_eq!(corner, &[0, 0, 0, 0]);
        let center = (51 * 102 + 51) * 4;
        let pixel = &rgba.bytes[center..center + 4];
        assert!((pixel[0] as i32 - 200).abs() <= 2, "red was {pixel:?}");
        assert!((pixel[1] as i32 - 100).abs() <= 2, "green was {pixel:?}");
        assert!((pixel[2] as i32 - 50).abs() <= 2, "blue was {pixel:?}");
        assert!((pixel[3] as i32 - 128).abs() <= 1, "alpha was {pixel:?}");
    }

    #[test]
    fn oversized_scenes_are_rejected_before_rendering() {
        let mut document = Document::new();
        document.add(rectangle(0.0, 0.0, 5_000.0, 100.0));
        let error = render_png(
            &document,
            &ExportOptions {
                scale: export::ExportScale::Four,
                ..ExportOptions::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            PngError::Layout(ExportError::TooLarge { .. })
        ));
    }
}
