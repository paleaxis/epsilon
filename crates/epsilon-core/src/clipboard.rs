//! Clipboard payloads and paste preparation.
//!
//! The app owns the system clipboard text; this module defines the portable
//! JSON shape and the pure document transformation used when pasting. Pasted
//! elements always receive fresh IDs and paint-order slots, and keep the
//! relative order they had on the clipboard.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::document::{Document, Element, ElementId, GroupId};
use crate::geometry::Point;

/// World-space offset applied to each successive paste.
///
/// The offset is in document units so a paste lands at a predictable place
/// regardless of zoom, and repeated pastes cascade instead of overlapping.
pub const PASTE_OFFSET: f64 = 10.0;

/// Marker written into clipboard JSON so unrelated text is never mistaken for
/// an Epsilon payload.
pub const CLIPBOARD_FORMAT: &str = "epsilon/clipboard";
/// Current clipboard payload version.
pub const CLIPBOARD_FORMAT_VERSION: u32 = 1;

/// Serialized selection exchanged through the system clipboard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub format: String,
    pub version: u32,
    pub elements: Vec<Element>,
}

impl ClipboardPayload {
    /// Builds a payload from elements in document (vector) order.
    pub fn new(elements: Vec<Element>) -> Self {
        Self {
            format: CLIPBOARD_FORMAT.to_owned(),
            version: CLIPBOARD_FORMAT_VERSION,
            elements,
        }
    }

    /// Whether this payload uses the format and version this build understands.
    pub fn is_supported(&self) -> bool {
        self.format == CLIPBOARD_FORMAT
            && self.version == CLIPBOARD_FORMAT_VERSION
            && !self.elements.is_empty()
    }

