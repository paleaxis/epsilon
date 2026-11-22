//! Document envelope, canvas settings and element schema.
//!
//! The document model is intentionally UI-independent. It stores elements in
//! insertion order; paint order is derived from each element's `z_order`, with
//! insertion order breaking ties so equal z-order values remain stable.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::geometry::{
    Point, Rect, distance_to_polyline, distance_to_rect, distance_to_segment, point_in_polygon,
    rotate_point,
};
use crate::grid;

fn default_font_size() -> f64 {
    20.0
}

/// Current serialized document schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Stable identifier for an element.
///
/// IDs are generated from a process-wide monotonic counter. Loading a document
/// preserves the IDs stored in it; newly created elements continue above them
/// only within the current process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ElementId(u64);

static NEXT_ELEMENT_ID: AtomicU64 = AtomicU64::new(1);

impl ElementId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// Generates a fresh process-local element ID.
    pub fn generate() -> Self {
        Self(NEXT_ELEMENT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for ElementId {
    fn default() -> Self {
        Self::generate()
    }
}

/// The shape represented by an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    Rectangle,
    Ellipse,
    Diamond,
    Line,
    Arrow,
    Freehand,
    Text,
}

/// Horizontal alignment for text elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// RGBA color stored without depending on a UI toolkit's color type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// Visual styling shared by every element kind.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ElementStyle {
    pub stroke: Color,
    pub fill: Color,
    pub stroke_width: f64,
    /// Element opacity in the range `0.0..=1.0`.
    pub opacity: f64,
}

impl Default for ElementStyle {
    fn default() -> Self {
        Self {
            stroke: Color::BLACK,
            fill: Color::TRANSPARENT,
            stroke_width: 2.0,
            opacity: 1.0,
        }
    }
}

/// Document-level canvas settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasSettings {
    pub background: Color,
    pub grid_size: f64,
    pub grid_visible: bool,
}

impl Default for CanvasSettings {
    fn default() -> Self {
        Self {
            background: Color::WHITE,
            grid_size: grid::DEFAULT_GRID_SIZE,
            grid_visible: false,
        }
    }
}

/// Base fields shared by every element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: ElementId,
    pub kind: ElementKind,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub style: ElementStyle,
    /// Content for text elements; empty for every other kind.
    #[serde(default)]
    pub text: String,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    #[serde(default)]
    pub text_align: TextAlign,
    /// World-space vertices for linear elements (`Line`, `Arrow`, `Freehand`).
    /// Closed shapes leave this empty; `x`/`y`/`width`/`height` are the
    /// derived bounds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<Point>,
    /// Paint order; larger values are drawn later.
    pub z_order: i32,
    /// Deterministic seed for procedural detail such as hand-drawn jitter.
    pub seed: u64,
    /// Element revision, incremented by edits.
    pub version: u64,
}

impl Element {
    /// Creates an element with default style and the next generated ID.
    pub fn new(kind: ElementKind, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::with_id(ElementId::generate(), kind, x, y, width, height)
    }

