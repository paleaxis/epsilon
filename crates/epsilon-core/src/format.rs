//! Native `.epsilon` document format, version 1.
//!
//! The file is a JSON envelope holding a schema version, the document, and
//! per-document application state (currently the viewport). Unknown fields at
//! the envelope, document and app-state levels are kept in [`Extras`] maps so
//! that saving a file written by a newer compatible build does not silently
//! discard data.
//!
//! Versions are rejected before the envelope is deserialized, so a future file
//! produces a clear [`FileError::UnsupportedVersion`] instead of a confusing
//! missing-field error.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::document::{Document, Extras, SCHEMA_VERSION as DOCUMENT_SCHEMA_VERSION};
use crate::viewport::Viewport;

/// Marker stored in the envelope's `format` field.
pub const FILE_FORMAT: &str = "epsilon";
/// Schema version written by this build.
pub const FILE_SCHEMA_VERSION: u32 = 1;
/// File extension, without the leading dot.
pub const FILE_EXTENSION: &str = "epsilon";

/// Per-document application state. Unknown fields are preserved for
/// forward-compatible round-trips.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AppState {
    /// Camera position and zoom when the document was saved.
    #[serde(default)]
    pub viewport: Viewport,
    #[serde(default, flatten)]
    pub extras: Extras,
}

/// A complete native document file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpsilonFile {
    pub format: String,
    pub schema_version: u32,
    pub document: Document,
    #[serde(default)]
    pub app_state: AppState,
    #[serde(default, flatten)]
    pub extras: Extras,
}

impl EpsilonFile {
    /// Wraps a document in a fresh v1 envelope with default app state.
    pub fn new(document: Document) -> Self {
        Self {
            format: FILE_FORMAT.to_owned(),
            schema_version: FILE_SCHEMA_VERSION,
            document,
            app_state: AppState::default(),
            extras: BTreeMap::new(),
        }
    }

    /// Serializes the envelope as human-readable JSON.
    pub fn to_json(&self) -> Result<String, FileError> {
        serde_json::to_string_pretty(self).map_err(FileError::Json)
    }

    /// Parses and validates a native document file.
    pub fn from_json(text: &str) -> Result<Self, FileError> {
        let value: Value = serde_json::from_str(text).map_err(FileError::Json)?;
        let object = value.as_object().ok_or(FileError::ExpectedObject)?;

        let format = object
            .get("format")
            .and_then(Value::as_str)
            .ok_or(FileError::MissingFormat)?;
        if format != FILE_FORMAT {
            return Err(FileError::UnsupportedFormat {
                found: format.to_owned(),
            });
        }

        let version = object
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or(FileError::MissingVersion)?;
        if version != u64::from(FILE_SCHEMA_VERSION) {
            return Err(FileError::UnsupportedVersion {
                found: version,
                supported: FILE_SCHEMA_VERSION,
            });
        }

        let file: Self = serde_json::from_value(value).map_err(FileError::Json)?;
        if file.document.schema_version != DOCUMENT_SCHEMA_VERSION {
            return Err(FileError::UnsupportedDocumentVersion {
                found: file.document.schema_version,
                supported: DOCUMENT_SCHEMA_VERSION,
            });
        }
        Ok(file)
    }
}

/// Why a native document file could not be read or written.
#[derive(Debug)]
pub enum FileError {
    Json(serde_json::Error),
    /// The top-level JSON value was not an object.
    ExpectedObject,
    /// The envelope has no string `format` field.
    MissingFormat,
    /// The envelope has no numeric `schema_version` field.
    MissingVersion,
    /// The envelope belongs to another application.
    UnsupportedFormat {
        found: String,
    },
    /// The envelope version is not the one this build writes and reads.
    UnsupportedVersion {
        found: u64,
        supported: u32,
    },
    /// The document schema itself is not supported.
    UnsupportedDocumentVersion {
        found: u32,
        supported: u32,
    },
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "invalid document JSON: {error}"),
            Self::ExpectedObject => write!(f, "not an Epsilon document: expected a JSON object"),
            Self::MissingFormat => write!(f, "not an Epsilon document: missing string `format`"),
            Self::MissingVersion => {
                write!(
                    f,
                    "not an Epsilon document: missing integer `schema_version`"
                )
            }
            Self::UnsupportedFormat { found } => write!(
                f,
                "not an Epsilon document: format `{found}` is not `{FILE_FORMAT}`"
            ),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "unsupported document version {found}: this build supports version {supported}"
            ),
            Self::UnsupportedDocumentVersion { found, supported } => write!(
                f,
                "unsupported document schema version {found}: this build supports version {supported}"
            ),
        }
    }
}