    /// Serializes this payload for the system clipboard.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parses clipboard text. Callers must still check [`Self::is_supported`]:
    /// arbitrary JSON can parse without being an Epsilon selection.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Returns paste-ready copies of the payload elements.
    ///
    /// Copies are translated by `offset`, sorted into paint order, given fresh
    /// IDs that cannot collide with the current document or reuse an ID from
    /// the payload, and assigned new `z_order` values above every existing
    /// element. Group membership keeps its nesting structure but receives
    /// fresh group IDs, so a pasted group never merges with the original. The
    /// input order is preserved for elements sharing a `z_order`.
    pub fn paste_elements(&self, document: &Document, offset: Point) -> Vec<Element> {
        if !self.is_supported() {
            return Vec::new();
        }

        let mut ordered: Vec<(usize, &Element)> = self.elements.iter().enumerate().collect();
        ordered.sort_by(|(left_index, left), (right_index, right)| {
            left.z_order
                .cmp(&right.z_order)
                .then(left_index.cmp(right_index))
        });

        // Reserve both live document IDs and the clipboard's own IDs so a
        // pasted element never silently reuses the identity it was copied with.
        let mut used: HashSet<ElementId> = document
            .elements
            .iter()
            .map(|element| element.id)
            .chain(self.elements.iter().map(|element| element.id))
            .collect();
        // One fresh group ID per source group keeps nested membership intact
        // without joining the group the copy came from.
        let mut groups: HashMap<GroupId, GroupId> = HashMap::new();
        for element in &self.elements {
            for group in &element.group_ids {
                groups
                    .entry(group.clone())
                    .or_insert_with(GroupId::generate);
            }
        }
        let first_z_order = document.next_z_order();
        ordered
            .into_iter()
            .enumerate()
            .map(|(index, (_, element))| {
                let mut pasted = element.translated(offset);
                let mut id = ElementId::generate();
                while used.contains(&id) {
                    id = ElementId::generate();
                }
                used.insert(id);
                pasted.id = id;
                pasted.z_order = first_z_order.saturating_add(index as i32);
                pasted.group_ids = element
                    .group_ids
                    .iter()
                    .filter_map(|group| groups.get(group).cloned())
                    .collect();
                pasted.version = 1;
                pasted
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ElementKind;

    fn rectangle(id: u64, x: f64, y: f64, z_order: i32) -> Element {
        let mut element =
            Element::with_id(ElementId::new(id), ElementKind::Rectangle, x, y, 10.0, 10.0);
        element.z_order = z_order;
        element
    }

    #[test]
    fn payload_round_trips_through_json() {
        let payload = ClipboardPayload::new(vec![rectangle(2, 10.0, 20.0, 3)]);
        let json = payload.to_json().expect("serialize clipboard");
        let restored = ClipboardPayload::from_json(&json).expect("deserialize clipboard");

        assert_eq!(restored, payload);
        assert!(restored.is_supported());
        assert!(json.contains(CLIPBOARD_FORMAT));
    }

    #[test]
    fn unrelated_json_is_parsed_but_not_supported() {
        let restored =
            ClipboardPayload::from_json(r#"{"format":"other/app","version":1,"elements":[]}"#)
                .expect("valid JSON shape");
        assert!(!restored.is_supported());
        assert!(ClipboardPayload::from_json("plain text").is_err());
    }

    #[test]
    fn paste_regenerates_ids_and_offsets_geometry() {
        let document = Document::new();
        let payload = ClipboardPayload::new(vec![rectangle(2, 10.0, 20.0, 3)]);
        let pasted = payload.paste_elements(&document, Point::new(20.0, 30.0));

        assert_eq!(pasted.len(), 1);
        assert_ne!(pasted[0].id, ElementId::new(2));
        assert!(document.element(pasted[0].id).is_none());
        assert_eq!(pasted[0].x, 30.0);
        assert_eq!(pasted[0].y, 50.0);
        assert_eq!(pasted[0].version, 1);
    }

    #[test]
    fn paste_preserves_relative_order_above_the_document() {
        let mut document = Document::new();
        document.elements.push(rectangle(1, 0.0, 0.0, 5));

        let payload = ClipboardPayload::new(vec![
            rectangle(2, 10.0, 0.0, 7),
            rectangle(3, 20.0, 0.0, 6),
            rectangle(4, 30.0, 0.0, 7),
        ]);
        let pasted = payload.paste_elements(&document, Point::ZERO);

        assert_eq!(pasted.len(), 3);
        // Sorted by paint order: z=6 (x=20), then z=7 in input order (x=10,
        // x=30), then stacked above the existing document element.
        assert_eq!(
            pasted.iter().map(|element| element.x).collect::<Vec<_>>(),
            vec![20.0, 10.0, 30.0]
        );
        assert_eq!(
            pasted
                .iter()
                .map(|element| element.z_order)
                .collect::<Vec<_>>(),
            vec![6, 7, 8]
        );

        let original_ids: HashSet<_> = payload.elements.iter().map(|element| element.id).collect();
        let pasted_ids: HashSet<_> = pasted.iter().map(|element| element.id).collect();
        assert_eq!(pasted_ids.len(), pasted.len());
        assert!(pasted_ids.is_disjoint(&original_ids));
        assert!(pasted_ids.is_disjoint(&document.elements.iter().map(|e| e.id).collect()));
    }

    #[test]
    fn paste_offsets_linear_world_points() {
        let document = Document::new();
        let payload = ClipboardPayload::new(vec![Element::line(
            Point::new(1.0, 2.0),
            Point::new(11.0, 22.0),
        )]);
        let pasted = payload.paste_elements(&document, Point::new(5.0, 6.0));

        assert_eq!(
            pasted[0].points,
            vec![Point::new(6.0, 8.0), Point::new(16.0, 28.0)]
        );
        assert_eq!(pasted[0].x, 6.0);
        assert_eq!(pasted[0].y, 8.0);
    }

    #[test]
    fn paste_preserves_group_nesting_with_fresh_ids() {
        let outer = GroupId::new("outer");
        let inner = GroupId::new("inner");
        let mut first = rectangle(2, 10.0, 20.0, 0);
        first.group_ids = vec![outer.clone(), inner.clone()];
        let mut second = rectangle(3, 20.0, 20.0, 1);
        second.group_ids = vec![outer.clone(), inner.clone()];

        let mut document = Document::new();
        let mut original = rectangle(1, 0.0, 0.0, 0);
        original.group_ids = vec![outer.clone()];
        document.elements.push(original);

        let payload = ClipboardPayload::new(vec![first, second]);
        let pasted = payload.paste_elements(&document, Point::ZERO);

        assert_eq!(pasted.len(), 2);
        assert_eq!(pasted[0].group_ids.len(), 2);
        assert_eq!(pasted[1].group_ids, pasted[0].group_ids);
        assert_ne!(pasted[0].group_ids[0], outer);
        assert_ne!(pasted[0].group_ids[1], inner);

        // Each paste is a separate group, not a merge with the previous copy.
        let again = payload.paste_elements(&document, Point::ZERO);
        assert_ne!(again[0].group_ids[0], pasted[0].group_ids[0]);
    }

    #[test]
    fn unsupported_payloads_are_rejected() {
        let document = Document::new();
        let mut payload = ClipboardPayload::new(vec![rectangle(2, 0.0, 0.0, 0)]);

        payload.format = "something/else".to_owned();
        assert!(!payload.is_supported());
        assert!(payload.paste_elements(&document, Point::ZERO).is_empty());

        payload.format = CLIPBOARD_FORMAT.to_owned();
        payload.version = CLIPBOARD_FORMAT_VERSION + 1;
        assert!(!payload.is_supported());
        assert!(payload.paste_elements(&document, Point::ZERO).is_empty());

        payload.version = CLIPBOARD_FORMAT_VERSION;
        payload.elements.clear();
        assert!(!payload.is_supported());
    }
}
