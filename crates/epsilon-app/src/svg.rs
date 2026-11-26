//! Deterministic SVG export of a document (Stage 6).
//!
//! SVG is a vector projection of the document, written in world units with a
//! viewBox cropped to the scene bounds, so it scales without raster artifacts.

use std::fmt::Write as _;

use epsilon_core::document::{
    Color, Document, Element, ElementKind, FillStyle, StrokeStyle, TextAlign,
};
use epsilon_core::export::{ExportOptions, export_bounds};
use epsilon_core::geometry::{Point, Rect, arrowhead_points, rotate_point, rough_polyline};

use crate::app::{ARROW_HEAD_HALF_WIDTH_PIXELS, ARROW_HEAD_LENGTH_PIXELS, ROUGH_AMPLITUDE_PIXELS};

const HATCH_SPACING: f64 = 8.0;

/// Renders the document to an SVG string.
pub fn export_svg(document: &Document, options: &ExportOptions) -> Result<String, String> {
    let scale = options.scale.factor();
    if !scale.is_finite() || scale <= 0.0 || scale > 32.0 {
        return Err(format!("invalid export scale {scale}"));
    }
    let bounds = export_bounds(document, f64::from(scale))
        .ok_or_else(|| "there is nothing to export".to_owned())?;
    let mut svg = String::new();
    let width = bounds.width();
    let height = bounds.height();
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.3}" height="{:.3}" viewBox="{:.6} {:.6} {:.6} {:.6}">"#,
        width * f64::from(scale),
        height * f64::from(scale),
        bounds.min.x,
        bounds.min.y,
        width,
        height
    );
    if let Some(background) = options.background {
        let _ = writeln!(
            svg,
            r#"<rect x="{:.6}" y="{:.6}" width="{:.6}" height="{:.6}" fill="{}" />"#,
            bounds.min.x,
            bounds.min.y,
            width,
            height,
            color_hex(background)
        );
    }

    let mut clip_id = 0usize;
    for element in document.paint_order() {
        render_element(
            &mut svg,
            element,
            options.rough,
            f64::from(scale),
            &mut clip_id,
        );
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

fn color_hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn color_attr(color: Color, opacity: f64) -> (String, String) {
    let alpha = (f64::from(color.a) / 255.0 * opacity.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    if (alpha - 1.0).abs() < 1e-9 {
        (color_hex(color), String::new())
    } else {
        (color_hex(color), format!(r#" opacity="{alpha:.4}""#))
    }
}

fn fmt(n: f64) -> String {
    format!("{n:.6}")
}

fn points_attr(points: &[Point]) -> String {
    points
        .iter()
        .map(|point| format!("{},{}", fmt(point.x), fmt(point.y)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn dash_array(style: StrokeStyle, width: f64) -> String {
    match style {
        StrokeStyle::Solid => String::new(),
        StrokeStyle::Dashed => format!(r#" stroke-dasharray="{:.3} {:.3}""#, 8.0, 6.0),
        StrokeStyle::Dotted => format!(r#" stroke-dasharray="{:.3} {:.3}""#, width.max(0.1), 4.0),
    }
}

fn rotated_points(
    element: &Element,
    points: &[Point],
    rough: bool,
    amplitude: f64,
    closed: bool,
) -> Vec<Point> {
    let center = element.bounds().center();
    let world: Vec<Point> = points
        .iter()
        .map(|point| rotate_point(*point, center, element.angle))
        .collect();
    if rough {
        rough_polyline(&world, element.seed, amplitude, closed)
    } else {
        world
    }
}

fn render_element(
    svg: &mut String,
    element: &Element,
    rough: bool,
    scale: f64,
    clip_id: &mut usize,
) {
    let opacity = element.style.opacity.clamp(0.0, 1.0);
    let stroke_width = element.style.stroke_width.max(0.0);
    let amplitude = ROUGH_AMPLITUDE_PIXELS / scale;
    let center = element.bounds().center();

    match element.kind {
        ElementKind::Rectangle => {
            let bounds = element.bounds();
            let corners = [
                bounds.min,
                Point::new(bounds.max.x, bounds.min.y),
                bounds.max,
                Point::new(bounds.min.x, bounds.max.y),
            ];
            let points = rotated_points(element, &corners, rough, amplitude, true);
            let (fill_color, fill_opacity) = color_attr(element.style.fill, opacity);
            let (stroke_color, stroke_opacity) = color_attr(element.style.stroke, opacity);
            let mut out = String::new();
            if element.style.fill.a > 0 || element.style.fill_style != FillStyle::Solid {
                match element.style.fill_style {
                    FillStyle::Solid => {
                        if element.angle.abs() <= 1e-9 {
                            let _ = write!(
                                out,
                                r#"<rect x="{:.6}" y="{:.6}" width="{:.6}" height="{:.6}" rx="{:.6}" ry="{:.6}" fill="{}"{} />"#,
                                fmt(bounds.min.x),
                                fmt(bounds.min.y),
                                fmt(bounds.width()),
                                fmt(bounds.height()),
                                fmt(element.style.roundness.max(0.0)),
                                fmt(element.style.roundness.max(0.0)),
                                fill_color,
                                fill_opacity
                            );
                        } else {
                            let _ = write!(
                                out,
                                r#"<polygon points="{}" fill="{}"{} />"#,
                                points_attr(&points),
                                fill_color,
                                fill_opacity
                            );
                        }
                    }
                    FillStyle::Hatch | FillStyle::CrossHatch => {
                        let clip = *clip_id;
                        *clip_id += 1;
                        let cross = element.style.fill_style == FillStyle::CrossHatch;
                        let lines = hatch_segments(&points, HATCH_SPACING, cross);
                        let _ = write!(
                            out,
                            r#"<defs><clipPath id="clip{clip}"><polygon points="{}" /></clipPath></defs>"#,
                            points_attr(&points)
                        );
                        let _ = write!(out, r#"<g clip-path="url(#clip{clip})">"#);
                        for (start, end) in lines {
                            let _ = write!(
                                out,
                                r#"<line x1="{:.6}" y1="{:.6}" x2="{:.6}" y2="{:.6}" stroke="{}" stroke-width="1"{} />"#,
                                fmt(start.x),
                                fmt(start.y),
                                fmt(end.x),
                                fmt(end.y),
                                fill_color,
                                fill_opacity
                            );
                        }
                        out.push_str("</g>");
                    }
                }
            }
            if stroke_width > 0.0 {
                let dash = dash_array(element.style.stroke_style, stroke_width);
                let _ = write!(
                    out,
                    r#"<polygon points="{}" fill="none" stroke="{}" stroke-width="{}"{}{} />"#,
                    points_attr(&points),
                    stroke_color,
                    fmt(stroke_width),
                    stroke_opacity,
                    dash
                );
            }
            svg.push_str(&out);
        }
        ElementKind::Ellipse => {
            let bounds = element.bounds();
            let local_points = ellipse_local_points(bounds);
            let points = rotated_points(element, &local_points, rough, amplitude, true);
            let (fill_color, fill_opacity) = color_attr(element.style.fill, opacity);
            let (stroke_color, stroke_opacity) = color_attr(element.style.stroke, opacity);
            let mut out = String::new();
            if element.style.fill.a > 0 || element.style.fill_style != FillStyle::Solid {
                match element.style.fill_style {
                    FillStyle::Solid => {
                        if element.angle.abs() <= 1e-9 {
                            let _ = write!(
                                out,
                                r#"<ellipse cx="{:.6}" cy="{:.6}" rx="{:.6}" ry="{:.6}" fill="{}"{} />"#,
                                fmt(bounds.center().x),
                                fmt(bounds.center().y),
                                fmt(bounds.width() / 2.0),
                                fmt(bounds.height() / 2.0),
                                fill_color,
                                fill_opacity
                            );
                        } else {
                            let _ = write!(
                                out,
                                r#"<polygon points="{}" fill="{}"{} />"#,
                                points_attr(&points),
                                fill_color,
                                fill_opacity
                            );
                        }
                    }
                    FillStyle::Hatch | FillStyle::CrossHatch => {
                        let clip = *clip_id;
                        *clip_id += 1;
                        let cross = element.style.fill_style == FillStyle::CrossHatch;
                        let lines = hatch_segments(&points, HATCH_SPACING, cross);
                        let _ = write!(
                            out,
                            r#"<defs><clipPath id="clip{clip}"><polygon points="{}" /></clipPath></defs>"#,
                            points_attr(&points)
                        );
                        let _ = write!(out, r#"<g clip-path="url(#clip{clip})">"#);
                        for (start, end) in lines {
                            let _ = write!(
                                out,
                                r#"<line x1="{:.6}" y1="{:.6}" x2="{:.6}" y2="{:.6}" stroke="{}" stroke-width="1"{} />"#,
                                fmt(start.x),
                                fmt(start.y),
                                fmt(end.x),
                                fmt(end.y),
                                fill_color,
                                fill_opacity
                            );
                        }
                        out.push_str("</g>");
                    }
                }
            }
            if stroke_width > 0.0 {
                let dash = dash_array(element.style.stroke_style, stroke_width);
                if element.angle.abs() <= 1e-9 {
                    let _ = write!(
                        out,
                        r#"<ellipse cx="{:.6}" cy="{:.6}" rx="{:.6}" ry="{:.6}" fill="none" stroke="{}" stroke-width="{}"{}{} />"#,
                        fmt(bounds.center().x),
                        fmt(bounds.center().y),
                        fmt(bounds.width() / 2.0),
                        fmt(bounds.height() / 2.0),
                        stroke_color,
                        fmt(stroke_width),
                        stroke_opacity,
                        dash
                    );
                } else {
                    let _ = write!(
                        out,
                        r#"<polygon points="{}" fill="none" stroke="{}" stroke-width="{}"{}{} />"#,
                        points_attr(&points),
                        stroke_color,
                        fmt(stroke_width),
                        stroke_opacity,
                        dash
                    );
                }
            }
            svg.push_str(&out);
        }
        ElementKind::Diamond => {
            let bounds = element.bounds();
            let local_points = bounds.diamond_points();
            let points = rotated_points(element, &local_points, rough, amplitude, true);
            let (fill_color, fill_opacity) = color_attr(element.style.fill, opacity);
            let (stroke_color, stroke_opacity) = color_attr(element.style.stroke, opacity);
            let mut out = String::new();
            if element.style.fill.a > 0 || element.style.fill_style != FillStyle::Solid {
                match element.style.fill_style {
                    FillStyle::Solid => {
                        let _ = write!(
                            out,
                            r#"<polygon points="{}" fill="{}"{} />"#,
                            points_attr(&points),
                            fill_color,
                            fill_opacity
                        );
                    }
                    FillStyle::Hatch | FillStyle::CrossHatch => {
                        let clip = *clip_id;
                        *clip_id += 1;
                        let cross = element.style.fill_style == FillStyle::CrossHatch;
                        let lines = hatch_segments(&points, HATCH_SPACING, cross);
                        let _ = write!(
                            out,
                            r#"<defs><clipPath id="clip{clip}"><polygon points="{}" /></clipPath></defs>"#,
                            points_attr(&points)
                        );
                        let _ = write!(out, r#"<g clip-path="url(#clip{clip})">"#);
                        for (start, end) in lines {
                            let _ = write!(
                                out,
                                r#"<line x1="{:.6}" y1="{:.6}" x2="{:.6}" y2="{:.6}" stroke="{}" stroke-width="1"{} />"#,
                                fmt(start.x),
                                fmt(start.y),
                                fmt(end.x),
                                fmt(end.y),
                                fill_color,
                                fill_opacity
                            );
                        }
                        out.push_str("</g>");
                    }
                }
            }
            if stroke_width > 0.0 {
                let dash = dash_array(element.style.stroke_style, stroke_width);
                let _ = write!(
                    out,
                    r#"<polygon points="{}" fill="none" stroke="{}" stroke-width="{}"{}{} />"#,
                    points_attr(&points),
                    stroke_color,
                    fmt(stroke_width),
                    stroke_opacity,
                    dash
                );
            }
            svg.push_str(&out);
        }
        ElementKind::Line | ElementKind::Arrow | ElementKind::Freehand => {
            if element.points.len() < 2 {
                return;
            }
            let points = rotated_points(element, &element.points, rough, amplitude, false);
            let (stroke_color, stroke_opacity) = color_attr(element.style.stroke, opacity);
            let dash = dash_array(element.style.stroke_style, stroke_width);
            let _ = write!(
                svg,
                r#"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}"{}{} />"#,
                points_attr(&points),
                stroke_color,
                fmt(stroke_width),
                stroke_opacity,
                dash
            );
            if element.kind == ElementKind::Arrow {
                let center = element.bounds().center();
                let start = rotate_point(element.points[0], center, element.angle);
                let end = rotate_point(element.points[1], center, element.angle);
                if let Some(head) = arrowhead_points(
                    start,
                    end,
                    ARROW_HEAD_LENGTH_PIXELS / scale,
                    ARROW_HEAD_HALF_WIDTH_PIXELS / scale,
                ) {
                    let _ = write!(
                        svg,
                        r#"<polygon points="{}" fill="{}"{} />"#,
                        points_attr(&head),
                        stroke_color,
                        stroke_opacity
                    );
                }
            }
        }
        ElementKind::Text => {
            if element.text.is_empty() {
                return;
            }
            let (fill_color, fill_opacity) = color_attr(element.style.stroke, opacity);
            let lines: Vec<&str> = element.text.split('\n').collect();
            let line_height = element.font_size * 1.25;
            let x = match element.text_align {
                TextAlign::Left => element.x,
                TextAlign::Center => element.x + element.width / 2.0,
                TextAlign::Right => element.x + element.width,
            };
            let anchor = match element.text_align {
                TextAlign::Left => "start",
                TextAlign::Center => "middle",
                TextAlign::Right => "end",
            };
            let mut group = String::new();
            if element.angle.abs() > 1e-9 {
                let _ = write!(
                    group,
                    r#"<g transform="rotate({:.6} {} {})">"#,
                    element.angle.to_degrees(),
                    fmt(center.x),
                    fmt(center.y)
                );
            }
            let _ = write!(
                group,
                r#"<text x="{:.6}" y="{:.6}" font-size="{:.6}" fill="{}"{} text-anchor="{}" font-family="sans-serif">"#,
                fmt(x),
                fmt(element.y + element.font_size * 0.8),
                fmt(element.font_size),
                fill_color,
                fill_opacity,
                anchor
            );
            for (index, line) in lines.iter().enumerate() {
                if index == 0 {
                    let _ = write!(
                        group,
                        r#"<tspan x="{:.6}" dy="0">{}</tspan>"#,
                        fmt(x),
                        escape_text(line)
                    );
                } else {
                    let _ = write!(
                        group,
                        r#"<tspan x="{:.6}" dy="{:.6}">{}</tspan>"#,
                        fmt(x),
                        fmt(line_height),
                        escape_text(line)
                    );
                }
            }
            group.push_str("</text>");
            if element.angle.abs() > 1e-9 {
                group.push_str("</g>");
            }
            svg.push_str(&group);
        }
    }
}

fn ellipse_local_points(bounds: Rect) -> Vec<Point> {
    let center = bounds.center();
    let radius = bounds.size() * 0.5;
    (0..64)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / 64.0;
            Point::new(
                center.x + angle.cos() * radius.x,
                center.y + angle.sin() * radius.y,
            )
        })
        .collect()
}

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
        let count = (((max - min) / spacing).ceil() as usize).min(4_000);
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

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use epsilon_core::document::{Element, ElementKind};

    fn rectangle_document() -> Document {
        let mut document = Document::new();
        let mut element = Element::new(ElementKind::Rectangle, 10.0, 20.0, 30.0, 40.0);
        element.style.stroke_width = 2.0;
        element.style.fill = Color::rgb(100, 150, 200);
        document.add(element);
        document
    }

    #[test]
    fn svg_is_well_formed_and_cropped() {
        let document = rectangle_document();
        let svg = export_svg(&document, &ExportOptions::default()).expect("svg");
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>\n"));
        assert!(svg.contains("<rect") || svg.contains("<polygon"));
        // Half the 2-unit stroke plus one pixel of jitter margin on each side.
        assert!(
            svg.contains(r#"viewBox="8.000000 18.000000 34.000000 44.000000""#),
            "unexpected viewBox in {svg}"
        );
        assert!(svg.contains(r#"width="34.000" height="44.000""#));
    }

    #[test]
    fn svg_rejects_empty_documents() {
        let error = export_svg(&Document::new(), &ExportOptions::default()).unwrap_err();
        assert!(error.contains("nothing"));
    }

    #[test]
    fn svg_keeps_paint_order_and_is_deterministic() {
        let mut document = Document::new();
        let mut first = Element::new(ElementKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
        first.style.fill = Color::rgb(255, 0, 0);
        document.add(first);
        let mut second = Element::new(ElementKind::Rectangle, 20.0, 0.0, 10.0, 10.0);
        second.style.fill = Color::rgb(0, 0, 255);
        document.add(second);

        let first_svg = export_svg(&document, &ExportOptions::default()).expect("svg");
        let second_svg = export_svg(&document, &ExportOptions::default()).expect("svg");
        assert_eq!(first_svg, second_svg);
        let red = first_svg.find("#ff0000").expect("red fill");
        let blue = first_svg.find("#0000ff").expect("blue fill");
        assert!(red < blue, "paint order should follow z-order");
    }

    #[test]
    fn svg_contains_every_element_kind() {
        let mut document = Document::new();
        document.add(Element::new(ElementKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
        document.add(Element::new(ElementKind::Ellipse, 20.0, 0.0, 10.0, 10.0));
        document.add(Element::new(ElementKind::Diamond, 40.0, 0.0, 10.0, 10.0));
        document.add(Element::arrow(
            Point::new(60.0, 0.0),
            Point::new(80.0, 10.0),
        ));
        let mut text = Element::text(90.0, 0.0, 40.0, 12.0);
        text.text = "hi".to_owned();
        document.add(text);
        let svg = export_svg(&document, &ExportOptions::default()).expect("svg");
        assert!(svg.contains("<rect") || svg.contains("<polygon"));
        assert!(svg.contains("<ellipse") || svg.contains("<polygon"));
        assert!(svg.contains("<polygon"));
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("<text"));
    }
}