impl std::error::Error for FileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Element, ElementKind, GroupId};
    use crate::geometry::Point;
    use serde_json::json;

    fn sample_file() -> EpsilonFile {
        let mut document = Document::new();
        document.canvas.grid_visible = true;
        let first = document.add(Element::new(
            ElementKind::Rectangle,
            10.0,
            20.0,
            100.0,
            50.0,
        ));
        let second = document.add(Element::new(ElementKind::Ellipse, 40.0, 60.0, 30.0, 20.0));
        if let Some(first) = document
            .elements
            .iter_mut()
            .find(|element| element.id == first)
        {
            first.group_ids = vec![GroupId::new("outer"), GroupId::new("inner")];
        }
        if let Some(second) = document
            .elements
            .iter_mut()
            .find(|element| element.id == second)
        {
            second.group_ids = vec![GroupId::new("outer"), GroupId::new("inner")];
        }

        let mut file = EpsilonFile::new(document);
        file.app_state.viewport.offset = Point::new(12.5, -8.0);
        file.app_state.viewport.zoom = 2.5;
        file.app_state
            .extras
            .insert("future_ui".to_owned(), json!({"panel": "notes"}));
        file.extras
            .insert("future_top".to_owned(), json!(["kept", 1]));
        file
    }

    #[test]
    fn file_round_trips_document_and_app_state() {
        let file = sample_file();
        let json = file.to_json().expect("serialize file");
        let restored = EpsilonFile::from_json(&json).expect("deserialize file");

        assert_eq!(restored, file);
        assert_eq!(restored.app_state.viewport.zoom, 2.5);
        assert_eq!(
            restored.document.elements[0].group_ids,
            vec![GroupId::new("outer"), GroupId::new("inner")]
        );
    }

    #[test]
    fn unknown_fields_are_preserved_at_every_envelope_level() {
        let mut document = serde_json::to_value(Document::new()).expect("serialize document");
        document["future_document"] = json!({"kept": true});

        let raw = json!({
            "format": FILE_FORMAT,
            "schema_version": FILE_SCHEMA_VERSION,
            "document": document,
            "app_state": {
                "viewport": { "offset": { "x": 1.0, "y": 2.0 }, "zoom": 1.5 },
                "future_app_state": 42
            },
            "future_top": "preserved"
        });

        let file = EpsilonFile::from_json(&raw.to_string()).expect("parse file");
        assert_eq!(file.extras["future_top"], json!("preserved"));
        assert_eq!(file.app_state.extras["future_app_state"], json!(42));
        assert_eq!(
            file.document.extras["future_document"],
            json!({"kept": true})
        );

        let round_tripped = serde_json::to_value(&file).expect("serialize file");
        assert_eq!(round_tripped["future_top"], json!("preserved"));
        assert_eq!(round_tripped["app_state"]["future_app_state"], json!(42));
        assert_eq!(
            round_tripped["document"]["future_document"],
            json!({"kept": true})
        );
    }

    #[test]
    fn future_versions_fail_before_deserialization() {
        let mut value =
            serde_json::to_value(EpsilonFile::new(Document::new())).expect("serialize empty file");
        value["schema_version"] = json!(2);
        value["unknown_from_future"] = json!({"shape": "not understood"});

        let error = EpsilonFile::from_json(&value.to_string()).expect_err("future version");
        assert!(matches!(
            error,
            FileError::UnsupportedVersion {
                found: 2,
                supported: 1
            }
        ));
        assert!(error.to_string().contains("version 2"));
        assert!(error.to_string().contains("version 1"));
    }

    #[test]
    fn malformed_and_foreign_files_report_clear_errors() {
        assert!(matches!(
            EpsilonFile::from_json("{"),
            Err(FileError::Json(_))
        ));
        assert!(matches!(
            EpsilonFile::from_json("[]"),
            Err(FileError::ExpectedObject)
        ));
        assert!(matches!(
            EpsilonFile::from_json(r#"{"schema_version":1}"#),
            Err(FileError::MissingFormat)
        ));
        assert!(matches!(
            EpsilonFile::from_json(r#"{"format":"epsilon"}"#),
            Err(FileError::MissingVersion)
        ));

        let mut value =
            serde_json::to_value(EpsilonFile::new(Document::new())).expect("serialize file");
        value["format"] = json!("other/app");
        let error = EpsilonFile::from_json(&value.to_string()).expect_err("wrong format");
        assert!(matches!(error, FileError::UnsupportedFormat { .. }));
        assert!(error.to_string().contains("other/app"));

        value["format"] = json!(FILE_FORMAT);
        value["document"]["schema_version"] = json!(99);
        assert!(matches!(
            EpsilonFile::from_json(&value.to_string()),
            Err(FileError::UnsupportedDocumentVersion {
                found: 99,
                supported: 1
            })
        ));
    }
}
