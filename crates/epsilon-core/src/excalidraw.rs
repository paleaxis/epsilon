//! Excalidraw import/export.
//!
//! Excalidraw files are JSON envelopes with an `elements` array and `appState`.
//! Import maps the supported element subset into Epsilon's document model and
//! retains unknown per-element fields in [`Element::extras`] (plus unknown
//! envelope/app-state fields in document extras) so exporting a re-imported
//! file does not discard data. Unknown element types are skipped with warnings
//! instead of failing the whole import.

use serde_json::{Map, Value, json};

use crate::document::{
    Color, Document, Element, ElementId, ElementKind, Extras, FillStyle, GroupId, StrokeStyle,
    TextAlign,
};
use crate::format::{self, FileError};
use crate::geometry::{Point, Rect};
use crate::viewport::{MAX_ZOOM, MIN_ZOOM, Viewport};

pub const EXCALIDRAW_TYPE: &str = "excalidraw";
pub const EXCALIDRAW_VERSION: u64 = 2;
/// Document extras key preserving unknown envelope fields for re-export.
pub const ENVELOPE_EXTRAS_KEY: &str = "excalidraw_envelope";
/// Document extras key preserving unknown appState fields for re-export.
pub const APP_STATE_EXTRAS_KEY: &str = "excalidraw_app_state";
/// Document extras key preserving the envelope's `files` map.
pub const FILES_EXTRAS_KEY: &str = "excalidraw_files";
/// Element extras key preserving the original Excalidraw string ID.
const ELEMENT_ID_KEY: &str = "excalidraw_id";

/// Result of importing an Excalidraw scene.
#[derive(Debug, Clone, PartialEq)]
pub struct ExcalidrawImport {
    pub document: Document,
    pub viewport: Viewport,
    pub warnings: Vec<String>,
}

/// Why an Excalidraw file could not be read or written.
#[derive(Debug)]
pub enum ExcalidrawError {
    Json(serde_json::Error),
    NotObject,
    MissingElements,
    Io(std::io::Error),
    Write(FileError),
}

impl std::fmt::Display for ExcalidrawError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid Excalidraw JSON: {error}"),
            Self::NotObject => formatter.write_str("not an Excalidraw file: expected an object"),
            Self::MissingElements => {
                formatter.write_str("not an Excalidraw file: missing `elements` array")
            }
            Self::Io(error) => write!(formatter, "Excalidraw file error: {error}"),
            Self::Write(error) => write!(formatter, "could not write Excalidraw file: {error}"),
        }
    }
}

impl std::error::Error for ExcalidrawError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Write(error) => Some(error),
            _ => None,
        }
    }
}

const KNOWN_ELEMENT_FIELDS: &[&str] = &[
    "id",
    "type",
    "x",
    "y",
    "width",
    "height",
    "angle",
    "strokeColor",
    "backgroundColor",
    "fillStyle",
    "strokeWidth",
    "strokeStyle",
    "roughness",
    "opacity",
    "seed",
    "version",
    "versionNonce",
    "index",
    "isDeleted",
    "groupIds",
    "roundness",
    "text",
    "fontSize",
    "fontFamily",
    "textAlign",
    "verticalAlign",
    "containerId",
    "lineHeight",
    "autoResize",
    "points",
    "pressures",
    "simulatePressure",
    "lastCommittedPoint",
    "startBinding",
    "endBinding",
    "startArrowhead",
    "endArrowhead",
    "boundElements",
    "updated",
    "created",
    "link",
    "locked",
];

const KNOWN_APP_STATE_FIELDS: &[&str] = &[
    "zoom",
    "scrollX",
    "scrollY",
    "gridSize",
    "viewBackgroundColor",
];

const KNOWN_ENVELOPE_FIELDS: &[&str] =
    &["type", "version", "source", "elements", "appState", "files"];

