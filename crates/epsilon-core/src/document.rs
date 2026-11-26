//! Document envelope, canvas settings and element schema.
//!
//! The document model is intentionally UI-independent. It stores elements in
//! insertion order; paint order is derived from each element's `z_order`, with
//! insertion order breaking ties so equal z-order values remain stable.

use std::collections::HashSet;
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

/// Stable identifier for a group.
///
/// Elements store their membership as a stack, outermost group first. Group
/// IDs are strings because Excalidraw's `groupIds` are strings; the generated
/// values only need to be unique within a process.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId(String);

static NEXT_GROUP_ID: AtomicU64 = AtomicU64::new(1);

impl GroupId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Generates a fresh process-local group ID.
    pub fn generate() -> Self {
        Self(format!(
            "g{}",
            NEXT_GROUP_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }
}

impl Default for GroupId {
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

/// Stroke pattern for outlines and linear elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// Fill pattern for closed shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillStyle {
    #[default]
    Solid,
    Hatch,
    CrossHatch,
}

/// Visual styling shared by every element kind.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ElementStyle {
    pub stroke: Color,
    pub fill: Color,
    pub stroke_width: f64,
    /// Element opacity in the range `0.0..=1.0`.
    pub opacity: f64,
    #[serde(default)]
    pub stroke_style: StrokeStyle,
    #[serde(default)]
    pub fill_style: FillStyle,
    /// Corner rounding for rectangles, in world units.
    #[serde(default)]
    pub roundness: f64,
}