    /// Creates an element with an explicit ID, useful when importing or testing.
    pub fn with_id(
        id: ElementId,
        kind: ElementKind,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Self {
        Self {
            id,
            kind,
            x,
            y,
            width,
            height,
            angle: 0.0,
            style: ElementStyle::default(),
            text: String::new(),
            font_size: default_font_size(),
            text_align: TextAlign::Left,
            points: Vec::new(),
            z_order: 0,
            seed: id.get(),
            version: 1,
        }
    }

    /// Creates a line whose endpoints are stored in world space. The element's
    /// base bounds are derived from those endpoints.
    pub fn line(start: Point, end: Point) -> Self {
        Self::linear(ElementKind::Line, start, end)
    }

    /// Creates an arrow with world-space endpoints and derived bounds.
    pub fn arrow(start: Point, end: Point) -> Self {
        Self::linear(ElementKind::Arrow, start, end)
    }

    /// Creates an empty text element with a wrapping width and font size.
    pub fn text(x: f64, y: f64, width: f64, font_size: f64) -> Self {
        let font_size = font_size.max(1.0);
        let mut element = Self::new(ElementKind::Text, x, y, width.max(1.0), font_size * 1.25);
        element.font_size = font_size;
        element
    }

    /// Creates a freehand polyline from world-space points, or `None` when
    /// fewer than two points are supplied.
    pub fn freehand(points: Vec<Point>) -> Option<Self> {
        if points.len() < 2 {
            return None;
        }
        let bounds = Rect::from_points(points.iter().copied())?;
        let mut element = Self::new(
            ElementKind::Freehand,
            bounds.min.x,
            bounds.min.y,
            bounds.width(),
            bounds.height(),
        );
        element.points = points;
        Some(element)
    }

    fn linear(kind: ElementKind, start: Point, end: Point) -> Self {
        let bounds = Rect::from_corners(start, end);
        let mut element = Self::new(
            kind,
            bounds.min.x,
            bounds.min.y,
            bounds.width(),
            bounds.height(),
        );
        element.points = vec![start, end];
        element
    }

    /// Unrotated bounds in the element's local frame.
    pub fn bounds(&self) -> Rect {
        Rect::from_xywh(self.x, self.y, self.width, self.height)
    }

    /// Rotates a world point into this element's unrotated local frame.
    pub fn local_point(&self, point: Point) -> Point {
        rotate_point(point, self.bounds().center(), -self.angle)
    }

    /// Axis-aligned world bounds after applying the element rotation.
    pub fn world_bounds(&self) -> Rect {
        let center = self.bounds().center();
        if !self.points.is_empty() {
            return Rect::from_points(
                self.points
                    .iter()
                    .map(|point| rotate_point(*point, center, self.angle)),
            )
            .unwrap_or_else(|| self.bounds());
        }

        let bounds = self.bounds();
        Rect::from_points(
            [
                bounds.min,
                Point::new(bounds.max.x, bounds.min.y),
                bounds.max,
                Point::new(bounds.min.x, bounds.max.y),
            ]
            .into_iter()
            .map(|point| rotate_point(point, center, self.angle)),
        )
        .unwrap_or(bounds)
    }

    /// Returns a translated copy of this element, including its world points.
    pub fn translated(&self, delta: Point) -> Self {
        let mut translated = self.clone();
        translated.x += delta.x;
        translated.y += delta.y;
        for point in &mut translated.points {
            *point += delta;
        }
        translated
    }

    /// Returns a copy scaled from one bounds rectangle into another.
    pub fn scaled_from(&self, from: Rect, to: Rect) -> Self {
        let scale_x = if from.width() > f64::EPSILON {
            to.width() / from.width()
        } else {
            1.0
        };
        let scale_y = if from.height() > f64::EPSILON {
            to.height() / from.height()
        } else {
            1.0
        };

        let mut scaled = self.clone();
        scaled.x = to.min.x + (self.x - from.min.x) * scale_x;
        scaled.y = to.min.y + (self.y - from.min.y) * scale_y;
        scaled.width = self.width * scale_x;
        scaled.height = self.height * scale_y;
        scaled.points = self
            .points
            .iter()
            .map(|point| {
                Point::new(
                    to.min.x + (point.x - from.min.x) * scale_x,
                    to.min.y + (point.y - from.min.y) * scale_y,
                )
            })
            .collect();
        if self.kind == ElementKind::Text {
            scaled.font_size = (self.font_size * scale_y).max(1.0);
        }
        scaled
    }

    /// Hit test in world space, with rotation and a caller-supplied tolerance.
    /// Transparent fills only respond near their stroke; filled shapes respond
    /// anywhere inside.
    pub fn hit_test(&self, point: Point, tolerance: f64) -> bool {
        let local = self.local_point(point);
        let rect = self.bounds();
        let tolerance = tolerance.max(0.0);
        let outline_tolerance = tolerance + self.style.stroke_width * 0.5;
        let filled = self.style.fill.a > 0;

        match self.kind {
            ElementKind::Rectangle => {
                (filled && rect.contains(local))
                    || distance_to_rect(local, rect) <= outline_tolerance
            }
            ElementKind::Ellipse => {
                let rx = self.width * 0.5;
                let ry = self.height * 0.5;
                if rx <= f64::EPSILON || ry <= f64::EPSILON {
                    return false;
                }
                let center = rect.center();
                let normalized = (((local.x - center.x) / rx).powi(2)
                    + ((local.y - center.y) / ry).powi(2))
                .sqrt();
                if filled && normalized <= 1.0 {
                    true
                } else {
                    (normalized - 1.0).abs() * rx.min(ry) <= outline_tolerance
                }
            }
            ElementKind::Diamond => {
                let points = rect.diamond_points();
                (filled && point_in_polygon(local, &points))
                    || distance_to_polyline(local, &points, true) <= outline_tolerance
            }
            ElementKind::Line | ElementKind::Arrow => {
                if let [start, end] = self.points.as_slice() {
                    distance_to_segment(local, *start, *end) <= outline_tolerance
                } else {
                    false
                }
            }
            ElementKind::Freehand => {
                !self.points.is_empty()
                    && distance_to_polyline(local, &self.points, false) <= outline_tolerance
            }
            ElementKind::Text => rect.expand(tolerance).contains(local),
        }
    }

    /// Marquee intersection using the rotated world bounds.
    pub fn intersects_rect(&self, rect: Rect) -> bool {
        self.world_bounds().intersects(&rect)
    }
}

/// A complete document: schema version, canvas settings and ordered elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub schema_version: u32,
    pub canvas: CanvasSettings,
    pub elements: Vec<Element>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            canvas: CanvasSettings::default(),
            elements: Vec::new(),
        }
    }

    /// Appends an element, assigning it the next paint-order slot.
    pub fn add(&mut self, mut element: Element) -> ElementId {
        element.z_order = self.next_z_order();
        let id = element.id;
        self.elements.push(element);
        id
    }

    /// Next free paint-order value.
    pub fn next_z_order(&self) -> i32 {
        self.elements
            .iter()
            .map(|element| element.z_order)
            .max()
            .map_or(0, |max| max.saturating_add(1))
    }

    /// Elements in painter order: ascending `z_order`, ties in insertion order.
    pub fn paint_order(&self) -> Vec<&Element> {
        let mut elements: Vec<(usize, &Element)> = self.elements.iter().enumerate().collect();
        elements.sort_by(|(left_index, left), (right_index, right)| {
            left.z_order
                .cmp(&right.z_order)
                .then(left_index.cmp(right_index))
        });
        elements.into_iter().map(|(_, element)| element).collect()
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }

    pub fn remove(&mut self, id: ElementId) -> Option<Element> {
        let index = self.elements.iter().position(|element| element.id == id)?;
        Some(self.elements.remove(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_document() -> Document {
        let mut document = Document::new();
        document.canvas.grid_visible = true;

        let mut first = Element::with_id(
            ElementId::new(10),
            ElementKind::Rectangle,
            10.0,
            20.0,
            100.0,
            50.0,
        );
        first.z_order = 0;
        first.style.fill = Color::rgb(40, 120, 200);
        first.angle = 15.0;
        document.elements.push(first);

        let mut second = Element::with_id(
            ElementId::new(11),
            ElementKind::Text,
            30.0,
            40.0,
            80.0,
            20.0,
        );
        second.z_order = 2;
        second.style.opacity = 0.75;
        second.text = "Hello\nworld".to_owned();
        second.font_size = 24.0;
        second.text_align = TextAlign::Center;
        document.elements.push(second);

        let mut third = Element::line(Point::new(-5.0, 6.0), Point::new(7.0, -8.0));
        third.z_order = 3;
        document.elements.push(third);

        document
    }

    #[test]
    fn document_round_trips_through_json() {
        let document = sample_document();
        let json = serde_json::to_string(&document).expect("serialize document");
        let restored: Document = serde_json::from_str(&json).expect("deserialize document");

        assert_eq!(restored, document);
        assert_eq!(restored.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn paint_order_uses_z_order_then_insertion_order() {
        let mut document = Document::new();
        let mut back = Element::with_id(
            ElementId::new(1),
            ElementKind::Rectangle,
            0.0,
            0.0,
            1.0,
            1.0,
        );
        back.z_order = 5;
        let mut middle_a =
            Element::with_id(ElementId::new(2), ElementKind::Ellipse, 0.0, 0.0, 1.0, 1.0);
        middle_a.z_order = 1;
        let mut middle_b =
            Element::with_id(ElementId::new(3), ElementKind::Diamond, 0.0, 0.0, 1.0, 1.0);
        middle_b.z_order = 1;
        document.elements.extend([back, middle_a, middle_b]);

        let order: Vec<ElementId> = document
            .paint_order()
            .into_iter()
            .map(|element| element.id)
            .collect();

        assert_eq!(
            order,
            vec![ElementId::new(2), ElementId::new(3), ElementId::new(1)]
        );
    }

    #[test]
    fn line_element_keeps_world_endpoints_and_derives_bounds() {
        let start = Point::new(10.0, 70.0);
        let end = Point::new(4.0, 20.0);
        let element = Element::line(start, end);

        assert_eq!(element.kind, ElementKind::Line);
        assert_eq!(element.points, vec![start, end]);
        assert_eq!(element.x, 4.0);
        assert_eq!(element.y, 20.0);
        assert_eq!(element.width, 6.0);
        assert_eq!(element.height, 50.0);
    }

    #[test]
    fn arrow_element_keeps_world_endpoints_and_derives_bounds() {
        let start = Point::new(8.0, 30.0);
        let end = Point::new(2.0, 10.0);
        let element = Element::arrow(start, end);

        assert_eq!(element.kind, ElementKind::Arrow);
        assert_eq!(element.points, vec![start, end]);
        assert_eq!(element.x, 2.0);
        assert_eq!(element.y, 10.0);
        assert_eq!(element.width, 6.0);
        assert_eq!(element.height, 20.0);
    }

    #[test]
    fn freehand_element_requires_points_and_derives_bounds() {
        assert!(Element::freehand(Vec::new()).is_none());
        assert!(Element::freehand(vec![Point::ZERO]).is_none());

        let points = vec![
            Point::new(3.0, 4.0),
            Point::new(-2.0, 8.0),
            Point::new(6.0, -1.0),
        ];
        let element = Element::freehand(points.clone()).expect("two or more points");
        assert_eq!(element.kind, ElementKind::Freehand);
        assert_eq!(element.points, points);
        assert_eq!(element.x, -2.0);
        assert_eq!(element.y, -1.0);
        assert_eq!(element.width, 8.0);
        assert_eq!(element.height, 9.0);
    }

    #[test]
    fn text_element_sets_content_defaults() {
        let element = Element::text(10.0, 20.0, 160.0, 18.0);
        assert_eq!(element.kind, ElementKind::Text);
        assert_eq!(element.x, 10.0);
        assert_eq!(element.y, 20.0);
        assert_eq!(element.width, 160.0);
        assert_eq!(element.height, 22.5);
        assert_eq!(element.font_size, 18.0);
        assert_eq!(element.text_align, TextAlign::Left);
        assert!(element.text.is_empty());
    }

    #[test]
    fn hit_test_handles_transparent_fills_and_rotation() {
        let mut rectangle = Element::with_id(
            ElementId::new(1),
            ElementKind::Rectangle,
            0.0,
            0.0,
            100.0,
            20.0,
        );
        assert!(!rectangle.hit_test(Point::new(50.0, 10.0), 0.1));
        assert!(rectangle.hit_test(Point::new(0.0, 10.0), 0.1));

        rectangle.style.fill = Color::WHITE;
        assert!(rectangle.hit_test(Point::new(50.0, 10.0), 0.1));

        rectangle.style.fill = Color::WHITE;
        rectangle.angle = std::f64::consts::FRAC_PI_2;
        assert!(rectangle.hit_test(Point::new(50.0, 50.0), 0.1));
        assert!(!rectangle.hit_test(Point::new(10.0, 50.0), 0.1));
        assert!(
            rectangle
                .world_bounds()
                .intersects(&Rect::from_xywh(40.0, 30.0, 20.0, 20.0))
        );
    }

    #[test]
    fn line_and_freehand_hit_tests_use_stroke_distance() {
        let line = Element::line(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        assert!(line.hit_test(Point::new(50.0, 1.0), 0.0));
        assert!(!line.hit_test(Point::new(50.0, 5.0), 0.0));

        let freehand = Element::freehand(vec![
            Point::new(0.0, 0.0),
            Point::new(20.0, 20.0),
            Point::new(40.0, 0.0),
        ])
        .expect("polyline");
        assert!(freehand.hit_test(Point::new(10.0, 10.0), 1.0));
        assert!(!freehand.hit_test(Point::new(10.0, 20.0), 1.0));
    }

    #[test]
    fn diamond_fill_and_text_bounds_are_hit_testable() {
        let mut diamond = Element::with_id(
            ElementId::new(1),
            ElementKind::Diamond,
            0.0,
            0.0,
            100.0,
            50.0,
        );
        assert!(!diamond.hit_test(Point::new(50.0, 25.0), 0.1));
        diamond.style.fill = Color::WHITE;
        assert!(diamond.hit_test(Point::new(50.0, 25.0), 0.1));

        let text = Element::text(0.0, 0.0, 100.0, 20.0);
        assert!(text.hit_test(Point::new(50.0, 10.0), 1.0));
        assert!(!text.hit_test(Point::new(150.0, 10.0), 1.0));
    }

    #[test]
    fn translation_moves_bounds_and_world_points() {
        let line = Element::line(Point::new(0.0, 0.0), Point::new(10.0, 5.0));
        let moved = line.translated(Point::new(7.0, -3.0));
        assert_eq!(moved.x, 7.0);
        assert_eq!(moved.y, -3.0);
        assert_eq!(
            moved.points,
            vec![Point::new(7.0, -3.0), Point::new(17.0, 2.0)]
        );
    }

    #[test]
    fn scaling_maps_geometry_without_drift() {
        let mut text = Element::text(10.0, 20.0, 100.0, 20.0);
        text.text = "Hello".to_owned();
        let from = text.bounds();
        let to = Rect::from_xywh(20.0, 40.0, 200.0, 50.0);
        let scaled = text.scaled_from(from, to);
        assert_eq!(scaled.bounds(), to);
        assert_eq!(scaled.font_size, 40.0);

        let restored = scaled.scaled_from(to, from);
        assert!((restored.x - text.x).abs() < 1e-9);
        assert!((restored.y - text.y).abs() < 1e-9);
        assert!((restored.width - text.width).abs() < 1e-9);
        assert!((restored.height - text.height).abs() < 1e-9);
        assert!((restored.font_size - text.font_size).abs() < 1e-9);
    }

    #[test]
    fn add_assigns_increasing_z_order() {
        let mut document = Document::new();
        let first = document.add(Element::new(ElementKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
        let second = document.add(Element::new(ElementKind::Ellipse, 0.0, 0.0, 10.0, 10.0));

        assert_eq!(document.element(first).unwrap().z_order, 0);
        assert_eq!(document.element(second).unwrap().z_order, 1);
    }

    #[test]
    fn element_ids_are_unique_and_increasing() {
        let first = ElementId::generate();
        let second = ElementId::generate();
        assert!(second > first);
    }
}