/// Imports an Excalidraw scene. Unknown element types and malformed individual
/// elements are skipped with warnings; malformed JSON or a missing element
/// array is an error.
pub fn import(json: &str) -> Result<ExcalidrawImport, ExcalidrawError> {
    let value: Value = serde_json::from_str(json).map_err(ExcalidrawError::Json)?;
    let root = value.as_object().ok_or(ExcalidrawError::NotObject)?;
    let elements_value = root
        .get("elements")
        .ok_or(ExcalidrawError::MissingElements)?;
    let elements_array = elements_value
        .as_array()
        .ok_or(ExcalidrawError::MissingElements)?;

    let mut warnings = Vec::new();
    let mut document = Document::new();
    let mut z_order = 0;
    for (index, element_value) in elements_array.iter().enumerate() {
        let Some(object) = element_value.as_object() else {
            warnings.push(format!("element {index}: not an object, skipped"));
            continue;
        };
        if object
            .get("isDeleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(kind) = object
            .get("type")
            .and_then(Value::as_str)
            .and_then(element_kind)
        else {
            let kind = object
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("<missing type>");
            warnings.push(format!(
                "element {index}: unsupported type `{kind}`, skipped"
            ));
            continue;
        };
        let Some(element) = import_element(object, kind, z_order, &mut warnings, index) else {
            continue;
        };
        document.elements.push(element);
        z_order += 1;
    }

    let mut viewport = Viewport::default();
    if let Some(app_state) = root
        .get("appState")
        .or_else(|| root.get("app_state"))
        .and_then(Value::as_object)
    {
        import_app_state(app_state, &mut document, &mut viewport);
        let unknown: Map<String, Value> = app_state
            .iter()
            .filter(|(key, _)| !KNOWN_APP_STATE_FIELDS.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if !unknown.is_empty() {
            document
                .extras
                .insert(APP_STATE_EXTRAS_KEY.to_owned(), Value::Object(unknown));
        }
    }
    let unknown_envelope: Map<String, Value> = root
        .iter()
        .filter(|(key, _)| !KNOWN_ENVELOPE_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if !unknown_envelope.is_empty() {
        document.extras.insert(
            ENVELOPE_EXTRAS_KEY.to_owned(),
            Value::Object(unknown_envelope),
        );
    }
    if let Some(files) = root.get("files").and_then(Value::as_object)
        && !files.is_empty()
    {
        document
            .extras
            .insert(FILES_EXTRAS_KEY.to_owned(), Value::Object(files.clone()));
    }

    Ok(ExcalidrawImport {
        document,
        viewport,
        warnings,
    })
}

fn import_element(
    object: &Map<String, Value>,
    kind: ElementKind,
    z_order: i32,
    warnings: &mut Vec<String>,
    index: usize,
) -> Option<Element> {
    let number = |key: &str| object.get(key).and_then(Value::as_f64);
    let x = number("x").unwrap_or(0.0);
    let y = number("y").unwrap_or(0.0);
    let mut width = number("width").unwrap_or(0.0);
    let mut height = number("height").unwrap_or(0.0);

    let mut element = Element::with_id(ElementId::generate(), kind, x, y, width, height);
    element.angle = number("angle").unwrap_or(0.0);
    element.z_order = z_order;
    element.seed = object.get("seed").and_then(Value::as_u64).unwrap_or(0);
    element.version = object.get("version").and_then(Value::as_u64).unwrap_or(1);

    let style = &mut element.style;
    style.stroke = object
        .get("strokeColor")
        .map(|value| parse_color(value, style.stroke))
        .unwrap_or(style.stroke);
    style.fill = object
        .get("backgroundColor")
        .map(|value| parse_color(value, style.fill))
        .unwrap_or(style.fill);
    style.stroke_width = number("strokeWidth").unwrap_or(style.stroke_width);
    style.opacity = number("opacity")
        .map(|opacity| (opacity / 100.0).clamp(0.0, 1.0))
        .unwrap_or(style.opacity);
    if let Some(fill) = object.get("fillStyle").and_then(Value::as_str) {
        style.fill_style = match fill {
            "hachure" | "zigzag" => FillStyle::Hatch,
            "cross-hatch" => FillStyle::CrossHatch,
            _ => FillStyle::Solid,
        };
    }
    if let Some(stroke) = object.get("strokeStyle").and_then(Value::as_str) {
        style.stroke_style = match stroke {
            "dashed" => StrokeStyle::Dashed,
            "dotted" => StrokeStyle::Dotted,
            _ => StrokeStyle::Solid,
        };
    }
    if let Some(roundness) = object.get("roundness").and_then(Value::as_object) {
        style.roundness = roundness
            .get("type")
            .and_then(Value::as_u64)
            .filter(|kind| *kind != 0)
            .map(|_| style.stroke_width * 4.0)
            .unwrap_or(0.0);
    }

    if matches!(
        kind,
        ElementKind::Line | ElementKind::Arrow | ElementKind::Freehand
    ) {
        let mut points = Vec::new();
        if let Some(array) = object.get("points").and_then(Value::as_array) {
            for point in array {
                if let Some(point) = parse_point(point) {
                    points.push(Point::new(point.x + x, point.y + y));
                }
            }
        }
        if points.len() < 2 {
            warnings.push(format!(
                "element {index}: linear element has fewer than two points, skipped"
            ));
            return None;
        }
        let bounds = Rect::from_points(points.iter().copied()).unwrap_or(element.bounds());
        width = bounds.width();
        height = bounds.height();
        element.x = bounds.min.x;
        element.y = bounds.min.y;
        element.width = width;
        element.height = height;
        element.points = points;
    }

    if let Some(text) = object.get("text").and_then(Value::as_str) {
        element.text = text.to_owned();
    }
    element.font_size = number("fontSize").unwrap_or(element.font_size);
    if let Some(align) = object.get("textAlign").and_then(Value::as_str) {
        element.text_align = match align {
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            _ => TextAlign::Left,
        };
    }
    if let Some(groups) = object.get("groupIds").and_then(Value::as_array) {
        element.group_ids = groups
            .iter()
            .filter_map(Value::as_str)
            .map(|group| GroupId::new(group.to_owned()))
            .collect();
    }

    let extras: Extras = object
        .iter()
        .filter(|(key, _)| !KNOWN_ELEMENT_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    element.extras = extras;
    if let Some(id) = object.get("id").and_then(Value::as_str) {
        element
            .extras
            .insert(ELEMENT_ID_KEY.to_owned(), Value::String(id.to_owned()));
    }
    Some(element)
}

fn import_app_state(
    app_state: &Map<String, Value>,
    document: &mut Document,
    viewport: &mut Viewport,
) {
    let zoom = app_state
        .get("zoom")
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.get("value").and_then(Value::as_f64))
        })
        .filter(|zoom| zoom.is_finite())
        .map(|zoom| zoom.clamp(MIN_ZOOM, MAX_ZOOM));
    if let Some(zoom) = zoom {
        viewport.zoom = zoom;
    }
    let scroll_x = app_state
        .get("scrollX")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let scroll_y = app_state
        .get("scrollY")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    viewport.offset = Point::new(-scroll_x * viewport.zoom, -scroll_y * viewport.zoom);

    if let Some(grid) = app_state.get("gridSize").and_then(Value::as_f64)
        && grid > 0.0
    {
        document.canvas.grid_size = grid;
    }
    if let Some(background) = app_state.get("viewBackgroundColor") {
        document.canvas.background = parse_color(background, document.canvas.background);
    }
}

/// Reads and imports an Excalidraw scene from disk.
pub fn import_from_path(path: &std::path::Path) -> Result<ExcalidrawImport, ExcalidrawError> {
    let text = std::fs::read_to_string(path).map_err(ExcalidrawError::Io)?;
    import(&text)
}

/// Exports the document as an Excalidraw scene. Unknown element fields and
/// preserved envelope/app-state fields are merged back into the output.
pub fn export(document: &Document, viewport: &Viewport) -> Result<String, ExcalidrawError> {
    let mut elements: Vec<&Element> = document.elements.iter().collect();
    elements.sort_by_key(|element| element.z_order);

    let mut output_elements = Vec::with_capacity(elements.len());
    for element in elements {
        output_elements.push(export_element(element));
    }

    let mut app_state = document
        .extras
        .get(APP_STATE_EXTRAS_KEY)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    app_state.insert("zoom".to_owned(), json!({ "value": viewport.zoom }));
    app_state.insert(
        "scrollX".to_owned(),
        json!(-viewport.offset.x / viewport.zoom),
    );
    app_state.insert(
        "scrollY".to_owned(),
        json!(-viewport.offset.y / viewport.zoom),
    );
    app_state.insert("gridSize".to_owned(), json!(document.canvas.grid_size));
    app_state.insert(
        "viewBackgroundColor".to_owned(),
        json!(color_to_hex(document.canvas.background)),
    );

    let mut root = document
        .extras
        .get(ENVELOPE_EXTRAS_KEY)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    root.insert("type".to_owned(), json!(EXCALIDRAW_TYPE));
    root.insert("version".to_owned(), json!(EXCALIDRAW_VERSION));
    root.insert("source".to_owned(), json!("epsilon"));
    root.insert("elements".to_owned(), Value::Array(output_elements));
    root.insert("appState".to_owned(), Value::Object(app_state));
    root.insert(
        "files".to_owned(),
        document
            .extras
            .get(FILES_EXTRAS_KEY)
            .cloned()
            .unwrap_or_else(|| json!({})),
    );

    serde_json::to_string_pretty(&Value::Object(root)).map_err(ExcalidrawError::Json)
}

/// Writes an Excalidraw scene atomically, like native saves.
pub fn export_to_path(
    document: &Document,
    viewport: &Viewport,
    path: &std::path::Path,
) -> Result<(), ExcalidrawError> {
    let json = export(document, viewport)?;
    format::write_json_atomically(path, &json).map_err(ExcalidrawError::Write)
}

fn export_element(element: &Element) -> Value {
    let mut object: Map<String, Value> = element
        .extras
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    object.remove(ELEMENT_ID_KEY);
    let id = element
        .extras
        .get(ELEMENT_ID_KEY)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| element.id.get().to_string());
    object.insert("id".to_owned(), json!(id));
    object.insert("type".to_owned(), json!(excalidraw_type(element.kind)));
    object.insert("x".to_owned(), json!(element.x));
    object.insert("y".to_owned(), json!(element.y));
    object.insert("width".to_owned(), json!(element.width));
    object.insert("height".to_owned(), json!(element.height));
    object.insert("angle".to_owned(), json!(element.angle));
    object.insert(
        "strokeColor".to_owned(),
        json!(color_to_hex(element.style.stroke)),
    );
    object.insert(
        "backgroundColor".to_owned(),
        json!(if element.style.fill.a == 0 {
            "transparent".to_owned()
        } else {
            color_to_hex(element.style.fill)
        }),
    );
    object.insert(
        "fillStyle".to_owned(),
        json!(match element.style.fill_style {
            FillStyle::Solid => "solid",
            FillStyle::Hatch => "hachure",
            FillStyle::CrossHatch => "cross-hatch",
        }),
    );
    object.insert("strokeWidth".to_owned(), json!(element.style.stroke_width));
    object.insert(
        "strokeStyle".to_owned(),
        json!(match element.style.stroke_style {
            StrokeStyle::Solid => "solid",
            StrokeStyle::Dashed => "dashed",
            StrokeStyle::Dotted => "dotted",
        }),
    );
    object
        .entry("roughness".to_owned())
        .or_insert_with(|| json!(1));
    object.insert("opacity".to_owned(), json!(element.style.opacity * 100.0));
    object.insert("seed".to_owned(), json!(element.seed));
    object.insert("version".to_owned(), json!(element.version));
    object
        .entry("versionNonce".to_owned())
        .or_insert_with(|| json!(element.seed));
    object.entry("index".to_owned()).or_insert(Value::Null);
    object.insert("isDeleted".to_owned(), json!(false));
    object.insert(
        "groupIds".to_owned(),
        json!(
            element
                .group_ids
                .iter()
                .map(GroupId::as_str)
                .collect::<Vec<_>>()
        ),
    );
    object.insert(
        "roundness".to_owned(),
        if element.style.roundness > 0.0 {
            json!({ "type": 3, "radius": element.style.roundness })
        } else {
            Value::Null
        },
    );
    object
        .entry("boundElements".to_owned())
        .or_insert(json!([]));
    object.entry("updated".to_owned()).or_insert(json!(0));
    object.entry("created".to_owned()).or_insert(json!(0));
    object.entry("link".to_owned()).or_insert(Value::Null);
    object.entry("locked".to_owned()).or_insert(json!(false));

    match element.kind {
        ElementKind::Line | ElementKind::Arrow | ElementKind::Freehand => {
            object.insert(
                "points".to_owned(),
                json!(
                    element
                        .points
                        .iter()
                        .map(|point| json!([point.x - element.x, point.y - element.y]))
                        .collect::<Vec<_>>()
                ),
            );
            if element.kind == ElementKind::Arrow {
                object
                    .entry("startArrowhead".to_owned())
                    .or_insert(Value::Null);
                object
                    .entry("endArrowhead".to_owned())
                    .or_insert_with(|| json!("arrow"));
            }
            if element.kind == ElementKind::Line {
                object
                    .entry("startArrowhead".to_owned())
                    .or_insert(Value::Null);
                object
                    .entry("endArrowhead".to_owned())
                    .or_insert(Value::Null);
            }
        }
        ElementKind::Text => {
            object.insert("text".to_owned(), json!(element.text));
            object.insert("fontSize".to_owned(), json!(element.font_size));
            object
                .entry("fontFamily".to_owned())
                .or_insert_with(|| json!(1));
            object.insert(
                "textAlign".to_owned(),
                json!(match element.text_align {
                    TextAlign::Left => "left",
                    TextAlign::Center => "center",
                    TextAlign::Right => "right",
                }),
            );
            object
                .entry("verticalAlign".to_owned())
                .or_insert_with(|| json!("top"));
            object
                .entry("containerId".to_owned())
                .or_insert(Value::Null);
            object
                .entry("lineHeight".to_owned())
                .or_insert_with(|| json!(1.25));
            object.entry("autoResize".to_owned()).or_insert(json!(true));
        }
        ElementKind::Rectangle | ElementKind::Ellipse | ElementKind::Diamond => {}
    }

    Value::Object(object)
}