impl Default for ElementStyle {
    fn default() -> Self {
        Self {
            stroke: Color::BLACK,
            fill: Color::TRANSPARENT,
            stroke_width: 2.0,
            opacity: 1.0,
            stroke_style: StrokeStyle::Solid,
            fill_style: FillStyle::Solid,
            roundness: 0.0,
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
    /// Group membership from outermost to innermost. Empty for ungrouped
    /// elements; elements sharing the first ID form one selectable group.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_ids: Vec<GroupId>,
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
            group_ids: Vec::new(),
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

    /// The outermost group this element belongs to. Clicking any member selects
    /// every element sharing this ID; inner groups stay nested until the outer
    /// group is ungrouped.
    pub fn outer_group(&self) -> Option<&GroupId> {
        self.group_ids.first()
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

    /// Returns a copy rotated around `pivot` by `delta` radians.
    ///
    /// Closed shapes keep their local bounds and accumulate `angle`; linear
    /// and freehand points are rotated directly so their world-space storage
    /// stays authoritative.
    pub fn rotated_around(&self, pivot: Point, delta: f64) -> Self {
        let mut rotated = self.clone();
        if self.points.is_empty() {
            let center = rotate_point(self.bounds().center(), pivot, delta);
            rotated.x = center.x - self.width * 0.5;
            rotated.y = center.y - self.height * 0.5;
            rotated.angle += delta;
            return rotated;
        }

        let old_center = self.bounds().center();
        rotated.points = self
            .points
            .iter()
            .map(|point| rotate_point(rotate_point(*point, old_center, self.angle), pivot, delta))
            .collect();
        if let Some(bounds) = Rect::from_points(rotated.points.iter().copied()) {
            rotated.x = bounds.min.x;
            rotated.y = bounds.min.y;
            rotated.width = bounds.width();
            rotated.height = bounds.height();
        }
        rotated.angle = 0.0;
        rotated
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

/// A paint-order movement for a selection. Front is drawn last; Back first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZOrderMove {
    Front,
    Forward,
    Backward,
    Back,
}

/// A collective alignment edge for a multi-element selection. All elements
/// are translated onto the same world-space edge or centre line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
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

    /// Computes the before/after snapshots for moving `ids` in paint order.
    ///
    /// `Front`/`Back` move the whole selection to the end/start while keeping
    /// its relative order. `Forward`/`Backward` move each selected element one
    /// slot in that direction, so a contiguous block moves as a group and
    /// unselected neighbours swap past it. The result is empty when nothing
    /// would change. Paint-order indices are written back as dense `z_order`
    /// values, so only elements whose slot actually moved are returned.
    pub fn z_order_changes(
        &self,
        ids: &[ElementId],
        movement: ZOrderMove,
    ) -> Vec<(Element, Element)> {
        let current: Vec<ElementId> = self
            .paint_order()
            .into_iter()
            .map(|element| element.id)
            .collect();
        if current.is_empty() || !current.iter().any(|id| ids.contains(id)) {
            return Vec::new();
        }

        let mut next = current.clone();
        match movement {
            ZOrderMove::Front => {
                // Stable sort keeps the selection's relative paint order.
                next.sort_by_key(|id| ids.contains(id));
            }
            ZOrderMove::Back => {
                next.sort_by_key(|id| !ids.contains(id));
            }
            ZOrderMove::Forward => {
                for index in (0..next.len().saturating_sub(1)).rev() {
                    if ids.contains(&next[index]) && !ids.contains(&next[index + 1]) {
                        next.swap(index, index + 1);
                    }
                }
            }
            ZOrderMove::Backward => {
                for index in 1..next.len() {
                    if ids.contains(&next[index]) && !ids.contains(&next[index - 1]) {
                        next.swap(index - 1, index);
                    }
                }
            }
        }
        if next == current {
            return Vec::new();
        }

        next.iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let element = self.element(*id)?;
                let z_order = i32::try_from(index).ok()?;
                (element.z_order != z_order).then(|| {
                    let mut after = element.clone();
                    after.z_order = z_order;
                    (element.clone(), after)
                })
            })
            .collect()
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }

    /// Every element in the outermost group containing `id`, in document
    /// order. An ungrouped element expands to itself; unknown IDs expand to
    /// nothing.
    pub fn group_members(&self, id: ElementId) -> Vec<ElementId> {
        match self.element(id).and_then(Element::outer_group) {
            Some(group) => self
                .elements
                .iter()
                .filter(|element| element.group_ids.contains(group))
                .map(|element| element.id)
                .collect(),
            None => self
                .element(id)
                .map(|element| element.id)
                .into_iter()
                .collect(),
        }
    }

    /// Expands a selection to whole outermost groups, in document order and
    /// without duplicates. This is the selection unit for clicks, marquees and
    /// context menus, so a group can never be partially selected.
    pub fn expand_selection(&self, ids: &[ElementId]) -> Vec<ElementId> {
        let selected: HashSet<ElementId> = ids.iter().copied().collect();
        let groups: HashSet<&GroupId> = self
            .elements
            .iter()
            .filter(|element| selected.contains(&element.id))
            .filter_map(Element::outer_group)
            .collect();
        self.elements
            .iter()
            .filter(|element| {
                selected.contains(&element.id)
                    || element
                        .outer_group()
                        .is_some_and(|group| groups.contains(group))
            })
            .map(|element| element.id)
            .collect()
    }

    /// Before/after snapshots that put the selection into a new outermost
    /// group. The selection is expanded to whole groups first, so grouping a
    /// subset of an existing group cannot split it. Fewer than two involved
    /// elements produce no changes.
    pub fn group_changes(&self, ids: &[ElementId]) -> Vec<(Element, Element)> {
        let ids = self.expand_selection(ids);
        if ids.len() < 2 {
            return Vec::new();
        }
        let group = GroupId::generate();
        ids.iter()
            .filter_map(|id| self.element(*id))
            .map(|element| {
                let mut after = element.clone();
                after.group_ids.insert(0, group.clone());
                (element.clone(), after)
            })
            .collect()
    }

    /// Before/after snapshots that remove one grouping level from the
    /// selection. Each selected outermost group is ungrouped as a unit, so a
    /// partially selected group cannot be left behind; inner group IDs remain
    /// and expose the next nesting level. Returns empty when nothing is
    /// grouped.
    pub fn ungroup_changes(&self, ids: &[ElementId]) -> Vec<(Element, Element)> {
        let ids = self.expand_selection(ids);
        ids.iter()
            .filter_map(|id| self.element(*id))
            .filter(|element| !element.group_ids.is_empty())
            .map(|element| {
                let mut after = element.clone();
                after.group_ids.remove(0);
                (element.clone(), after)
            })
            .collect()
    }

    /// Before/after snapshots that align two or more elements on a shared
    /// world-space edge. Rotated elements are aligned by their axis-aligned
    /// world bounds, so the visible extent lands on the edge. Unknown IDs are
    /// ignored, fewer than two elements and already-aligned selections produce
    /// no changes.
    pub fn align_changes(&self, ids: &[ElementId], align: Align) -> Vec<(Element, Element)> {
        let elements: Vec<&Element> = ids.iter().filter_map(|id| self.element(*id)).collect();
        if elements.len() < 2 {
            return Vec::new();
        }
        let bounds: Vec<Rect> = elements
            .iter()
            .map(|element| element.world_bounds())
            .collect();
        let left = bounds
            .iter()
            .map(|bound| bound.min.x)
            .fold(f64::INFINITY, f64::min);
        let right = bounds
            .iter()
            .map(|bound| bound.max.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let top = bounds
            .iter()
            .map(|bound| bound.min.y)
            .fold(f64::INFINITY, f64::min);
        let bottom = bounds
            .iter()
            .map(|bound| bound.max.y)
            .fold(f64::NEG_INFINITY, f64::max);

        elements
            .into_iter()
            .zip(bounds)
            .filter_map(|(element, bounds)| {
                let delta = match align {
                    Align::Left => Point::new(left - bounds.min.x, 0.0),
                    Align::CenterX => Point::new((left + right) / 2.0 - bounds.center().x, 0.0),
                    Align::Right => Point::new(right - bounds.max.x, 0.0),
                    Align::Top => Point::new(0.0, top - bounds.min.y),
                    Align::CenterY => Point::new(0.0, (top + bottom) / 2.0 - bounds.center().y),
                    Align::Bottom => Point::new(0.0, bottom - bounds.max.y),
                };
                if delta.x.abs() <= f64::EPSILON && delta.y.abs() <= f64::EPSILON {
                    None
                } else {
                    Some((element.clone(), element.translated(delta)))
                }
            })
            .collect()
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
        first.style.stroke_style = StrokeStyle::Dashed;
        first.style.fill_style = FillStyle::CrossHatch;
        first.style.roundness = 8.0;
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

    fn three_stacked_elements() -> Document {
        let mut document = Document::new();
        for (id, z_order) in [(1, 0), (2, 1), (3, 2)] {
            let mut element = Element::with_id(
                ElementId::new(id),
                ElementKind::Rectangle,
                0.0,
                0.0,
                10.0,
                10.0,
            );
            element.z_order = z_order;
            document.elements.push(element);
        }
        document
    }

    fn paint_ids(document: &Document) -> Vec<u64> {
        document
            .paint_order()
            .into_iter()
            .map(|element| element.id.get())
            .collect()
    }

    fn apply_changes(document: &mut Document, changes: &[(Element, Element)]) {
        for (_, after) in changes {
            let current = document
                .elements
                .iter_mut()
                .find(|element| element.id == after.id)
                .expect("changed element exists");
            *current = after.clone();
        }
    }

    #[test]
    fn z_order_front_and_back_move_the_selection_as_a_block() {
        let mut document = three_stacked_elements();
        let changes = document.z_order_changes(&[ElementId::new(1)], ZOrderMove::Front);
        assert_eq!(
            changes.len(),
            3,
            "normalising slots touches each moved element"
        );
        apply_changes(&mut document, &changes);
        assert_eq!(paint_ids(&document), vec![2, 3, 1]);
        assert_eq!(document.element(ElementId::new(1)).unwrap().z_order, 2);

        let changes = document.z_order_changes(&[ElementId::new(3)], ZOrderMove::Back);
        apply_changes(&mut document, &changes);
        assert_eq!(paint_ids(&document), vec![3, 2, 1]);
    }

    #[test]
    fn z_order_forward_and_backward_move_one_slot_and_group_blocks() {
        let mut document = three_stacked_elements();
        let changes = document.z_order_changes(&[ElementId::new(1)], ZOrderMove::Forward);
        apply_changes(&mut document, &changes);
        assert_eq!(paint_ids(&document), vec![2, 1, 3]);

        let changes = document.z_order_changes(&[ElementId::new(1)], ZOrderMove::Forward);
        apply_changes(&mut document, &changes);
        assert_eq!(paint_ids(&document), vec![2, 3, 1]);

        // An element already at the front cannot move further.
        assert!(
            document
                .z_order_changes(&[ElementId::new(1)], ZOrderMove::Forward)
                .is_empty()
        );

        let changes = document.z_order_changes(&[ElementId::new(3)], ZOrderMove::Backward);
        apply_changes(&mut document, &changes);
        assert_eq!(paint_ids(&document), vec![3, 2, 1]);

        // A contiguous selection moves back as a group.
        let mut group = three_stacked_elements();
        let changes = group.z_order_changes(
            &[ElementId::new(2), ElementId::new(3)],
            ZOrderMove::Backward,
        );
        apply_changes(&mut group, &changes);
        assert_eq!(paint_ids(&group), vec![2, 3, 1]);
    }

    #[test]
    fn z_order_changes_ignore_unknown_ids_and_no_ops() {
        let document = three_stacked_elements();
        assert!(
            document
                .z_order_changes(&[ElementId::new(99)], ZOrderMove::Front)
                .is_empty()
        );
        assert!(
            document
                .z_order_changes(&[ElementId::new(1)], ZOrderMove::Backward)
                .is_empty()
        );
        assert!(
            document
                .z_order_changes(&[ElementId::new(3)], ZOrderMove::Forward)
                .is_empty()
        );
        let all = [ElementId::new(1), ElementId::new(2), ElementId::new(3)];
        assert!(document.z_order_changes(&all, ZOrderMove::Front).is_empty());
        assert!(document.z_order_changes(&all, ZOrderMove::Back).is_empty());
    }

    #[test]
    fn group_membership_round_trips_and_defaults_to_empty() {
        let mut grouped = Element::with_id(
            ElementId::new(1),
            ElementKind::Rectangle,
            0.0,
            0.0,
            10.0,
            10.0,
        );
        grouped.group_ids = vec![GroupId::new("outer"), GroupId::new("inner")];
        let json = serde_json::to_string(&grouped).expect("serialize grouped element");
        let restored: Element = serde_json::from_str(&json).expect("deserialize grouped element");
        assert_eq!(restored.group_ids, grouped.group_ids);

        let ungrouped = Element::with_id(
            ElementId::new(2),
            ElementKind::Ellipse,
            0.0,
            0.0,
            10.0,
            10.0,
        );
        let json = serde_json::to_string(&ungrouped).expect("serialize ungrouped element");
        assert!(
            !json.contains("group_ids"),
            "empty membership stays out of the payload"
        );
        let restored: Element = serde_json::from_str(&json).expect("deserialize legacy element");
        assert!(restored.group_ids.is_empty());
    }

    #[test]
    fn grouping_shares_one_outer_id_and_ungrouping_peels_one_level() {
        let mut document = Document::new();
        for id in 1..=4 {
            let mut element = Element::with_id(
                ElementId::new(id),
                ElementKind::Rectangle,
                0.0,
                0.0,
                10.0,
                10.0,
            );
            element.z_order = id as i32;
            document.elements.push(element);
        }
        let order: Vec<ElementId> = document.elements.iter().map(|element| element.id).collect();

        let changes = document.group_changes(&[ElementId::new(1), ElementId::new(2)]);
        assert_eq!(changes.len(), 2);
        apply_changes(&mut document, &changes);
        let first_group = document
            .element(ElementId::new(1))
            .expect("first element")
            .group_ids
            .clone();
        assert_eq!(first_group.len(), 1);
        assert_eq!(
            document
                .element(ElementId::new(2))
                .expect("second element")
                .group_ids,
            first_group
        );
        assert!(
            document
                .element(ElementId::new(3))
                .expect("third element")
                .group_ids
                .is_empty()
        );

        let changes = document.group_changes(&[ElementId::new(3), ElementId::new(4)]);
        apply_changes(&mut document, &changes);

        // Grouping a selection that already contains whole groups nests a new
        // outermost group without disturbing the inner IDs.
        let changes = document.group_changes(&[
            ElementId::new(1),
            ElementId::new(2),
            ElementId::new(3),
            ElementId::new(4),
        ]);
        assert_eq!(changes.len(), 4);
        apply_changes(&mut document, &changes);
        let outer = document
            .element(ElementId::new(1))
            .expect("first element")
            .group_ids
            .clone();
        assert_eq!(outer.len(), 2);
        for id in 1..=4 {
            assert_eq!(
                document
                    .element(ElementId::new(id))
                    .expect("element")
                    .group_ids[0],
                outer[0],
                "all four share the new outer group"
            );
        }
        assert_eq!(
            document
                .expand_selection(&[ElementId::new(1)])
                .into_iter()
                .map(ElementId::get)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );

        // Ungrouping removes exactly one level and never reorders elements.
        let changes = document.ungroup_changes(&[ElementId::new(1)]);
        assert_eq!(changes.len(), 4);
        apply_changes(&mut document, &changes);
        assert_eq!(
            document
                .elements
                .iter()
                .map(|element| element.id)
                .collect::<Vec<_>>(),
            order
        );
        assert_eq!(
            document
                .element(ElementId::new(1))
                .expect("first element")
                .group_ids,
            first_group
        );
        assert_eq!(
            document
                .expand_selection(&[ElementId::new(1)])
                .into_iter()
                .map(ElementId::get)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );

        let changes = document.ungroup_changes(&[ElementId::new(1)]);
        apply_changes(&mut document, &changes);
        assert!(
            document
                .element(ElementId::new(1))
                .expect("first element")
                .group_ids
                .is_empty()
        );
        assert!(
            document
                .element(ElementId::new(3))
                .expect("third element")
                .group_ids
                .len()
                == 1
        );
    }

    #[test]
    fn group_changes_need_two_elements_and_ignore_unknown_ids() {
        let document = three_stacked_elements();
        assert!(document.group_changes(&[]).is_empty());
        assert!(document.group_changes(&[ElementId::new(1)]).is_empty());
        assert!(
            document
                .group_changes(&[ElementId::new(99), ElementId::new(100)])
                .is_empty()
        );
        assert!(document.ungroup_changes(&[ElementId::new(1)]).is_empty());
        assert!(document.expand_selection(&[ElementId::new(99)]).is_empty());
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
    fn element_style_defaults_and_deserializes_legacy_fields() {
        let style = ElementStyle::default();
        assert_eq!(style.stroke_style, StrokeStyle::Solid);
        assert_eq!(style.fill_style, FillStyle::Solid);
        assert_eq!(style.roundness, 0.0);

        let legacy = r#"{
            "stroke": {"r":0,"g":0,"b":0,"a":255},
            "fill": {"r":0,"g":0,"b":0,"a":0},
            "stroke_width": 2.0,
            "opacity": 1.0
        }"#;
        let restored: ElementStyle = serde_json::from_str(legacy).expect("legacy style");
        assert_eq!(restored.stroke_style, StrokeStyle::Solid);
        assert_eq!(restored.fill_style, FillStyle::Solid);
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
    fn rotation_accumulates_shape_angles_and_rotates_points() {
        let rectangle = Element::new(ElementKind::Rectangle, 0.0, 0.0, 100.0, 50.0);
        let rotated = rectangle.rotated_around(Point::new(50.0, 25.0), std::f64::consts::FRAC_PI_2);
        assert!((rotated.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((rotated.x - 0.0).abs() < 1e-9);
        assert!((rotated.y - 0.0).abs() < 1e-9);

        let line = Element::line(Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        let rotated = line.rotated_around(Point::new(0.0, 0.0), std::f64::consts::FRAC_PI_2);
        assert_eq!(rotated.angle, 0.0);
        assert!((rotated.points[1].x - 0.0).abs() < 1e-9);
        assert!((rotated.points[1].y - 10.0).abs() < 1e-9);
        assert!(rotated.height >= 10.0);
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
    fn align_changes_use_world_bounds_for_every_edge() {
        let mut document = Document::new();
        let first = document.add(Element::new(ElementKind::Rectangle, 10.0, 10.0, 20.0, 10.0));
        let second = document.add(Element::new(ElementKind::Rectangle, 50.0, 40.0, 10.0, 30.0));
        let third = document.add(Element::new(ElementKind::Ellipse, 100.0, 70.0, 30.0, 20.0));
        let ids = [first, second, third];

        for (align, edge) in [
            (Align::Left, 10.0),
            (Align::Right, 130.0),
            (Align::Top, 10.0),
            (Align::Bottom, 90.0),
        ] {
            let changes = document.align_changes(&ids, align);
            assert!(!changes.is_empty(), "{align:?} must move something");
            for (_, after) in &changes {
                let bounds = after.world_bounds();
                let actual = match align {
                    Align::Left => bounds.min.x,
                    Align::Right => bounds.max.x,
                    Align::Top => bounds.min.y,
                    Align::Bottom => bounds.max.y,
                    Align::CenterX | Align::CenterY => unreachable!(),
                };
                assert!(
                    (actual - edge).abs() < 1e-9,
                    "{align:?}: {actual} != {edge}"
                );
            }
        }

        let changes = document.align_changes(&ids, Align::CenterX);
        for (_, after) in changes {
            assert!((after.world_bounds().center().x - 70.0).abs() < 1e-9);
        }
        let changes = document.align_changes(&ids, Align::CenterY);
        for (_, after) in changes {
            assert!((after.world_bounds().center().y - 50.0).abs() < 1e-9);
        }
    }

    #[test]
    fn align_changes_account_for_rotation_and_skip_no_ops() {
        let mut document = Document::new();
        let rotated = document.add(Element::new(ElementKind::Rectangle, 0.0, 0.0, 20.0, 10.0));
        if let Some(element) = document.element(rotated).cloned() {
            let mut element = element;
            element.angle = std::f64::consts::FRAC_PI_2;
            document.elements[0] = element;
        }
        let other = document.add(Element::new(ElementKind::Rectangle, 100.0, 0.0, 10.0, 10.0));

        let rotated_min = document
            .element(rotated)
            .map(|element| element.world_bounds().min.x)
            .expect("rotated element");
        assert!((rotated_min - 5.0).abs() < 1e-9);

        let changes = document.align_changes(&[rotated, other], Align::Left);
        assert_eq!(
            changes.len(),
            1,
            "the rotated element is already on the edge"
        );
        assert_eq!(changes[0].1.id, other);
        assert!((changes[0].1.world_bounds().min.x - rotated_min).abs() < 1e-9);

        assert!(document.align_changes(&[rotated], Align::Left).is_empty());
        assert!(
            document
                .align_changes(&[ElementId::new(9999), other], Align::Left)
                .is_empty()
        );
    }

    #[test]
    fn element_ids_are_unique_and_increasing() {
        let first = ElementId::generate();
        let second = ElementId::generate();
        assert!(second > first);
    }
}
