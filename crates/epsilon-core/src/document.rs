//! Document envelope, canvas settings and element schema.
//!
//! The document model is intentionally UI-independent. It stores elements in
//! insertion order; paint order is derived from each element's `z_order`, with
//! insertion order breaking ties so equal z-order values remain stable.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::grid;

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
            z_order: 0,
            seed: id.get(),
            version: 1,
        }
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
        document.elements.push(second);

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