fn element_kind(value: &str) -> Option<ElementKind> {
    match value {
        "rectangle" => Some(ElementKind::Rectangle),
        "ellipse" => Some(ElementKind::Ellipse),
        "diamond" => Some(ElementKind::Diamond),
        "line" => Some(ElementKind::Line),
        "arrow" => Some(ElementKind::Arrow),
        "freedraw" | "draw" => Some(ElementKind::Freehand),
        "text" => Some(ElementKind::Text),
        _ => None,
    }
}

fn excalidraw_type(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::Rectangle => "rectangle",
        ElementKind::Ellipse => "ellipse",
        ElementKind::Diamond => "diamond",
        ElementKind::Line => "line",
        ElementKind::Arrow => "arrow",
        ElementKind::Freehand => "freedraw",
        ElementKind::Text => "text",
    }
}

fn parse_point(value: &Value) -> Option<Point> {
    if let Some(array) = value.as_array() {
        let x = array.first()?.as_f64()?;
        let y = array.get(1)?.as_f64()?;
        return Some(Point::new(x, y));
    }
    let object = value.as_object()?;
    Some(Point::new(
        object.get("x")?.as_f64()?,
        object.get("y")?.as_f64()?,
    ))
}

fn parse_color(value: &Value, fallback: Color) -> Color {
    let Some(text) = value.as_str() else {
        return fallback;
    };
    if text.eq_ignore_ascii_case("transparent") || text == "none" {
        return Color::TRANSPARENT;
    }
    let Some(hex) = text.strip_prefix('#') else {
        return fallback;
    };
    if !hex.is_ascii() {
        return fallback;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
    match hex.len() {
        3 => {
            let r = channel(0..1);
            let g = channel(1..2);
            let b = channel(2..3);
            match (r, g, b) {
                (Some(r), Some(g), Some(b)) => Color::rgb(r * 17, g * 17, b * 17),
                _ => fallback,
            }
        }
        6 => match (channel(0..2), channel(2..4), channel(4..6)) {
            (Some(r), Some(g), Some(b)) => Color::rgb(r, g, b),
            _ => fallback,
        },
        8 => match (channel(0..2), channel(2..4), channel(4..6), channel(6..8)) {
            (Some(r), Some(g), Some(b), Some(a)) => Color::rgba(r, g, b, a),
            _ => fallback,
        },
        _ => fallback,
    }
}

fn color_to_hex(color: Color) -> String {
    if color.a == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r, color.g, color.b, color.a
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r##"{
        "type": "excalidraw",
        "version": 2,
        "source": "test",
        "elements": [
            {
                "id": "rect-1", "type": "rectangle",
                "x": 10, "y": 20, "width": 100, "height": 50, "angle": 0.5,
                "strokeColor": "#ff0000", "backgroundColor": "#00ff0080",
                "fillStyle": "cross-hatch", "strokeWidth": 3, "strokeStyle": "dashed",
                "opacity": 50, "roughness": 2, "seed": 42, "version": 7,
                "groupIds": ["group-a"], "roundness": {"type": 3},
                "customData": {"kept": true}
            },
            {
                "id": "line-1", "type": "line",
                "x": 100, "y": 100, "width": 50, "height": 30,
                "strokeColor": "#000000", "strokeWidth": 2,
                "points": [[0, 0], [50, 30]]
            },
            {
                "id": "text-1", "type": "text",
                "x": 0, "y": 0, "width": 80, "height": 24,
                "text": "Hello", "fontSize": 24, "textAlign": "center",
                "strokeColor": "#000000", "backgroundColor": "transparent"
            },
            {
                "id": "shape-1", "type": "embeddable",
                "x": 0, "y": 0, "width": 10, "height": 10
            }
        ],
        "appState": {
            "gridSize": 25, "viewBackgroundColor": "#fafafa",
            "zoom": {"value": 2}, "scrollX": 10, "scrollY": -20,
            "futureAppState": "kept"
        },
        "files": {},
        "futureEnvelope": {"kept": true}
    }"##;

    #[test]
    fn imports_supported_elements_and_skips_unknown_types_with_warnings() {
        let imported = import(FIXTURE).expect("import fixture");
        assert_eq!(imported.document.elements.len(), 3);
        assert_eq!(imported.warnings.len(), 1);
        assert!(imported.warnings[0].contains("embeddable"));
        assert_eq!(imported.viewport.zoom, 2.0);
        assert_eq!(imported.document.canvas.grid_size, 25.0);
        assert_eq!(
            imported.document.canvas.background,
            Color::rgb(250, 250, 250)
        );

        let rectangle = &imported.document.elements[0];
        assert_eq!(rectangle.kind, ElementKind::Rectangle);
        assert_eq!(rectangle.style.stroke, Color::rgb(255, 0, 0));
        assert_eq!(rectangle.style.fill, Color::rgba(0, 255, 0, 128));
        assert_eq!(rectangle.style.fill_style, FillStyle::CrossHatch);
        assert_eq!(rectangle.style.stroke_style, StrokeStyle::Dashed);
        assert!((rectangle.style.opacity - 0.5).abs() < 1e-9);
        assert_eq!(rectangle.group_ids, vec![GroupId::new("group-a")]);
        assert_eq!(rectangle.extras["customData"]["kept"], json!(true));

        let line = &imported.document.elements[1];
        assert_eq!(line.kind, ElementKind::Line);
        assert_eq!(line.points[0], Point::new(100.0, 100.0));
        assert_eq!(line.points[1], Point::new(150.0, 130.0));

        let text = &imported.document.elements[2];
        assert_eq!(text.text, "Hello");
        assert_eq!(text.font_size, 24.0);
        assert_eq!(text.text_align, TextAlign::Center);
    }

    #[test]
    fn export_preserves_ids_groups_custom_data_and_unknown_envelope_fields() {
        let imported = import(FIXTURE).expect("import fixture");
        let exported = export(&imported.document, &imported.viewport).expect("export");
        let value: Value = serde_json::from_str(&exported).expect("parse export");

        assert_eq!(value["type"], "excalidraw");
        assert_eq!(value["version"], 2);
        assert_eq!(value["futureEnvelope"]["kept"], true);
        assert_eq!(value["appState"]["futureAppState"], "kept");
        assert_eq!(value["elements"][0]["id"], "rect-1");
        assert_eq!(value["elements"][0]["groupIds"][0], "group-a");
        assert_eq!(value["elements"][0]["customData"]["kept"], true);
        assert_eq!(value["elements"][1]["points"][1][0], 50.0);
        assert_eq!(value["elements"][1]["points"][1][1], 30.0);

        let reimported = import(&exported).expect("re-import export");
        assert_eq!(reimported.document.elements.len(), 3);
        assert_eq!(reimported.document.elements[0].kind, ElementKind::Rectangle);
    }

    #[test]
    fn invalid_scenes_report_clear_errors() {
        assert!(matches!(import("not json"), Err(ExcalidrawError::Json(_))));
        assert!(matches!(import("[]"), Err(ExcalidrawError::NotObject)));
        assert!(matches!(
            import("{}"),
            Err(ExcalidrawError::MissingElements)
        ));
        assert!(matches!(
            import(r#"{"elements": {}}"#),
            Err(ExcalidrawError::MissingElements)
        ));
    }

    #[test]
    fn linear_elements_without_two_points_are_skipped_with_warnings() {
        let imported = import(
            r#"{"elements":[{"type":"line","x":0,"y":0,"width":10,"height":10,"points":[]}]}"#,
        )
        .expect("import");
        assert!(imported.document.elements.is_empty());
        assert!(imported.warnings[0].contains("fewer than two points"));
    }

    #[test]
    fn file_export_and_import_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "epsilon-excalidraw-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("scene.excalidraw");
        let imported = import(FIXTURE).expect("import fixture");
        export_to_path(&imported.document, &imported.viewport, &path).expect("export file");
        let restored = import_from_path(&path).expect("import file");
        assert_eq!(restored.document.elements.len(), 3);
        assert!(matches!(
            import_from_path(&dir.join("missing.excalidraw")),
            Err(ExcalidrawError::Io(_))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn colors_round_trip_in_short_and_alpha_forms() {
        assert_eq!(
            parse_color(&json!("#abc"), Color::BLACK),
            Color::rgb(170, 187, 204)
        );
        assert_eq!(
            parse_color(&json!("#11223344"), Color::BLACK),
            Color::rgba(17, 34, 51, 68)
        );
        assert_eq!(
            parse_color(&json!("transparent"), Color::BLACK),
            Color::TRANSPARENT
        );
        assert_eq!(color_to_hex(Color::rgb(1, 2, 3)), "#010203");
        assert_eq!(color_to_hex(Color::rgba(1, 2, 3, 4)), "#01020304");
    }
}
