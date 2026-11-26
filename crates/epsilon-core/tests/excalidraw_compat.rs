//! Fixture-based Excalidraw compatibility tests.
//!
//! Fixtures live under `tests/fixtures` and cover supported elements, unknown
//! fields/types, a future envelope version and malformed JSON.

use epsilon_core::document::{Document, ElementKind};
use epsilon_core::excalidraw::{ExcalidrawError, export, import};
use epsilon_core::format::{EpsilonFile, FileError};
use serde_json::{Value, json};

const SCENE: &str = include_str!("fixtures/scene.excalidraw");
const FUTURE: &str = include_str!("fixtures/future.excalidraw");
const MALFORMED: &str = include_str!("fixtures/malformed.excalidraw");

#[test]
fn fixture_imports_supported_subset_and_reports_skipped_types() {
    let imported = import(SCENE).expect("import scene fixture");
    assert_eq!(imported.document.elements.len(), 4);
    assert_eq!(
        imported
            .document
            .elements
            .iter()
            .map(|element| element.kind)
            .collect::<Vec<_>>(),
        vec![
            ElementKind::Rectangle,
            ElementKind::Ellipse,
            ElementKind::Line,
            ElementKind::Text
        ]
    );
    assert_eq!(imported.warnings.len(), 1);
    assert!(imported.warnings[0].contains("embeddable"));
    assert_eq!(imported.viewport.zoom, 1.5);
    assert_eq!(imported.document.canvas.grid_size, 20.0);

    let rectangle = &imported.document.elements[0];
    assert_eq!(rectangle.group_ids.len(), 1);
    assert_eq!(
        rectangle.extras["customData"],
        json!({ "fixture": "rectangle" })
    );
    assert_eq!(imported.document.elements[2].points.len(), 3);
    assert_eq!(imported.document.elements[3].text, "Fixture text");
}

#[test]
fn fixture_round_trip_preserves_known_geometry_and_unknown_fields() {
    let imported = import(SCENE).expect("import fixture");
    let exported = export(&imported.document, &imported.viewport).expect("export fixture");
    let value: Value = serde_json::from_str(&exported).expect("parse export");

    assert_eq!(value["type"], "excalidraw");
    assert_eq!(value["version"], 2);
    assert_eq!(value["futureEnvelopeField"], "preserved");
    assert_eq!(value["appState"]["futureAppState"]["keep"], true);
    assert_eq!(value["files"]["image-1"]["mimeType"], "image/png");
    assert_eq!(value["elements"][0]["id"], "fixture-rectangle");
    assert_eq!(value["elements"][0]["customData"]["fixture"], "rectangle");
    assert_eq!(value["elements"][2]["points"][1][0], 60.0);
    assert_eq!(value["elements"][2]["points"][1][1], 40.0);

    let reimported = import(&exported).expect("re-import fixture export");
    assert_eq!(reimported.document.elements.len(), 4);
    assert!(reimported.warnings.is_empty());
    assert_eq!(reimported.document.elements[1].kind, ElementKind::Ellipse);
}

#[test]
fn future_excalidraw_versions_import_additive_known_content() {
    let imported = import(FUTURE).expect("import future fixture");
    assert_eq!(imported.document.elements.len(), 1);
    assert_eq!(imported.warnings.len(), 1);
    assert!(imported.warnings[0].contains("future-shape"));
    assert_eq!(
        imported.document.elements[0].extras["futureElementField"],
        "preserved"
    );

    let exported = export(&imported.document, &imported.viewport).expect("export future fixture");
    let value: Value = serde_json::from_str(&exported).expect("parse export");
    assert_eq!(value["futureTopLevel"], json!([1, 2, 3]));
    assert_eq!(value["elements"][0]["futureElementField"], "preserved");
}

#[test]
fn malformed_fixture_is_rejected_with_a_json_error() {
    assert!(matches!(import(MALFORMED), Err(ExcalidrawError::Json(_))));
    assert!(matches!(import(""), Err(ExcalidrawError::Json(_))));
}

#[test]
fn native_format_still_rejects_future_versions() {
    let mut value =
        serde_json::to_value(EpsilonFile::new(Document::new())).expect("serialize native file");
    value["schema_version"] = json!(99);
    let error = EpsilonFile::from_json(&value.to_string()).expect_err("future native version");
    assert!(matches!(
        error,
        FileError::UnsupportedVersion {
            found: 99,
            supported: 1
        }
    ));
}
