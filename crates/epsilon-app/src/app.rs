use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Layout, RichText};

use epsilon_core::clipboard::{ClipboardPayload, PASTE_OFFSET};
use epsilon_core::document::{
    Align as ElementAlign, Color as DocumentColor, Distribute as ElementDistribute, Document,
    Element, ElementId, ElementKind, FillStyle, StrokeStyle, TextAlign, ZOrderMove,
};
use epsilon_core::excalidraw;
use epsilon_core::export::{ExportOptions, ExportScale};
use epsilon_core::format::{EpsilonFile, FILE_EXTENSION, write_bytes_atomically};
use epsilon_core::geometry::{
    Point, Rect, ResizeHandle, arrowhead_points, constrain_angle, resize_rect, rotate_point,
    rough_polyline, simplify_polyline, smooth_polyline, snap_angle,
};
use epsilon_core::grid;
use epsilon_core::history::{History, Transaction};
use epsilon_core::recent::RecentFiles;
use epsilon_core::recovery::{RECOVERY_SOURCE_KEY, RecoveryStore};
use epsilon_core::viewport::Viewport;

use crate::layout;
use crate::theme::{self, Theme};
use crate::tools::{self, Tool};

/// Storage key for application settings (theme, active tool).
const SETTINGS_KEY: &str = "epsilon.settings";

/// App-owned settings persisted across restarts.
///
/// Window geometry and panel widths are not stored here: eframe persists the
/// window settings (`persist_window`) and the egui memory, which includes
/// resizable panel sizes, in the same storage on exit.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct PersistedSettings {
    theme: Theme,
    tool: Tool,
    #[serde(default)]
    grid_enabled: bool,
    #[serde(default)]
    clean_rendering: bool,
    #[serde(default)]
    recent: RecentFiles,
}

/// How often an unsaved dirty document is written to the recovery directory.
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

/// Minimum on-screen width/height (or line length) for a drag to be committed.
const MIN_SHAPE_PIXELS: f64 = 3.0;

/// Shift-constrained linear tools snap to 15-degree increments.
const LINE_ANGLE_STEP: f64 = std::f64::consts::PI / 12.0;

/// Arrowhead size in screen pixels, independent of zoom. The PNG exporter
/// reuses these world-space defaults so exports match the canvas.
pub(crate) const ARROW_HEAD_LENGTH_PIXELS: f64 = 12.0;
pub(crate) const ARROW_HEAD_HALF_WIDTH_PIXELS: f64 = 6.0;

/// Freehand input is sampled at least this many screen pixels apart.
const MIN_FREEHAND_SAMPLE_PIXELS: f64 = 1.5;
/// Simplification tolerance for a finished freehand stroke, in screen pixels.
const FREEHAND_SIMPLIFY_PIXELS: f64 = 1.0;
/// Hard cap that keeps long freehand strokes from growing without bound.
const MAX_FREEHAND_POINTS: usize = 4096;

/// Defaults for newly placed text.
const DEFAULT_FONT_SIZE: f64 = 20.0;
const DEFAULT_TEXT_WIDTH: f64 = 180.0;
const MIN_TEXT_WIDTH_PIXELS: f64 = 40.0;

/// Extra screen-space radius used when selecting strokes.
const HIT_TOLERANCE_PIXELS: f64 = 6.0;
const HANDLE_SIZE_PIXELS: f32 = 8.0;
const HANDLE_HIT_RADIUS_PIXELS: f32 = 7.0;
const MIN_ELEMENT_PIXELS: f64 = 8.0;
const ROTATION_HANDLE_OFFSET_PIXELS: f32 = 24.0;
const ROTATION_HANDLE_RADIUS_PIXELS: f32 = 5.0;

/// Screen-space jitter amplitude for hand-drawn strokes.
pub(crate) const ROUGH_AMPLITUDE_PIXELS: f64 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq)]
struct SelectionProperties {
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    angle_degrees: Option<f64>,
}

/// High-level pointer interaction mode for the canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PointerMode {
    Idle,
    Panning {
        last: egui::Pos2,
    },
    /// A drag that creates or moves canvas content, in world coordinates.
    Dragging {
        kind: ElementKind,
        start: Point,
        current: Point,
    },
    /// A freehand stroke being collected in world coordinates.
    Freehanding {
        last: Point,
    },
    /// A marquee drag with an additive-selection flag.
    Selecting {
        start: Point,
        current: Point,
        additive: bool,
    },
    /// Moving the current selection from a captured start point.
    Moving {
        start: Point,
        current: Point,
    },
    /// Resizing the current selection from one of its eight handles.
    Resizing {
        handle: ResizeHandle,
    },
    /// Rotating the current selection around its collective center.
    Rotating,
}

/// Element kind created by a drag with the given tool.
fn drawing_kind(tool: Tool) -> Option<ElementKind> {
    match tool {
        Tool::Rectangle => Some(ElementKind::Rectangle),
        Tool::Ellipse => Some(ElementKind::Ellipse),
        Tool::Diamond => Some(ElementKind::Diamond),
        Tool::Line => Some(ElementKind::Line),
        Tool::Arrow => Some(ElementKind::Arrow),
        Tool::Freehand => Some(ElementKind::Freehand),
        Tool::Text => Some(ElementKind::Text),
        _ => None,
    }
}

/// Applies the active drag's optional constraint to its endpoint.
fn drawing_end(kind: ElementKind, start: Point, end: Point, constrain: bool) -> Point {
    if constrain && matches!(kind, ElementKind::Line | ElementKind::Arrow) {
        constrain_angle(start, end, LINE_ANGLE_STEP)
    } else {
        end
    }
}

/// Projects a world-space point into canvas screen space.
fn world_to_screen_pos(canvas_rect: egui::Rect, viewport: &Viewport, point: Point) -> egui::Pos2 {
    let screen = viewport.world_to_screen(point);
    egui::pos2(
        canvas_rect.min.x + screen.x as f32,
        canvas_rect.min.y + screen.y as f32,
    )
}

/// Projects an element's world bounds into screen space.
fn element_screen_rect(
    canvas_rect: egui::Rect,
    viewport: &Viewport,
    element: &Element,
) -> egui::Rect {
    egui::Rect::from_two_pos(
        world_to_screen_pos(canvas_rect, viewport, Point::new(element.x, element.y)),
        world_to_screen_pos(
            canvas_rect,
            viewport,
            Point::new(element.x + element.width, element.y + element.height),
        ),
    )
}

/// Projects an arrowhead into screen space, keeping its size zoom-independent.
fn arrowhead_screen_points(
    canvas_rect: egui::Rect,
    viewport: &Viewport,
    start: Point,
    end: Point,
) -> Option<Vec<egui::Pos2>> {
    let head = arrowhead_points(
        start,
        end,
        ARROW_HEAD_LENGTH_PIXELS / viewport.zoom,
        ARROW_HEAD_HALF_WIDTH_PIXELS / viewport.zoom,
    )?;
    Some(
        head.into_iter()
            .map(|point| world_to_screen_pos(canvas_rect, viewport, point))
            .collect(),
    )
}

fn paint_fill_path(
    painter: &egui::Painter,
    points: &[egui::Pos2],
    style: FillStyle,
    color: egui::Color32,
) {
    if points.len() < 3 {
        return;
    }
    match style {
        FillStyle::Solid => {
            painter.add(egui::Shape::convex_polygon(
                points.to_vec(),
                color,
                egui::Stroke::NONE,
            ));
        }
        FillStyle::Hatch => paint_hatch(painter, points, color, 8.0, false),
        FillStyle::CrossHatch => paint_hatch(painter, points, color, 8.0, true),
    }
}

fn paint_hatch(
    painter: &egui::Painter,
    polygon: &[egui::Pos2],
    color: egui::Color32,
    spacing: f32,
    cross: bool,
) {
    let stroke = egui::Stroke::new(1.0, color);
    for sign in [1.0f32, -1.0] {
        if sign < 0.0 && !cross {
            break;
        }
        let values: Vec<f32> = polygon
            .iter()
            .map(|point| point.y + sign * point.x)
            .collect();
        let min = values.iter().copied().fold(f32::INFINITY, f32::min);
        let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let count = (((max - min) / spacing).ceil() as usize).min(500);
        for index in 0..=count {
            let offset = min + index as f32 * spacing;
            let mut hits = Vec::new();
            for edge in 0..polygon.len() {
                let previous = polygon[(edge + polygon.len() - 1) % polygon.len()];
                let current = polygon[edge];
                let previous_value = previous.y + sign * previous.x;
                let current_value = current.y + sign * current.x;
                let denominator = current_value - previous_value;
                if denominator.abs() <= f32::EPSILON {
                    continue;
                }
                let t = (offset - previous_value) / denominator;
                if (0.0..=1.0).contains(&t) {
                    hits.push(previous + (current - previous) * t);
                }
            }
            hits.sort_by(|left, right| left.x.total_cmp(&right.x));
            for pair in hits.as_chunks::<2>().0 {
                painter.line_segment([pair[0], pair[1]], stroke);
            }
        }
    }
}

fn paint_stroke_path(
    painter: &egui::Painter,
    points: &[egui::Pos2],
    closed: bool,
    stroke: egui::Stroke,
    style: StrokeStyle,
) {
    if points.len() < 2 {
        return;
    }
    let (dash, gap) = match style {
        StrokeStyle::Solid => {
            if closed {
                painter.add(egui::Shape::closed_line(points.to_vec(), stroke));
            } else {
                painter.add(egui::Shape::line(points.to_vec(), stroke));
            }
            return;
        }
        StrokeStyle::Dashed => (8.0, 6.0),
        StrokeStyle::Dotted => (1.0, 4.0),
    };

    let segment_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for index in 0..segment_count {
        let start = points[index];
        let end = points[(index + 1) % points.len()];
        let vector = end - start;
        let length = vector.length();
        if length <= f32::EPSILON {
            continue;
        }
        let direction = vector / length;
        let mut distance = 0.0;
        while distance < length {
            let dash_end = (distance + dash).min(length);
            painter.line_segment(
                [start + direction * distance, start + direction * dash_end],
                stroke,
            );
            distance = dash_end + gap;
        }
    }
}

/// World-space point after applying the element's rotation around its center.
fn element_world_point(element: &Element, point: Point) -> Point {
    rotate_point(point, element.bounds().center(), element.angle)
}

fn element_stroke_screen_points(
    canvas_rect: egui::Rect,
    viewport: &Viewport,
    element: &Element,
    local_points: impl IntoIterator<Item = Point>,
    closed: bool,
    clean: bool,
) -> Vec<egui::Pos2> {
    let world_points: Vec<Point> = local_points
        .into_iter()
        .map(|point| element_world_point(element, point))
        .collect();
    let world_points = if clean {
        world_points
    } else {
        rough_polyline(
            &world_points,
            element.seed,
            ROUGH_AMPLITUDE_PIXELS / viewport.zoom,
            closed,
        )
    };
    world_points
        .into_iter()
        .map(|point| world_to_screen_pos(canvas_rect, viewport, point))
        .collect()
}

fn ellipse_local_points(bounds: Rect) -> Vec<Point> {
    let center = bounds.center();
    let radius = bounds.size() * 0.5;
    (0..48)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / 48.0;
            Point::new(
                center.x + angle.cos() * radius.x,
                center.y + angle.sin() * radius.y,
            )
        })
        .collect()
}

/// Projects an element's inscribed diamond into screen space.
fn diamond_screen_points(
    canvas_rect: egui::Rect,
    viewport: &Viewport,
    bounds: Rect,
) -> Vec<egui::Pos2> {
    bounds
        .diamond_points()
        .into_iter()
        .map(|point| world_to_screen_pos(canvas_rect, viewport, point))
        .collect()
}

/// Projects a draft drag's world bounds into screen space.
fn draft_screen_rect(
    canvas_rect: egui::Rect,
    viewport: &Viewport,
    start: Point,
    current: Point,
) -> egui::Rect {
    let min = viewport.world_to_screen(Point::new(start.x.min(current.x), start.y.min(current.y)));
    let max = viewport.world_to_screen(Point::new(start.x.max(current.x), start.y.max(current.y)));
    egui::Rect::from_two_pos(
        egui::pos2(
            canvas_rect.min.x + min.x as f32,
            canvas_rect.min.y + min.y as f32,
        ),
        egui::pos2(
            canvas_rect.min.x + max.x as f32,
            canvas_rect.min.y + max.y as f32,
        ),
    )
}

/// Projects a world-space rectangle into canvas screen space.
fn world_rect_screen(canvas_rect: egui::Rect, viewport: &Viewport, rect: Rect) -> egui::Rect {
    egui::Rect::from_two_pos(
        world_to_screen_pos(canvas_rect, viewport, rect.min),
        world_to_screen_pos(canvas_rect, viewport, rect.max),
    )
}

/// Screen positions of the eight resize handles around a selection.
fn handle_screen_positions(bounds: egui::Rect) -> [(ResizeHandle, egui::Pos2); 8] {
    let left = bounds.left();
    let center_x = bounds.center().x;
    let right = bounds.right();
    let top = bounds.top();
    let center_y = bounds.center().y;
    let bottom = bounds.bottom();
    [
        (ResizeHandle::NorthWest, egui::pos2(left, top)),
        (ResizeHandle::North, egui::pos2(center_x, top)),
        (ResizeHandle::NorthEast, egui::pos2(right, top)),
        (ResizeHandle::East, egui::pos2(right, center_y)),
        (ResizeHandle::SouthEast, egui::pos2(right, bottom)),
        (ResizeHandle::South, egui::pos2(center_x, bottom)),
        (ResizeHandle::SouthWest, egui::pos2(left, bottom)),
        (ResizeHandle::West, egui::pos2(left, center_y)),
    ]
}

fn rotation_handle_position(bounds: egui::Rect) -> egui::Pos2 {
    egui::pos2(
        bounds.center().x,
        bounds.top() - ROTATION_HANDLE_OFFSET_PIXELS,
    )
}

fn document_color(color: DocumentColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

fn uniform_value(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let mut values = values.into_iter();
    let first = values.next()?;
    values
        .all(|value| (value - first).abs() <= 1e-6)
        .then_some(first)
}

/// Renders a numeric field, showing `Mixed` when a selection disagrees.
fn scalar_field(
    ui: &mut egui::Ui,
    label: &str,
    value: f64,
    mixed: bool,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
) -> Option<f64> {
    let mut current = value;
    ui.label(label);
    let response = ui.add(
        egui::DragValue::new(&mut current)
            .speed(speed)
            .range(range)
            .suffix(suffix)
            .custom_formatter(move |number, _| {
                if mixed {
                    "Mixed".to_owned()
                } else {
                    format!("{number:.1}")
                }
            }),
    );
    response.changed().then_some(current)
}

fn normalized_angle(angle: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    (angle + std::f64::consts::PI).rem_euclid(tau) - std::f64::consts::PI
}

/// Angle shown in properties. Linear elements derive their direction from
/// their endpoints so property edits stay stable after handle rotation.
fn element_property_angle(element: &Element) -> f64 {
    if matches!(element.kind, ElementKind::Line | ElementKind::Arrow) && element.points.len() == 2 {
        let start = element.points[0];
        let end = element.points[1];
        normalized_angle((end.y - start.y).atan2(end.x - start.x) + element.angle)
    } else {
        normalized_angle(element.angle)
    }
}

fn document_color_with_opacity(color: DocumentColor, opacity: f64) -> egui::Color32 {
    let alpha = (color.a as f64 * opacity.clamp(0.0, 1.0)).round() as u8;
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, alpha)
}

fn document_color_from_egui(color: egui::Color32) -> DocumentColor {
    let [r, g, b, a] = color.to_array();
    DocumentColor::rgba(r, g, b, a)
}

/// Measures wrapped text in world units for the current zoom.
fn measured_text_height(
    ctx: &egui::Context,
    text: &str,
    font_size: f64,
    wrap_width: f64,
    zoom: f64,
) -> f64 {
    let font = egui::FontId::proportional((font_size * zoom).max(1.0) as f32);
    let wrap = (wrap_width * zoom).max(1.0) as f32;
    let galley =
        ctx.fonts_mut(|fonts| fonts.layout(text.to_owned(), font, egui::Color32::WHITE, wrap));
    ((galley.size().y / zoom as f32) as f64).max(font_size * 1.25)
}

/// Result of the last clipboard command, reported in the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ClipboardFeedback {
    Copied(usize),
    Cut(usize),
    Pasted(usize),
    Unsupported,
    Error(String),
}

impl ClipboardFeedback {
    fn is_error(&self) -> bool {
        matches!(self, Self::Unsupported | Self::Error(_))
    }

    fn message(&self) -> String {
        fn elements(count: usize) -> String {
            format!("{count} element{}", if count == 1 { "" } else { "s" })
        }

        match self {
            Self::Copied(count) => format!("Copied {}", elements(*count)),
            Self::Cut(count) => format!("Cut {}", elements(*count)),
            Self::Pasted(count) => format!("Pasted {}", elements(*count)),
            Self::Unsupported => "Clipboard has no Epsilon elements".to_owned(),
            Self::Error(message) => message.clone(),
        }
    }
}

/// Result of the most recent save or open, shown in the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FileFeedback {
    New(String),
    Saved(String),
    Opened(String),
    Recovered(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingAction {
    New,
    Open(Option<PathBuf>),
    Import(PathBuf),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

impl FileFeedback {
    fn message(&self) -> &str {
        match self {
            Self::New(message)
            | Self::Saved(message)
            | Self::Opened(message)
            | Self::Recovered(message) => message,
            Self::Error(message) => message,
        }
    }

    fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

/// Actions offered by the canvas context menu. The list is filtered per target
/// so the menu never shows a command that cannot run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Duplicate,
    Delete,
    Group,
    Ungroup,
    BringToFront,
    BringForward,
    SendBackward,
    SendToBack,
}

impl ContextAction {
    const ALL: [Self; 12] = [
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::SelectAll,
        Self::Duplicate,
        Self::Delete,
        Self::Group,
        Self::Ungroup,
        Self::BringToFront,
        Self::BringForward,
        Self::SendBackward,
        Self::SendToBack,
    ];

    fn action(self) -> Action {
        match self {
            Self::Cut => Action::Cut,
            Self::Copy => Action::Copy,
            Self::Paste => Action::Paste,
            Self::SelectAll => Action::SelectAll,
            Self::Duplicate => Action::Duplicate,
            Self::Delete => Action::Delete,
            Self::Group => Action::Group,
            Self::Ungroup => Action::Ungroup,
            Self::BringToFront => Action::ZOrder(ZOrderMove::Front),
            Self::BringForward => Action::ZOrder(ZOrderMove::Forward),
            Self::SendBackward => Action::ZOrder(ZOrderMove::Backward),
            Self::SendToBack => Action::ZOrder(ZOrderMove::Back),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::SelectAll => "Select All",
            Self::Duplicate => "Duplicate",
            Self::Delete => "Delete",
            Self::Group => "Group",
            Self::Ungroup => "Ungroup",
            Self::BringToFront => "Bring to Front",
            Self::BringForward => "Bring Forward",
            Self::SendBackward => "Send Backward",
            Self::SendToBack => "Send to Back",
        }
    }

    fn is_z_order(self) -> bool {
        matches!(
            self,
            Self::BringToFront | Self::BringForward | Self::SendBackward | Self::SendToBack
        )
    }
}

/// Shortcut text from the central registry, so menus and the help overlay
/// cannot drift apart.
fn shortcut_for(action: Action) -> Option<&'static str> {
    KEYBINDINGS
        .iter()
        .find(|binding| binding.action == action)
        .map(|binding| binding.shortcut)
}

/// Root application object and native shell.
///
/// This is the Stage 1 shell: regions are laid out and themed, with the tool
/// palette, working menu/toolbar actions, properties panel and status bar. The
/// shortcut registry and help overlay (1.7) are implemented here; canvas work
/// remains for Stage 2.
pub struct EpsilonApp {
    about_open: bool,
    theme: Theme,
    tool: Tool,
    /// Whether the right properties panel is expanded. In-session state; the
    /// panel's width still persists through egui memory.
    properties_open: bool,
    /// Whether the keyboard-shortcut help overlay is open.
    help_open: bool,
    /// Canvas camera. In-memory only for now; persistence can come later.
    viewport: Viewport,
    /// In-memory document being edited.
    document: Document,
    /// Path of the file this document was last saved to or opened from.
    file_path: Option<PathBuf>,
    /// Rotating autosave store under the application data directory.
    recovery_store: Option<RecoveryStore>,
    /// A recovered autosave awaiting the user's Recover/Discard choice.
    recovery_offer: Option<EpsilonFile>,
    /// Time of the last autosave attempt (successful or not).
    last_autosave: Instant,
    /// A guard action waiting for the unsaved-changes prompt to resolve.
    pending_action: Option<PendingAction>,
    /// True once the user has chosen to close, so the close guard stops
    /// cancelling the compositor's close request.
    allow_close: bool,
    /// Document snapshot at the last save/open, used for dirty tracking.
    saved_document: Document,
    /// Result of the last save/open command.
    file_feedback: Option<FileFeedback>,
    /// Persisted most-recently-used document paths.
    recent: RecentFiles,
    /// Whether the startup welcome panel is still open.
    welcome_open: bool,
    /// Last window title sent to the compositor, to avoid repeating commands.
    last_window_title: String,
    /// Reversible edit history for the document.
    history: History,
    /// Last failed history operation, shown in the status bar.
    history_error: Option<String>,
    /// Last copied or cut selection. Menu paste uses this in-app payload;
    /// Ctrl+V reads the platform clipboard text instead.
    clipboard: Option<ClipboardPayload>,
    /// Number of times the current clipboard payload has been pasted, used to
    /// cascade repeated pastes instead of stacking copies exactly.
    paste_count: u32,
    /// Last clipboard command result, shown in the status bar.
    clipboard_feedback: Option<ClipboardFeedback>,
    /// Selection snapshot captured before a continuous properties edit.
    property_edit_before: Option<Vec<Element>>,
    /// Last pointer position in world coordinates, if the pointer is over the
    /// canvas. Drives the status-bar readout.
    pointer_world: Option<Point>,
    /// Whether the canvas grid is visible.
    grid_enabled: bool,
    /// Draw exact architect-style strokes instead of seeded rough strokes.
    clean_rendering: bool,
    /// Last canvas rect, used by toolbar zoom controls.
    canvas_rect: egui::Rect,
    /// Current pointer interaction mode for the canvas.
    pointer_mode: PointerMode,
    /// Last known pointer position in screen space, including outside canvas.
    hover_screen: Option<egui::Pos2>,
    /// World-space samples for the freehand stroke currently being drawn.
    freehand_points: Vec<Point>,
    /// Element currently open in the in-canvas text editor.
    editing_text: Option<ElementId>,
    /// Whether the edited text element has not been committed to history yet.
    editing_text_is_new: bool,
    text_buffer: String,
    text_editor_needs_focus: bool,
    /// Elements currently selected in the canvas.
    selected: HashSet<ElementId>,
    /// Snapshots used by move/resize so every frame transforms from the drag
    /// origin instead of accumulating rounding drift.
    transform_original: Vec<Element>,
    resize_start_bounds: Rect,
    rotation_center: Point,
    rotation_start_angle: f64,
}

impl Default for EpsilonApp {
    fn default() -> Self {
        Self {
            about_open: false,
            theme: Theme::default(),
            tool: Tool::default(),
            properties_open: true,
            help_open: false,
            viewport: Viewport::default(),
            document: Document::default(),
            file_path: None,
            recovery_store: eframe::storage_dir("Epsilon")
                .map(|dir| RecoveryStore::new(dir.join("recovery"))),
            recovery_offer: None,
            last_autosave: Instant::now(),
            pending_action: None,
            allow_close: false,
            saved_document: Document::default(),
            file_feedback: None,
            recent: RecentFiles::new(),
            welcome_open: true,
            last_window_title: String::new(),
            history: History::default(),
            history_error: None,
            clipboard: None,
            paste_count: 0,
            clipboard_feedback: None,
            property_edit_before: None,
            pointer_world: None,
            canvas_rect: egui::Rect::NOTHING,
            grid_enabled: false,
            clean_rendering: false,
            pointer_mode: PointerMode::Idle,
            hover_screen: None,
            freehand_points: Vec::new(),
            editing_text: None,
            editing_text_is_new: false,
            text_buffer: String::new(),
            text_editor_needs_focus: false,
            selected: HashSet::new(),
            transform_original: Vec::new(),
            resize_start_bounds: Rect::default(),
            rotation_center: Point::ZERO,
            rotation_start_angle: 0.0,
        }
    }
}

/// Actions that can be triggered by a keyboard shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    SelectTool(Tool),
    ToggleHelp,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomFit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    New,
    Open,
    Save,
    SaveAs,
    Delete,
    Duplicate,
    SelectAll,
    ClearSelection,
    Group,
    Ungroup,
    ZOrder(ZOrderMove),
}

/// One entry in the central shortcut registry.
#[derive(Debug, Clone, Copy)]
struct KeyBinding {
    /// Display text for the key (for example `V` or `F1`).
    shortcut: &'static str,
    key: egui::Key,
    modifiers: egui::Modifiers,
    action: Action,
    description: &'static str,
}

/// All keyboard shortcuts implemented by the shell. Panels and menus should
/// consult this table rather than hard-coding key checks.
const KEYBINDINGS: &[KeyBinding] = &[
    KeyBinding {
        shortcut: "V",
        key: egui::Key::V,
        modifiers: egui::Modifiers::NONE,
        action: Action::SelectTool(Tool::Select),
        description: "Select tool",
    },
    KeyBinding {
        shortcut: "H",
        key: egui::Key::H,
        modifiers: egui::Modifiers::NONE,
        action: Action::SelectTool(Tool::Hand),
        description: "Hand (pan) tool",
    },
    KeyBinding {
        shortcut: "R",
        key: egui::Key::R,
        modifiers: egui::Modifiers::NONE,
        action: Action::SelectTool(Tool::Rectangle),
        description: "Rectangle tool",
    },
    KeyBinding {
        shortcut: "O",
        key: egui::Key::O,
        modifiers: egui::Modifiers::NONE,
        action: Action::SelectTool(Tool::Ellipse),
        description: "Ellipse tool",
    },
    KeyBinding {
        shortcut: "D",
        key: egui::Key::D,
        modifiers: egui::Modifiers::NONE,
        action: Action::SelectTool(Tool::Diamond),
        description: "Diamond tool",
    },
    KeyBinding {
        shortcut: "L",
        key: egui::Key::L,
        modifiers: egui::Modifiers::NONE,
        action: Action::SelectTool(Tool::Line),
        description: "Line tool",
    },
    KeyBinding {
        shortcut: "A",
        key: egui::Key::A,
        modifiers: egui::Modifiers::NONE,
        action: Action::SelectTool(Tool::Arrow),
        description: "Arrow tool",
    },
    KeyBinding {
        shortcut: "P",
        key: egui::Key::P,
        modifiers: egui::Modifiers::NONE,
        action: Action::SelectTool(Tool::Freehand),
        description: "Freehand tool",
    },
    KeyBinding {
        shortcut: "T",
        key: egui::Key::T,
        modifiers: egui::Modifiers::NONE,
        action: Action::SelectTool(Tool::Text),
        description: "Text tool",
    },
    KeyBinding {
        shortcut: "Ctrl+Z",
        key: egui::Key::Z,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::Undo,
        description: "Undo",
    },
    KeyBinding {
        shortcut: "Ctrl+Shift+Z",
        key: egui::Key::Z,
        modifiers: egui::Modifiers {
            shift: true,
            command: true,
            ..egui::Modifiers::NONE
        },
        action: Action::Redo,
        description: "Redo",
    },
    KeyBinding {
        shortcut: "Ctrl+Y",
        key: egui::Key::Y,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::Redo,
        description: "Redo",
    },
    // eframe's winit integration turns Ctrl+C/X/V into clipboard events before
    // egui sees a key press. These entries document the shortcuts (the help
    // overlay is generated from this table) and act as a key-event fallback.
    KeyBinding {
        shortcut: "Ctrl+C",
        key: egui::Key::C,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::Copy,
        description: "Copy selection",
    },
    KeyBinding {
        shortcut: "Ctrl+X",
        key: egui::Key::X,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::Cut,
        description: "Cut selection",
    },
    KeyBinding {
        shortcut: "Ctrl+V",
        key: egui::Key::V,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::Paste,
        description: "Paste",
    },
    KeyBinding {
        shortcut: "Ctrl+N",
        key: egui::Key::N,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::New,
        description: "New document",
    },
    KeyBinding {
        shortcut: "Ctrl+O",
        key: egui::Key::O,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::Open,
        description: "Open document",
    },
    KeyBinding {
        shortcut: "Ctrl+S",
        key: egui::Key::S,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::Save,
        description: "Save document",
    },
    KeyBinding {
        shortcut: "Ctrl+Shift+S",
        key: egui::Key::S,
        modifiers: egui::Modifiers {
            shift: true,
            command: true,
            ..egui::Modifiers::NONE
        },
        action: Action::SaveAs,
        description: "Save document as",
    },
    KeyBinding {
        shortcut: "Delete",
        key: egui::Key::Delete,
        modifiers: egui::Modifiers::NONE,
        action: Action::Delete,
        description: "Delete selection",
    },
    KeyBinding {
        shortcut: "Backspace",
        key: egui::Key::Backspace,
        modifiers: egui::Modifiers::NONE,
        action: Action::Delete,
        description: "Delete selection",
    },
    KeyBinding {
        shortcut: "Ctrl+D",
        key: egui::Key::D,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::Duplicate,
        description: "Duplicate selection",
    },
    KeyBinding {
        shortcut: "Ctrl+A",
        key: egui::Key::A,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::SelectAll,
        description: "Select all",
    },
    KeyBinding {
        shortcut: "Esc",
        key: egui::Key::Escape,
        modifiers: egui::Modifiers::NONE,
        action: Action::ClearSelection,
        description: "Clear selection and return to Select",
    },
    KeyBinding {
        shortcut: "Ctrl+G",
        key: egui::Key::G,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::Group,
        description: "Group selection",
    },
    KeyBinding {
        shortcut: "Ctrl+Shift+G",
        key: egui::Key::G,
        modifiers: egui::Modifiers {
            shift: true,
            command: true,
            ..egui::Modifiers::NONE
        },
        action: Action::Ungroup,
        description: "Ungroup selection",
    },
    KeyBinding {
        shortcut: "Ctrl+Shift+]",
        key: egui::Key::CloseBracket,
        modifiers: egui::Modifiers {
            shift: true,
            command: true,
            ..egui::Modifiers::NONE
        },
        action: Action::ZOrder(ZOrderMove::Front),
        description: "Bring to front",
    },
    KeyBinding {
        shortcut: "Ctrl+]",
        key: egui::Key::CloseBracket,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::ZOrder(ZOrderMove::Forward),
        description: "Bring forward",
    },
    KeyBinding {
        shortcut: "Ctrl+[",
        key: egui::Key::OpenBracket,
        modifiers: egui::Modifiers::COMMAND,
        action: Action::ZOrder(ZOrderMove::Backward),
        description: "Send backward",
    },
    KeyBinding {
        shortcut: "Ctrl+Shift+[",
        key: egui::Key::OpenBracket,
        modifiers: egui::Modifiers {
            shift: true,
            command: true,
            ..egui::Modifiers::NONE
        },
        action: Action::ZOrder(ZOrderMove::Back),
        description: "Send to back",
    },
    KeyBinding {
        shortcut: "F1",
        key: egui::Key::F1,
        modifiers: egui::Modifiers::NONE,
        action: Action::ToggleHelp,
        description: "Toggle shortcut help",
    },
    KeyBinding {
        shortcut: "?",
        key: egui::Key::Questionmark,
        modifiers: egui::Modifiers {
            shift: true,
            ..egui::Modifiers::NONE
        },
        action: Action::ToggleHelp,
        description: "Toggle shortcut help",
    },
    KeyBinding {
        shortcut: "+",
        key: egui::Key::Plus,
        modifiers: egui::Modifiers::NONE,
        action: Action::ZoomIn,
        description: "Zoom in",
    },
    KeyBinding {
        shortcut: "=",
        key: egui::Key::Equals,
        modifiers: egui::Modifiers::NONE,
        action: Action::ZoomIn,
        description: "Zoom in",
    },
    KeyBinding {
        shortcut: "-",
        key: egui::Key::Minus,
        modifiers: egui::Modifiers::NONE,
        action: Action::ZoomOut,
        description: "Zoom out",
    },
    KeyBinding {
        shortcut: "0",
        key: egui::Key::Num0,
        modifiers: egui::Modifiers::NONE,
        action: Action::ZoomReset,
        description: "Reset zoom",
    },
    KeyBinding {
        shortcut: "Shift+0",
        key: egui::Key::Num0,
        modifiers: egui::Modifiers {
            shift: true,
            ..egui::Modifiers::NONE
        },
        action: Action::ZoomFit,
        description: "Fit to content",
    },
];

impl EpsilonApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::from_storage(cc.storage);
        theme::apply(&cc.egui_ctx, app.theme);
        app.offer_recovery();
        app
    }

    /// Restores app-owned settings. Missing or invalid storage falls back to
    /// defaults; eframe logs decode failures rather than propagating them.
    fn from_storage(storage: Option<&dyn eframe::Storage>) -> Self {
        let mut app = Self::default();
        if let Some(settings) = storage
            .and_then(|storage| eframe::get_value::<PersistedSettings>(storage, SETTINGS_KEY))
        {
            app.theme = settings.theme;
            app.tool = settings.tool;
            app.grid_enabled = settings.grid_enabled;
            app.clean_rendering = settings.clean_rendering;
            app.recent = settings.recent;
        }
        app
    }

    fn set_theme(&mut self, ctx: &egui::Context, theme: Theme) {
        if self.theme != theme {
            self.theme = theme;
            theme::apply(ctx, theme);
        }
    }

    fn open_about(&mut self) {
        self.about_open = true;
    }

    fn toggle_theme(&mut self, ctx: &egui::Context) {
        let next = match self.theme {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        };
        self.set_theme(ctx, next);
    }

    /// Undo is unavailable while a text element is being edited, because its
    /// creation is recorded when the editor commits.
    fn can_undo(&self) -> bool {
        self.editing_text.is_none() && self.history.can_undo()
    }

    fn can_redo(&self) -> bool {
        self.editing_text.is_none() && self.history.can_redo()
    }

    /// Clipboard commands are unavailable while an in-place text editor owns
    /// the selection, mirroring undo/redo.
    fn can_copy(&self) -> bool {
        self.editing_text.is_none() && !self.selected.is_empty()
    }

    fn can_paste(&self) -> bool {
        self.editing_text.is_none()
            && self
                .clipboard
                .as_ref()
                .is_some_and(ClipboardPayload::is_supported)
    }

    /// Selected elements in document (insertion) order, as stored on the
    /// clipboard. Paste re-sorts them by paint order.
    fn selection_elements(&self) -> Vec<Element> {
        self.document
            .elements
            .iter()
            .filter(|element| self.selected.contains(&element.id))
            .cloned()
            .collect()
    }

    /// Copies the selection as JSON to the system clipboard and remembers the
    /// payload for the Edit menu, which cannot read the platform clipboard.
    fn copy_selection(&mut self, ctx: &egui::Context) -> bool {
        if !self.can_copy() {
            return false;
        }
        let payload = ClipboardPayload::new(self.selection_elements());
        let json = match payload.to_json() {
            Ok(json) => json,
            Err(error) => {
                self.clipboard_feedback =
                    Some(ClipboardFeedback::Error(format!("Copy failed: {error}")));
                return false;
            }
        };
        let count = payload.elements.len();
        ctx.copy_text(json);
        self.clipboard = Some(payload);
        self.paste_count = 0;
        self.clipboard_feedback = Some(ClipboardFeedback::Copied(count));
        true
    }

    /// Removes the selection through history. Shared by Cut and later delete
    /// commands so both produce one undoable transaction.
    fn delete_selection(&mut self) -> bool {
        if !self.can_copy() || self.history_blocked() {
            return false;
        }
        let ids: Vec<ElementId> = self
            .selection_elements()
            .iter()
            .map(|element| element.id)
            .collect();
        match Transaction::remove(&self.document, &ids) {
            Ok(transaction) => {
                if self.commit_transaction(transaction) {
                    self.selected.clear();
                    true
                } else {
                    false
                }
            }
            Err(error) => {
                self.history_error = Some(format!("Edit failed: {error}"));
                false
            }
        }
    }

    fn cut_selection(&mut self, ctx: &egui::Context) -> bool {
        if !self.can_copy() {
            return false;
        }
        let count = self.selection_elements().len();
        // Copy before deleting so the clipboard survives a failed removal.
        if !self.copy_selection(ctx) {
            return false;
        }
        if !self.delete_selection() {
            return false;
        }
        self.clipboard_feedback = Some(ClipboardFeedback::Cut(count));
        true
    }

    /// Inserts paste-ready copies through one history transaction, selects
    /// them, and makes the payload the one the Edit menu pastes next.
    fn paste_payload(&mut self, payload: ClipboardPayload) -> bool {
        if self.history_blocked() || !payload.is_supported() {
            self.clipboard_feedback = Some(ClipboardFeedback::Unsupported);
            return false;
        }
        // Every paste is offset so it never hides exactly under the source;
        // repeated pastes cascade instead of stacking.
        let step = PASTE_OFFSET * f64::from(self.paste_count + 1);
        let elements = payload.paste_elements(&self.document, Point::new(step, step));
        if elements.is_empty() {
            self.clipboard_feedback = Some(ClipboardFeedback::Unsupported);
            return false;
        }
        let count = elements.len();
        let ids: Vec<ElementId> = elements.iter().map(|element| element.id).collect();
        let index = self.document.elements.len();
        if !self.commit_transaction(Transaction::insert(index, elements)) {
            return false;
        }
        self.selected = ids.into_iter().collect();
        self.clipboard = Some(payload);
        self.paste_count = self.paste_count.saturating_add(1);
        self.clipboard_feedback = Some(ClipboardFeedback::Pasted(count));
        true
    }

    /// Pastes a system clipboard payload supplied by egui. Arbitrary text is
    /// ignored rather than inserted as an element.
    fn paste_text(&mut self, text: &str) -> bool {
        match ClipboardPayload::from_json(text) {
            Ok(payload) if payload.is_supported() => self.paste_payload(payload),
            Ok(_) | Err(_) => {
                self.clipboard_feedback = Some(ClipboardFeedback::Unsupported);
                false
            }
        }
    }

    /// Context-menu entries for the current target: selection commands when
    /// something is selected, otherwise only canvas-level commands that work.
    fn context_actions(&self) -> Vec<ContextAction> {
        if self.editing_text.is_some() {
            return Vec::new();
        }
        ContextAction::ALL
            .into_iter()
            .filter(|action| self.context_action_applies(*action))
            .collect()
    }

    fn context_action_applies(&self, action: ContextAction) -> bool {
        match action {
            ContextAction::Paste => self.can_paste(),
            // Select All only makes sense when the whole scene is not already
            // selected, which is exactly the empty-selection canvas menu.
            ContextAction::SelectAll => self.selected.is_empty() && self.can_select_all(),
            ContextAction::Group => self.can_group(),
            ContextAction::Ungroup => self.can_ungroup(),
            ContextAction::Cut
            | ContextAction::Copy
            | ContextAction::Duplicate
            | ContextAction::Delete
            | ContextAction::BringToFront
            | ContextAction::BringForward
            | ContextAction::SendBackward
            | ContextAction::SendToBack => self.can_copy(),
        }
    }

    /// Right-clicking an unselected element selects it (replacing the
    /// selection); right-clicking empty canvas clears the selection so the
    /// canvas menu appears. Right-clicking an already selected element keeps a
    /// multi-selection intact.
    fn update_context_selection(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        if self.editing_text.is_some() || !ctx.input(|input| input.pointer.secondary_pressed()) {
            return;
        }
        let Some(pointer) = ctx.pointer_hover_pos() else {
            return;
        };
        if !canvas_rect.contains(pointer) {
            return;
        }
        let world = self.screen_to_world(canvas_rect, pointer);
        let tolerance = HIT_TOLERANCE_PIXELS / self.viewport.zoom;
        let hit = self
            .document
            .paint_order()
            .into_iter()
            .rev()
            .find(|element| element.hit_test(world, tolerance))
            .map(|element| element.id);
        match hit {
            Some(id) => {
                if !self.selected.contains(&id) {
                    self.selected = self.document.expand_selection(&[id]).into_iter().collect();
                }
            }
            None => self.selected.clear(),
        }
    }

    fn context_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut z_order_started = false;
        for action in self.context_actions() {
            if action.is_z_order() && !z_order_started {
                ui.separator();
                z_order_started = true;
            }
            let mut button = egui::Button::new(action.label());
            if let Some(shortcut) = shortcut_for(action.action()) {
                button = button.shortcut_text(shortcut);
            }
            if ui.add(button).clicked() {
                self.perform_action(ctx, action.action(), Point::ZERO);
                ui.close();
            }
        }
    }

    /// Name shown in the title and status bar. Untitled documents have no path
    /// yet and are explicitly labelled.
    fn document_display_name(&self) -> String {
        self.file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned())
    }

    /// Dirty tracking compares against the snapshot at the last save/open, so
    /// undoing back to that state is clean again without mutation hooks.
    fn is_dirty(&self) -> bool {
        self.document != self.saved_document
    }

    fn window_title(&self) -> String {
        let marker = if self.is_dirty() { "• " } else { "" };
        format!("{marker}{} — Epsilon", self.document_display_name())
    }

    fn sync_window_title(&mut self, ctx: &egui::Context) {
        let title = self.window_title();
        if title != self.last_window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_window_title = title;
        }
    }

    /// Loads the newest valid autosave into the startup offer, if any. The
    /// file is not deleted until the user recovers or discards it.
    fn offer_recovery(&mut self) -> bool {
        let Some(store) = &self.recovery_store else {
            return false;
        };
        match store.load_latest() {
            Ok(Some((_, file))) => {
                self.recovery_offer = Some(file);
                true
            }
            Ok(None) => false,
            Err(error) => {
                self.file_feedback = Some(FileFeedback::Error(format!(
                    "Recovery unavailable: {error}"
                )));
                false
            }
        }
    }

    fn autosave_now(&mut self) -> bool {
        if !self.is_dirty() || self.history_blocked() || self.recovery_offer.is_some() {
            return false;
        }
        let Some(store) = &self.recovery_store else {
            return false;
        };
        let mut file = EpsilonFile::new(self.document.clone());
        file.app_state.viewport = self.viewport;
        if let Some(path) = &self.file_path {
            file.app_state.extras.insert(
                RECOVERY_SOURCE_KEY.to_owned(),
                serde_json::json!(path.to_string_lossy()),
            );
        }
        match store.save(&file) {
            Ok(_) => {
                self.last_autosave = Instant::now();
                true
            }
            Err(error) => {
                self.last_autosave = Instant::now();
                self.file_feedback = Some(FileFeedback::Error(format!("Autosave failed: {error}")));
                false
            }
        }
    }

    fn discard_recovery_files(&mut self) {
        if let Some(store) = &self.recovery_store
            && let Err(error) = store.discard_all()
        {
            self.file_feedback = Some(FileFeedback::Error(format!(
                "Could not remove recovery files: {error}"
            )));
        }
    }

    /// Applies a recovery document as unsaved work, restoring the source path
    /// when the autosave recorded one. History is reset because it cannot be
    /// reconstructed from the file.
    fn apply_recovery(&mut self) {
        let Some(file) = self.recovery_offer.take() else {
            return;
        };
        let source = file
            .app_state
            .extras
            .get(RECOVERY_SOURCE_KEY)
            .and_then(|value| value.as_str())
            .map(PathBuf::from);
        self.document = file.document;
        self.viewport = file.app_state.viewport;
        self.viewport.set_zoom(self.viewport.zoom);
        self.history.clear();
        self.history_error = None;
        self.property_edit_before = None;
        self.pointer_mode = PointerMode::Idle;
        self.transform_original.clear();
        self.selected.clear();
        self.editing_text = None;
        self.editing_text_is_new = false;
        self.text_buffer.clear();
        self.text_editor_needs_focus = false;
        self.paste_count = 0;
        self.file_path = source;
        // The recovered scene was not necessarily the last saved state, so it
        // starts dirty and prompts before overwriting any source file.
        self.saved_document = Document::new();
        self.file_feedback = Some(FileFeedback::Recovered("Recovered unsaved work".to_owned()));
        self.discard_recovery_files();
    }

    fn recovery_window(&mut self, ctx: &egui::Context) {
        if self.recovery_offer.is_none() {
            return;
        }
        let mut recover = false;
        let mut discard = false;
        egui::Window::new("Recover unsaved work?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Epsilon found an autosave from a previous session.");
                ui.horizontal(|ui| {
                    if ui.button("Recover").clicked() {
                        recover = true;
                    }
                    if ui.button("Discard").clicked() {
                        discard = true;
                    }
                });
            });
        if recover {
            self.apply_recovery();
        } else if discard {
            self.recovery_offer = None;
            self.discard_recovery_files();
            self.file_feedback = Some(FileFeedback::New("Discarded recovered work".to_owned()));
        }
    }

    /// Serializes the document and viewport to `path`. The destination is only
    /// replaced after the full JSON has been written and synced.
    fn save_to_path(&mut self, path: &Path) -> bool {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let mut file = EpsilonFile::new(self.document.clone());
        file.app_state.viewport = self.viewport;
        match file.save_to_path(path) {
            Ok(()) => {
                self.file_path = Some(path.to_path_buf());
                self.saved_document = self.document.clone();
                self.recent.record(path.to_path_buf());
                self.last_autosave = Instant::now();
                self.discard_recovery_files();
                self.file_feedback = Some(FileFeedback::Saved(format!("Saved {name}")));
                true
            }
            Err(error) => {
                self.file_feedback = Some(FileFeedback::Error(format!("Save failed: {error}")));
                false
            }
        }
    }

    /// Loads a validated document, replacing the current scene and resetting
    /// session-only state (history, selection, editor). On failure nothing in
    /// the current document changes.
    fn open_from_path(&mut self, path: &Path) -> bool {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        match EpsilonFile::load_from_path(path) {
            Ok(file) => {
                self.document = file.document;
                self.viewport = file.app_state.viewport;
                self.viewport.set_zoom(self.viewport.zoom);
                self.history.clear();
                self.history_error = None;
                self.property_edit_before = None;
                self.pointer_mode = PointerMode::Idle;
                self.transform_original.clear();
                self.selected.clear();
                self.editing_text = None;
                self.editing_text_is_new = false;
                self.text_buffer.clear();
                self.text_editor_needs_focus = false;
                self.paste_count = 0;
                self.file_path = Some(path.to_path_buf());
                self.saved_document = self.document.clone();
                self.recent.record(path.to_path_buf());
                self.last_autosave = Instant::now();
                self.discard_recovery_files();
                self.file_feedback = Some(FileFeedback::Opened(format!("Opened {name}")));
                true
            }
            Err(error) => {
                self.file_feedback = Some(FileFeedback::Error(format!("Open failed: {error}")));
                false
            }
        }
    }

    /// Quit asks the guard first; once resolved, `finish_pending_action`
    /// permits the compositor's close and sends the close command itself.
    fn request_quit(&mut self, ctx: &egui::Context) {
        if self.is_dirty() && !self.allow_close {
            self.pending_action = Some(PendingAction::Quit);
        } else {
            request_quit(ctx);
        }
    }

    fn request_new_document(&mut self) {
        if self.history_blocked() {
            self.file_feedback = Some(FileFeedback::Error(
                "Finish the current edit before starting a new document".to_owned(),
            ));
            return;
        }
        if self.is_dirty() {
            self.pending_action = Some(PendingAction::New);
        } else {
            self.new_document();
        }
    }

    /// Replaces the scene with an empty document and clears session state.
    fn new_document(&mut self) {
        self.document = Document::new();
        self.saved_document = self.document.clone();
        self.viewport = Viewport::default();
        self.history.clear();
        self.history_error = None;
        self.property_edit_before = None;
        self.pointer_mode = PointerMode::Idle;
        self.transform_original.clear();
        self.selected.clear();
        self.editing_text = None;
        self.editing_text_is_new = false;
        self.text_buffer.clear();
        self.text_editor_needs_focus = false;
        self.paste_count = 0;
        self.file_path = None;
        self.pending_action = None;
        self.last_autosave = Instant::now();
        self.discard_recovery_files();
        self.file_feedback = Some(FileFeedback::New("New document".to_owned()));
    }

    /// Imports an Excalidraw scene as a new unsaved document. Unsupported
    /// element types are reported as warnings without aborting the import.
    fn import_excalidraw_from_path(&mut self, path: &Path) -> bool {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        match excalidraw::import_from_path(path) {
            Ok(imported) => {
                let count = imported.document.elements.len();
                let skipped = imported.warnings.len();
                self.document = imported.document;
                self.viewport = imported.viewport;
                self.viewport.set_zoom(self.viewport.zoom);
                self.history.clear();
                self.history_error = None;
                self.property_edit_before = None;
                self.pointer_mode = PointerMode::Idle;
                self.transform_original.clear();
                self.selected.clear();
                self.editing_text = None;
                self.editing_text_is_new = false;
                self.text_buffer.clear();
                self.text_editor_needs_focus = false;
                self.paste_count = 0;
                self.file_path = None;
                self.saved_document = Document::new();
                self.last_autosave = Instant::now();
                self.discard_recovery_files();
                self.file_feedback = Some(FileFeedback::New(if skipped == 0 {
                    format!("Imported {count} elements from {name}")
                } else {
                    format!("Imported {count} elements from {name} ({skipped} unsupported skipped)")
                }));
                true
            }
            Err(error) => {
                self.file_feedback = Some(FileFeedback::Error(format!("Import failed: {error}")));
                false
            }
        }
    }

    fn request_import_excalidraw(&mut self) {
        if self.history_blocked() {
            self.file_feedback = Some(FileFeedback::Error(
                "Finish the current edit before importing a file".to_owned(),
            ));
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Excalidraw scene", &["excalidraw", "json"])
            .set_title("Import Excalidraw scene")
            .pick_file()
        else {
            return;
        };
        if self.is_dirty() {
            self.pending_action = Some(PendingAction::Import(path));
        } else {
            self.import_excalidraw_from_path(&path);
        }
    }

    fn export_excalidraw_to_path(&mut self, path: &Path) -> bool {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        match excalidraw::export_to_path(&self.document, &self.viewport, path) {
            Ok(()) => {
                self.file_feedback = Some(FileFeedback::Saved(format!("Exported {name}")));
                true
            }
            Err(error) => {
                self.file_feedback = Some(FileFeedback::Error(format!("Export failed: {error}")));
                false
            }
        }
    }

    fn export_excalidraw_dialog(&mut self) {
        if self.history_blocked() {
            self.file_feedback = Some(FileFeedback::Error(
                "Finish the current edit before exporting".to_owned(),
            ));
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Excalidraw scene", &["excalidraw"])
            .set_title("Export Excalidraw scene")
            .set_file_name("untitled.excalidraw");
        if let Some(parent) = self.file_path.as_ref().and_then(|path| path.parent()) {
            dialog = dialog.set_directory(parent);
        }
        let Some(path) = dialog.save_file() else {
            return;
        };
        self.export_excalidraw_to_path(&with_extension_if_needed(path, "excalidraw"));
    }

    fn export_png_to_path(&mut self, path: &Path) -> bool {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let options = ExportOptions {
            scale: ExportScale::One,
            rough: !self.clean_rendering,
            ..ExportOptions::default()
        }
        .with_document_background(&self.document);
        let rendered = match crate::export::render_png(&self.document, &options) {
            Ok(rendered) => rendered,
            Err(error) => {
                self.file_feedback = Some(FileFeedback::Error(format!("Export failed: {error}")));
                return false;
            }
        };
        match write_bytes_atomically(path, &rendered.bytes) {
            Ok(()) => {
                self.file_feedback = Some(FileFeedback::Saved(format!(
                    "Exported {name} ({}×{})",
                    rendered.size.width, rendered.size.height
                )));
                true
            }
            Err(error) => {
                self.file_feedback = Some(FileFeedback::Error(format!("Export failed: {error}")));
                false
            }
        }
    }

    fn export_png_dialog(&mut self) {
        if self.history_blocked() {
            self.file_feedback = Some(FileFeedback::Error(
                "Finish the current edit before exporting".to_owned(),
            ));
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_title("Export PNG image")
            .set_file_name("untitled.png");
        if let Some(parent) = self.file_path.as_ref().and_then(|path| path.parent()) {
            dialog = dialog.set_directory(parent);
        }
        let Some(path) = dialog.save_file() else {
            return;
        };
        self.export_png_to_path(&with_extension_if_needed(path, "png"));
    }

    fn request_open_document(&mut self) {
        if self.history_blocked() {
            self.file_feedback = Some(FileFeedback::Error(
                "Finish the current edit before opening a file".to_owned(),
            ));
            return;
        }
        if self.is_dirty() {
            self.pending_action = Some(PendingAction::Open(None));
        } else {
            self.open_document_dialog();
        }
    }

    /// Opens a recent entry after the shared unsaved guard. Stale entries are
    /// removed and reported instead of attempting a read.
    fn open_recent(&mut self, path: &Path) {
        if self.history_blocked() {
            self.file_feedback = Some(FileFeedback::Error(
                "Finish the current edit before opening a file".to_owned(),
            ));
            return;
        }
        if !path.is_file() {
            self.recent.remove(path);
            self.file_feedback = Some(FileFeedback::Error(format!(
                "File no longer exists: {}",
                path.display()
            )));
            return;
        }
        if self.is_dirty() {
            self.pending_action = Some(PendingAction::Open(Some(path.to_path_buf())));
        } else {
            self.open_from_path(path);
        }
    }

    fn open_document_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Epsilon document", &[FILE_EXTENSION])
            .set_title("Open Epsilon document")
            .pick_file()
        else {
            return;
        };
        self.open_from_path(&path);
    }

    fn save_document(&mut self) -> bool {
        if self.history_blocked() {
            self.file_feedback = Some(FileFeedback::Error(
                "Finish the current edit before saving".to_owned(),
            ));
            return false;
        }
        match self.file_path.clone() {
            Some(path) => self.save_to_path(&path),
            None => self.save_document_as(),
        }
    }

    fn save_document_as(&mut self) -> bool {
        if self.history_blocked() {
            self.file_feedback = Some(FileFeedback::Error(
                "Finish the current edit before saving".to_owned(),
            ));
            return false;
        }
        let default_name = if self.file_path.is_some() {
            self.document_display_name()
        } else {
            format!("untitled.{FILE_EXTENSION}")
        };
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Epsilon document", &[FILE_EXTENSION])
            .set_title("Save Epsilon document")
            .set_file_name(default_name);
        if let Some(parent) = self.file_path.as_ref().and_then(|path| path.parent()) {
            dialog = dialog.set_directory(parent);
        }
        let Some(path) = dialog.save_file() else {
            return false;
        };
        self.save_to_path(&with_document_extension(path))
    }

    /// Applies the unsaved-changes choice. Saving that is cancelled (or fails)
    /// keeps the prompt open and leaves the document untouched.
    fn resolve_unsaved(&mut self, ctx: &egui::Context, choice: UnsavedChoice) {
        match choice {
            UnsavedChoice::Cancel => self.pending_action = None,
            UnsavedChoice::Discard => self.finish_pending_action(ctx),
            UnsavedChoice::Save => {
                if self.save_document() {
                    self.finish_pending_action(ctx);
                }
            }
        }
    }

    fn finish_pending_action(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_action.take() else {
            return;
        };
        match action {
            PendingAction::New => self.new_document(),
            PendingAction::Open(None) => self.open_document_dialog(),
            PendingAction::Open(Some(path)) => {
                self.open_from_path(&path);
            }
            PendingAction::Import(path) => {
                self.import_excalidraw_from_path(&path);
            }
            PendingAction::Quit => {
                self.allow_close = true;
                self.discard_recovery_files();
                request_quit(ctx);
            }
        }
    }

    fn welcome_window(&mut self, ctx: &egui::Context) {
        if !self.welcome_open
            || self.is_dirty()
            || !self.document.elements.is_empty()
            || self.file_path.is_some()
            || self.pending_action.is_some()
            || self.recovery_offer.is_some()
        {
            return;
        }
        let recent = self.recent.clone();
        let mut open = None;
        let mut dismiss = false;
        egui::Window::new("Welcome to Epsilon")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Draw with the toolbar or press a tool shortcut.");
                ui.label(
                    RichText::new("Open or create a document from the File menu.")
                        .small()
                        .weak(),
                );
                if !recent.is_empty() {
                    ui.separator();
                    ui.label("Recent documents");
                    for path in recent.paths() {
                        let name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.display().to_string());
                        if ui
                            .button(name)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            open = Some(path.clone());
                        }
                    }
                }
                ui.separator();
                if ui.button("Close").clicked() {
                    dismiss = true;
                }
            });
        if let Some(path) = open {
            self.welcome_open = false;
            self.open_recent(&path);
        } else if dismiss {
            self.welcome_open = false;
        }
    }

    fn unsaved_changes_window(&mut self, ctx: &egui::Context) {
        if self.pending_action.is_none() {
            return;
        }
        let mut choice = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!("Save changes to {}?", self.document_display_name()));
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(UnsavedChoice::Save);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(UnsavedChoice::Discard);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(UnsavedChoice::Cancel);
                    }
                });
            });
        if let Some(choice) = choice {
            self.resolve_unsaved(ctx, choice);
        }
    }

    fn paste_from_menu(&mut self) -> bool {
        match self.clipboard.clone() {
            Some(payload) => self.paste_payload(payload),
            None => false,
        }
    }

    fn can_select_all(&self) -> bool {
        self.editing_text.is_none() && !self.document.elements.is_empty()
    }

    /// Grouping needs at least two selected elements; a selection that is
    /// already one whole group can still be wrapped in an outer group.
    fn can_group(&self) -> bool {
        self.editing_text.is_none() && self.selected.len() >= 2
    }

    /// Alignment needs at least two selected elements to have a shared edge.
    fn can_align(&self) -> bool {
        self.editing_text.is_none() && self.selected.len() >= 2
    }

    /// Distribution needs three selected elements to create equal gaps.
    fn can_distribute(&self) -> bool {
        self.editing_text.is_none() && self.selected.len() >= 3
    }

    /// Ungrouping needs at least one selected element that belongs to a group.
    fn can_ungroup(&self) -> bool {
        self.editing_text.is_none()
            && self.selected.iter().any(|id| {
                self.document
                    .element(*id)
                    .is_some_and(|element| !element.group_ids.is_empty())
            })
    }

    fn select_all(&mut self) {
        if !self.can_select_all() || self.history_blocked() {
            return;
        }
        self.selected = self
            .document
            .elements
            .iter()
            .map(|element| element.id)
            .collect();
    }

    /// Copies the selection one grid step away and selects the copies. Reuses
    /// the clipboard payload so duplicates get fresh IDs and new paint slots.
    fn duplicate_selection(&mut self) -> bool {
        if !self.can_copy() || self.history_blocked() {
            return false;
        }
        let payload = ClipboardPayload::new(self.selection_elements());
        let offset = Point::new(grid::DEFAULT_GRID_SIZE, grid::DEFAULT_GRID_SIZE);
        let elements = payload.paste_elements(&self.document, offset);
        if elements.is_empty() {
            return false;
        }
        let ids: Vec<ElementId> = elements.iter().map(|element| element.id).collect();
        let index = self.document.elements.len();
        if !self.commit_transaction(Transaction::insert(index, elements)) {
            return false;
        }
        self.selected = ids.into_iter().collect();
        true
    }

    /// Selected IDs in document order.
    fn selected_ids(&self) -> Vec<ElementId> {
        self.document
            .elements
            .iter()
            .filter(|element| self.selected.contains(&element.id))
            .map(|element| element.id)
            .collect()
    }

    /// Wraps the selection in a new outermost group. The selection is expanded
    /// to whole groups first, so nested groups cannot be split. One history
    /// transaction records the change and the selection is preserved.
    fn group_selection(&mut self) -> bool {
        if !self.can_group() || self.history_blocked() {
            return false;
        }
        let ids = self.document.expand_selection(&self.selected_ids());
        self.selected = ids.iter().copied().collect();
        let changes = self.document.group_changes(&ids);
        if changes.is_empty() {
            return false;
        }
        let (before, after): (Vec<Element>, Vec<Element>) = changes.into_iter().unzip();
        match Transaction::replace(before, after) {
            Ok(transaction) => self.commit_transaction(transaction),
            Err(error) => {
                self.history_error = Some(format!("Edit failed: {error}"));
                false
            }
        }
    }

    /// Removes one grouping level from the selection. The selection is
    /// expanded to whole groups first, so no group is left partially grouped.
    fn ungroup_selection(&mut self) -> bool {
        if !self.can_ungroup() || self.history_blocked() {
            return false;
        }
        let ids = self.document.expand_selection(&self.selected_ids());
        self.selected = ids.iter().copied().collect();
        let changes = self.document.ungroup_changes(&ids);
        if changes.is_empty() {
            return false;
        }
        let (before, after): (Vec<Element>, Vec<Element>) = changes.into_iter().unzip();
        match Transaction::replace(before, after) {
            Ok(transaction) => self.commit_transaction(transaction),
            Err(error) => {
                self.history_error = Some(format!("Edit failed: {error}"));
                false
            }
        }
    }

    /// Aligns the whole selection on one world-space edge as a single history
    /// transaction, preserving the selection.
    fn align_selection(&mut self, align: ElementAlign) -> bool {
        if !self.can_align() || self.history_blocked() {
            return false;
        }
        let changes = self.document.align_changes(&self.selected_ids(), align);
        if changes.is_empty() {
            return false;
        }
        let (before, after): (Vec<Element>, Vec<Element>) = changes.into_iter().unzip();
        match Transaction::replace(before, after) {
            Ok(transaction) => self.commit_transaction(transaction),
            Err(error) => {
                self.history_error = Some(format!("Edit failed: {error}"));
                false
            }
        }
    }

    /// Distributes the selection with equal gaps along one axis as a single
    /// history transaction, preserving the selection.
    fn distribute_selection(&mut self, axis: ElementDistribute) -> bool {
        if !self.can_distribute() || self.history_blocked() {
            return false;
        }
        let changes = self.document.distribute_changes(&self.selected_ids(), axis);
        if changes.is_empty() {
            return false;
        }
        let (before, after): (Vec<Element>, Vec<Element>) = changes.into_iter().unzip();
        match Transaction::replace(before, after) {
            Ok(transaction) => self.commit_transaction(transaction),
            Err(error) => {
                self.history_error = Some(format!("Edit failed: {error}"));
                false
            }
        }
    }

    /// Escape drops the selection and returns to the Select tool. It is a no-op
    /// while an edit is in flight so it cannot interrupt a drag or the text
    /// editor (which handles Escape itself).
    fn clear_selection(&mut self) {
        if self.history_blocked() {
            return;
        }
        self.selected.clear();
        self.tool = Tool::Select;
    }

    /// Moves the selection in paint order through one history transaction and
    /// leaves the selection itself untouched.
    fn move_selection_in_z_order(&mut self, movement: ZOrderMove) -> bool {
        if !self.can_copy() || self.history_blocked() {
            return false;
        }
        let ids: Vec<ElementId> = self
            .selection_elements()
            .iter()
            .map(|element| element.id)
            .collect();
        let changes = self.document.z_order_changes(&ids, movement);
        if changes.is_empty() {
            return false;
        }
        let (before, after): (Vec<Element>, Vec<Element>) = changes.into_iter().unzip();
        match Transaction::replace(before, after) {
            Ok(transaction) => self.commit_transaction(transaction),
            Err(error) => {
                self.history_error = Some(format!("Edit failed: {error}"));
                false
            }
        }
    }

    fn commit_transaction(&mut self, transaction: Transaction) -> bool {
        match self.history.commit(&mut self.document, transaction) {
            Ok(()) => {
                self.history_error = None;
                true
            }
            Err(error) => {
                self.history_error = Some(format!("Edit failed: {error}"));
                false
            }
        }
    }

    /// Inserts a new element through history, assigning it the next z-order.
    fn insert_element(&mut self, mut element: Element) -> Option<ElementId> {
        element.z_order = self.document.next_z_order();
        let id = element.id;
        let index = self.document.elements.len();
        self.commit_transaction(Transaction::insert(index, vec![element]))
            .then_some(id)
    }

    /// History is unavailable while an edit is still in flight: the text editor
    /// records on commit, and drag snapshots must finish before they can be
    /// reverted or the next frame would reapply them.
    fn history_blocked(&self) -> bool {
        self.editing_text.is_some()
            || !matches!(self.pointer_mode, PointerMode::Idle)
            || self.property_edit_before.is_some()
    }

    fn undo(&mut self) -> bool {
        if self.history_blocked() {
            return false;
        }
        match self.history.undo(&mut self.document) {
            Ok(changed) => {
                if changed {
                    self.history_error = None;
                    self.prune_after_history();
                }
                changed
            }
            Err(error) => {
                self.history_error = Some(format!("Undo failed: {error}"));
                false
            }
        }
    }

    fn redo(&mut self) -> bool {
        if self.history_blocked() {
            return false;
        }
        match self.history.redo(&mut self.document) {
            Ok(changed) => {
                if changed {
                    self.history_error = None;
                    self.prune_after_history();
                }
                changed
            }
            Err(error) => {
                self.history_error = Some(format!("Redo failed: {error}"));
                false
            }
        }
    }

    /// Drops selection and editor references to elements history removed.
    fn prune_after_history(&mut self) {
        self.selected
            .retain(|id| self.document.element(*id).is_some());
        if let Some(id) = self.editing_text
            && self.document.element(id).is_none()
        {
            self.editing_text = None;
            self.editing_text_is_new = false;
            self.text_buffer.clear();
            self.text_editor_needs_focus = false;
        }
    }

    /// Records the net effect of a finished move, resize or rotation drag.
    fn finish_transform(&mut self) {
        let originals = std::mem::take(&mut self.transform_original);
        if originals.is_empty() {
            return;
        }

        let mut before = Vec::new();
        let mut after = Vec::new();
        for original in originals {
            let Some(current) = self.document.element(original.id) else {
                continue;
            };
            if current != &original {
                before.push(original);
                after.push(current.clone());
            }
        }
        if before.is_empty() {
            return;
        }

        match Transaction::replace(before, after) {
            Ok(transaction) => {
                self.commit_transaction(transaction);
            }
            Err(error) => self.history_error = Some(format!("Edit failed: {error}")),
        }
    }

    /// Records a finished properties-panel edit against its before snapshot.
    fn commit_property_edit(&mut self, before: Vec<Element>) {
        let mut originals = Vec::new();
        let mut after = Vec::new();
        for original in before {
            let Some(current) = self.document.element(original.id) else {
                continue;
            };
            if current != &original {
                originals.push(original);
                after.push(current.clone());
            }
        }
        if originals.is_empty() {
            return;
        }

        match Transaction::replace(originals, after) {
            Ok(transaction) => {
                self.commit_transaction(transaction);
            }
            Err(error) => self.history_error = Some(format!("Edit failed: {error}")),
        }
    }

    /// The single theme control used by both the View menu and the toolbar.
    fn theme_toggle(&mut self, ui: &mut egui::Ui) {
        let mut dark = self.theme == Theme::Dark;
        if ui.checkbox(&mut dark, "Dark mode").changed() {
            self.toggle_theme(ui.ctx());
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu_bar")
            .exact_size(layout::MENU_BAR_HEIGHT)
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    // Only actions that already work are listed. File/Edit will
                    // grow when document and editing stages land.
                    ui.menu_button("File", |ui| {
                        if ui
                            .add(egui::Button::new("New").shortcut_text("Ctrl+N"))
                            .clicked()
                        {
                            self.request_new_document();
                            ui.close();
                        }
                        if ui
                            .add(egui::Button::new("Open…").shortcut_text("Ctrl+O"))
                            .clicked()
                        {
                            self.request_open_document();
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add(egui::Button::new("Save").shortcut_text("Ctrl+S"))
                            .clicked()
                        {
                            self.save_document();
                            ui.close();
                        }
                        if ui
                            .add(egui::Button::new("Save As…").shortcut_text("Ctrl+Shift+S"))
                            .clicked()
                        {
                            self.save_document_as();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Import Excalidraw…").clicked() {
                            self.request_import_excalidraw();
                            ui.close();
                        }
                        if ui.button("Export Excalidraw…").clicked() {
                            self.export_excalidraw_dialog();
                            ui.close();
                        }
                        if ui.button("Export PNG…").clicked() {
                            self.export_png_dialog();
                            ui.close();
                        }
                        if !self.recent.is_empty() {
                            ui.separator();
                            let recent = self.recent.clone();
                            ui.menu_button("Open Recent", |ui| {
                                for path in recent.paths() {
                                    let name = path
                                        .file_name()
                                        .map(|name| name.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| path.display().to_string());
                                    if ui
                                        .button(name)
                                        .on_hover_text(path.display().to_string())
                                        .clicked()
                                    {
                                        self.open_recent(path);
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui.button("Clear Recent").clicked() {
                                    self.recent = RecentFiles::new();
                                    ui.close();
                                }
                            });
                        }
                        ui.separator();
                        if ui.button("Quit").clicked() {
                            self.request_quit(ui.ctx());
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        if ui
                            .add_enabled(
                                self.can_undo(),
                                egui::Button::new("Undo").shortcut_text("Ctrl+Z"),
                            )
                            .clicked()
                        {
                            self.undo();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.can_redo(),
                                egui::Button::new("Redo").shortcut_text("Ctrl+Shift+Z"),
                            )
                            .clicked()
                        {
                            self.redo();
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.can_copy(),
                                egui::Button::new("Cut").shortcut_text("Ctrl+X"),
                            )
                            .clicked()
                        {
                            self.cut_selection(ui.ctx());
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.can_copy(),
                                egui::Button::new("Copy").shortcut_text("Ctrl+C"),
                            )
                            .clicked()
                        {
                            self.copy_selection(ui.ctx());
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.can_paste(),
                                egui::Button::new("Paste").shortcut_text("Ctrl+V"),
                            )
                            .clicked()
                        {
                            self.paste_from_menu();
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.can_select_all(),
                                egui::Button::new("Select All").shortcut_text("Ctrl+A"),
                            )
                            .clicked()
                        {
                            self.select_all();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.can_copy(),
                                egui::Button::new("Duplicate").shortcut_text("Ctrl+D"),
                            )
                            .clicked()
                        {
                            self.duplicate_selection();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.can_copy(),
                                egui::Button::new("Delete").shortcut_text("Del"),
                            )
                            .clicked()
                        {
                            self.delete_selection();
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.can_group(),
                                egui::Button::new("Group").shortcut_text("Ctrl+G"),
                            )
                            .clicked()
                        {
                            self.group_selection();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.can_ungroup(),
                                egui::Button::new("Ungroup").shortcut_text("Ctrl+Shift+G"),
                            )
                            .clicked()
                        {
                            self.ungroup_selection();
                            ui.close();
                        }
                        ui.separator();
                        ui.menu_button("Order", |ui| {
                            for (label, shortcut, movement) in [
                                ("Bring to Front", "Ctrl+Shift+]", ZOrderMove::Front),
                                ("Bring Forward", "Ctrl+]", ZOrderMove::Forward),
                                ("Send Backward", "Ctrl+[", ZOrderMove::Backward),
                                ("Send to Back", "Ctrl+Shift+[", ZOrderMove::Back),
                            ] {
                                if ui
                                    .add_enabled(
                                        self.can_copy(),
                                        egui::Button::new(label).shortcut_text(shortcut),
                                    )
                                    .clicked()
                                {
                                    self.move_selection_in_z_order(movement);
                                    ui.close();
                                }
                            }
                        });
                        ui.menu_button("Arrange", |ui| {
                            for (label, align) in [
                                ("Align Left", ElementAlign::Left),
                                ("Align Center", ElementAlign::CenterX),
                                ("Align Right", ElementAlign::Right),
                                ("Align Top", ElementAlign::Top),
                                ("Align Middle", ElementAlign::CenterY),
                                ("Align Bottom", ElementAlign::Bottom),
                            ] {
                                if ui
                                    .add_enabled(self.can_align(), egui::Button::new(label))
                                    .clicked()
                                {
                                    self.align_selection(align);
                                    ui.close();
                                }
                            }
                            ui.separator();
                            for (label, axis) in [
                                ("Distribute Horizontally", ElementDistribute::Horizontal),
                                ("Distribute Vertically", ElementDistribute::Vertical),
                            ] {
                                if ui
                                    .add_enabled(self.can_distribute(), egui::Button::new(label))
                                    .clicked()
                                {
                                    self.distribute_selection(axis);
                                    ui.close();
                                }
                            }
                        });
                    });
                    ui.menu_button("View", |ui| {
                        self.theme_toggle(ui);
                        let mut grid = self.grid_enabled;
                        if ui.checkbox(&mut grid, "Grid").changed() {
                            self.grid_enabled = grid;
                        }
                        let mut clean = self.clean_rendering;
                        if ui.checkbox(&mut clean, "Clean rendering").changed() {
                            self.clean_rendering = clean;
                        }
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("About Epsilon").clicked() {
                            self.open_about();
                            ui.close();
                        }
                    });
                });
            });
    }

    fn top_toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_toolbar")
            .exact_size(layout::TOP_TOOLBAR_HEIGHT)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("EPSILON").heading().strong());
                    ui.add_space(4.0);
                    ui.label(RichText::new("Untitled").weak());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("About").clicked() {
                            self.open_about();
                        }
                        self.theme_toggle(ui);
                        ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).weak());
                        if ui
                            .add_enabled(self.can_undo(), egui::Button::new("Undo"))
                            .clicked()
                        {
                            self.undo();
                        }
                        if ui
                            .add_enabled(self.can_redo(), egui::Button::new("Redo"))
                            .clicked()
                        {
                            self.redo();
                        }

                        let anchor = if self.canvas_rect == egui::Rect::NOTHING {
                            Point::new(0.0, 0.0)
                        } else {
                            pointer_anchor(ui.ctx(), self.canvas_rect)
                        };
                        if ui.button("-").clicked() {
                            self.viewport.zoom_by(0.8, anchor);
                        }
                        if ui.button("+").clicked() {
                            self.viewport.zoom_by(1.25, anchor);
                        }
                        if ui.button("0").clicked() {
                            self.viewport = Viewport::default();
                        }
                    });
                });
            });
    }

    fn tool_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("tool_panel")
            .resizable(false)
            .exact_size(layout::TOOL_RAIL_WIDTH)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("TOOLS").small().strong());
                });
                ui.add_space(2.0);
                ui.separator();
                ui.add_space(4.0);

                for tool in Tool::ALL {
                    let selected = self.tool == tool;
                    let clicked = ui
                        .vertical_centered(|ui| tools::tool_button(ui, tool, selected).clicked())
                        .inner;
                    if clicked {
                        self.tool = tool;
                    }
                    ui.add_space(4.0);
                }
            });
    }

    /// Applies registered keyboard shortcuts, unless a widget currently owns
    /// keyboard input (a text field, for example). Undo and redo stay available
    /// when a properties field has focus, but not while canvas text is edited.
    fn handle_shortcuts(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        let wants_keyboard = ctx.egui_wants_keyboard_input();
        let events: Vec<egui::Event> = ctx.input(|input| input.events.clone());
        for event in events {
            // winit translates Ctrl+C/X/V into clipboard events instead of key
            // presses, so these are the real native shortcut path. A focused
            // text widget keeps its own clipboard handling.
            match event {
                egui::Event::Copy if !wants_keyboard && !self.help_open => {
                    self.copy_selection(ctx);
                    continue;
                }
                egui::Event::Cut if !wants_keyboard && !self.help_open => {
                    self.cut_selection(ctx);
                    continue;
                }
                egui::Event::Paste(text) if !wants_keyboard && !self.help_open => {
                    self.paste_text(&text);
                    continue;
                }
                egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => continue,
                _ => {}
            }

            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                continue;
            };

            let binding = KEYBINDINGS
                .iter()
                .find(|binding| binding.key == key && modifiers.matches_exact(binding.modifiers));

            if wants_keyboard {
                // A focused widget (a DragValue, for example) would otherwise
                // swallow the undo shortcut. The canvas text editor keeps its
                // own undo stack, so document history stays out of its way.
                if !self.help_open
                    && self.editing_text.is_none()
                    && let Some(binding) = binding
                    && matches!(binding.action, Action::Undo | Action::Redo)
                {
                    self.perform_action(ctx, binding.action, pointer_anchor(ctx, canvas_rect));
                }
                continue;
            }

            // Help overlay toggles and closes are handled even while it is open.
            if key == egui::Key::F1 || key == egui::Key::Questionmark {
                if binding.is_some() {
                    self.help_open = !self.help_open;
                }
                continue;
            }
            if key == egui::Key::Escape && self.help_open {
                self.help_open = false;
                continue;
            }

            // Ignore tool and other shortcuts while the help overlay is open.
            if self.help_open {
                continue;
            }

            if let Some(binding) = binding {
                self.perform_action(ctx, binding.action, pointer_anchor(ctx, canvas_rect));
            }
        }
    }

    fn perform_action(&mut self, ctx: &egui::Context, action: Action, anchor: Point) {
        match action {
            Action::SelectTool(tool) => self.tool = tool,
            Action::ToggleHelp => self.help_open = !self.help_open,
            Action::ZoomIn => self.viewport.zoom_by(1.25, anchor),
            Action::ZoomOut => self.viewport.zoom_by(0.8, anchor),
            Action::ZoomReset => self.viewport = Viewport::default(),
            Action::ZoomFit => self.viewport = Viewport::default(),
            Action::Undo => {
                self.undo();
            }
            Action::Redo => {
                self.redo();
            }
            Action::Cut => {
                self.cut_selection(ctx);
            }
            Action::Copy => {
                self.copy_selection(ctx);
            }
            Action::Paste => {
                self.paste_from_menu();
            }
            Action::New => {
                self.request_new_document();
            }
            Action::Open => {
                self.request_open_document();
            }
            Action::Save => {
                self.save_document();
            }
            Action::SaveAs => {
                self.save_document_as();
            }
            Action::Delete => {
                self.delete_selection();
            }
            Action::Duplicate => {
                self.duplicate_selection();
            }
            Action::SelectAll => {
                self.select_all();
            }
            Action::ClearSelection => {
                self.clear_selection();
            }
            Action::Group => {
                self.group_selection();
            }
            Action::Ungroup => {
                self.ungroup_selection();
            }
            Action::ZOrder(movement) => {
                self.move_selection_in_z_order(movement);
            }
        }
    }

    fn selection_properties(
        document: &Document,
        selected: &HashSet<ElementId>,
    ) -> Option<SelectionProperties> {
        let elements: Vec<&Element> = document
            .elements
            .iter()
            .filter(|element| selected.contains(&element.id))
            .collect();
        if elements.is_empty() {
            return None;
        }
        Some(SelectionProperties {
            x: uniform_value(elements.iter().map(|element| element.x)),
            y: uniform_value(elements.iter().map(|element| element.y)),
            width: uniform_value(elements.iter().map(|element| element.width)),
            height: uniform_value(elements.iter().map(|element| element.height)),
            angle_degrees: uniform_value(
                elements
                    .iter()
                    .map(|element| element_property_angle(element).to_degrees()),
            ),
        })
    }

    fn set_selection_x(document: &mut Document, selected: &HashSet<ElementId>, value: f64) {
        let Some(first) = document
            .elements
            .iter()
            .find(|element| selected.contains(&element.id))
        else {
            return;
        };
        let delta = Point::new(value - first.x, 0.0);
        for element in document
            .elements
            .iter_mut()
            .filter(|element| selected.contains(&element.id))
        {
            *element = element.translated(delta);
        }
    }

    fn set_selection_y(document: &mut Document, selected: &HashSet<ElementId>, value: f64) {
        let Some(first) = document
            .elements
            .iter()
            .find(|element| selected.contains(&element.id))
        else {
            return;
        };
        let delta = Point::new(0.0, value - first.y);
        for element in document
            .elements
            .iter_mut()
            .filter(|element| selected.contains(&element.id))
        {
            *element = element.translated(delta);
        }
    }

    fn set_selection_width(document: &mut Document, selected: &HashSet<ElementId>, value: f64) {
        for element in document
            .elements
            .iter_mut()
            .filter(|element| selected.contains(&element.id))
        {
            let from = element.bounds();
            let to = Rect::from_xywh(from.min.x, from.min.y, value.max(1.0), from.height());
            *element = element.scaled_from(from, to);
        }
    }

    fn set_selection_height(document: &mut Document, selected: &HashSet<ElementId>, value: f64) {
        for element in document
            .elements
            .iter_mut()
            .filter(|element| selected.contains(&element.id))
        {
            let from = element.bounds();
            let to = Rect::from_xywh(from.min.x, from.min.y, from.width(), value.max(1.0));
            *element = element.scaled_from(from, to);
        }
    }

    fn set_selection_angle(document: &mut Document, selected: &HashSet<ElementId>, degrees: f64) {
        let target = degrees.to_radians();
        for element in document
            .elements
            .iter_mut()
            .filter(|element| selected.contains(&element.id))
        {
            if matches!(element.kind, ElementKind::Line | ElementKind::Arrow)
                && element.points.len() == 2
            {
                element.angle += target - element_property_angle(element);
            } else {
                element.angle = target;
            }
        }
    }

    fn transform_controls(
        ui: &mut egui::Ui,
        document: &mut Document,
        selected: &HashSet<ElementId>,
    ) -> bool {
        let Some(properties) = Self::selection_properties(document, selected) else {
            return false;
        };
        let mut changed = false;
        let first = document
            .elements
            .iter()
            .find(|element| selected.contains(&element.id));
        let (first_x, first_y, first_width, first_height, first_angle) =
            first.map_or((0.0, 0.0, 1.0, 1.0, 0.0), |element| {
                (
                    element.x,
                    element.y,
                    element.width,
                    element.height,
                    element_property_angle(element).to_degrees(),
                )
            });

        ui.label(RichText::new("Transform").strong());
        ui.horizontal(|ui| {
            if let Some(value) = scalar_field(
                ui,
                "X",
                properties.x.unwrap_or(first_x),
                properties.x.is_none(),
                1.0,
                -1_000_000.0..=1_000_000.0,
                "",
            ) {
                Self::set_selection_x(document, selected, value);
                changed = true;
            }
            if let Some(value) = scalar_field(
                ui,
                "Y",
                properties.y.unwrap_or(first_y),
                properties.y.is_none(),
                1.0,
                -1_000_000.0..=1_000_000.0,
                "",
            ) {
                Self::set_selection_y(document, selected, value);
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            if let Some(value) = scalar_field(
                ui,
                "W",
                properties.width.unwrap_or(first_width),
                properties.width.is_none(),
                1.0,
                1.0..=1_000_000.0,
                "",
            ) {
                Self::set_selection_width(document, selected, value);
                changed = true;
            }
            if let Some(value) = scalar_field(
                ui,
                "H",
                properties.height.unwrap_or(first_height),
                properties.height.is_none(),
                1.0,
                1.0..=1_000_000.0,
                "",
            ) {
                Self::set_selection_height(document, selected, value);
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            if let Some(value) = scalar_field(
                ui,
                "Angle",
                properties.angle_degrees.unwrap_or(first_angle),
                properties.angle_degrees.is_none(),
                1.0,
                -360.0..=360.0,
                "°",
            ) {
                Self::set_selection_angle(document, selected, value);
                changed = true;
            }
        });
        ui.add_space(4.0);
        changed
    }

    /// Style controls applied to every selected element.
    fn style_controls(
        ui: &mut egui::Ui,
        document: &mut Document,
        selected: &HashSet<ElementId>,
    ) -> bool {
        let styles: Vec<epsilon_core::document::ElementStyle> = document
            .elements
            .iter()
            .filter(|element| selected.contains(&element.id))
            .map(|element| element.style)
            .collect();
        let Some(&first_style) = styles.first() else {
            return false;
        };
        let mut style = first_style;
        let mut changed = false;
        let mixed_stroke = styles.iter().any(|value| value.stroke != style.stroke);
        let mixed_fill = styles.iter().any(|value| value.fill != style.fill);
        let mixed_width = styles
            .iter()
            .any(|value| (value.stroke_width - style.stroke_width).abs() > 1e-6);
        let mixed_opacity = styles
            .iter()
            .any(|value| (value.opacity - style.opacity).abs() > 1e-6);
        let mixed_stroke_style = styles
            .iter()
            .any(|value| value.stroke_style != style.stroke_style);
        let mixed_fill_style = styles
            .iter()
            .any(|value| value.fill_style != style.fill_style);
        let mixed_roundness = styles
            .iter()
            .any(|value| (value.roundness - style.roundness).abs() > 1e-6);

        ui.horizontal(|ui| {
            let mut stroke = document_color(style.stroke);
            if ui.color_edit_button_srgba(&mut stroke).changed() {
                style.stroke = document_color_from_egui(stroke);
                changed = true;
            }
            ui.label("Stroke");
            if mixed_stroke {
                ui.label(RichText::new("Mixed").weak());
            }
        });
        ui.horizontal(|ui| {
            let mut fill = document_color(style.fill);
            if ui.color_edit_button_srgba(&mut fill).changed() {
                style.fill = document_color_from_egui(fill);
                changed = true;
            }
            ui.label("Fill");
            if mixed_fill {
                ui.label(RichText::new("Mixed").weak());
            }
        });
        ui.horizontal(|ui| {
            if let Some(value) = scalar_field(
                ui,
                "Width",
                style.stroke_width,
                mixed_width,
                0.1,
                0.1..=30.0,
                "",
            ) {
                style.stroke_width = value;
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            if let Some(value) = scalar_field(
                ui,
                "Opacity",
                style.opacity,
                mixed_opacity,
                0.01,
                0.0..=1.0,
                "",
            ) {
                style.opacity = value;
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Stroke");
            egui::ComboBox::from_id_salt("stroke_style")
                .selected_text(if mixed_stroke_style {
                    "Mixed".to_owned()
                } else {
                    format!("{:?}", style.stroke_style)
                })
                .show_ui(ui, |ui| {
                    for option in [StrokeStyle::Solid, StrokeStyle::Dashed, StrokeStyle::Dotted] {
                        if ui
                            .selectable_value(
                                &mut style.stroke_style,
                                option,
                                format!("{option:?}"),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Fill");
            egui::ComboBox::from_id_salt("fill_style")
                .selected_text(if mixed_fill_style {
                    "Mixed".to_owned()
                } else {
                    format!("{:?}", style.fill_style)
                })
                .show_ui(ui, |ui| {
                    for option in [FillStyle::Solid, FillStyle::Hatch, FillStyle::CrossHatch] {
                        if ui
                            .selectable_value(&mut style.fill_style, option, format!("{option:?}"))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            if let Some(value) = scalar_field(
                ui,
                "Roundness",
                style.roundness,
                mixed_roundness,
                0.5,
                0.0..=64.0,
                "",
            ) {
                style.roundness = value;
                changed = true;
            }
        });

        if changed {
            Self::apply_style_to_selection(document, selected, style);
        }
        changed
    }

    fn apply_style_to_selection(
        document: &mut Document,
        selected: &HashSet<ElementId>,
        style: epsilon_core::document::ElementStyle,
    ) {
        for element in document
            .elements
            .iter_mut()
            .filter(|element| selected.contains(&element.id))
        {
            element.style = style;
        }
    }

    fn properties_panel(&mut self, ui: &mut egui::Ui) {
        // The resize range is fixed rather than derived from the current
        // window: transient small frames at startup would otherwise clamp and
        // overwrite the persisted panel width. The compile-time budget in
        // `layout.rs` guarantees the canvas minimum at the window minimum.
        // `show_collapsible` keeps the panel resizable and lets it slide closed
        // without losing the persisted width.
        let mut hide_requested = false;
        let mut grid = self.grid_enabled;
        // Keep the open flag local so the panel closure can borrow `self` for
        // document edits without conflicting with the flag's mutable borrow.
        let mut properties_open = self.properties_open;
        egui::Panel::right("properties_panel")
            .resizable(true)
            .default_size(layout::PROPERTIES_PANEL_WIDTH)
            .size_range(layout::PROPERTIES_PANEL_MIN_WIDTH..=layout::PROPERTIES_PANEL_MAX_WIDTH)
            .show_collapsible(ui, &mut properties_open, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("PROPERTIES").small().strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Hide").clicked() {
                            hide_requested = true;
                        }
                    });
                });
                ui.separator();
                ui.add_space(4.0);
                let mut property_changed = false;
                let mut property_before = Vec::new();
                egui::CollapsingHeader::new("Selection")
                    .default_open(true)
                    .show(ui, |ui| {
                        if self.selected.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("No selection").strong());
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("Select an element to edit its properties.")
                                        .weak(),
                                );
                            });
                        } else {
                            ui.label(format!("{} selected", self.selected.len()));
                            ui.add_space(4.0);
                            property_before = self.selected_snapshots();
                            property_changed =
                                Self::transform_controls(ui, &mut self.document, &self.selected);
                            property_changed |=
                                Self::style_controls(ui, &mut self.document, &self.selected);
                        }
                    });
                ui.add_space(4.0);
                egui::CollapsingHeader::new("Canvas")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.checkbox(&mut grid, "Show grid");
                    });

                // Continuous widget drags mutate the document every frame.
                // Hold the pre-edit snapshot until the pointer is released so
                // one interaction becomes one history entry.
                let pointer_down = ui.ctx().input(|input| input.pointer.any_down());
                if property_changed {
                    let before = self.property_edit_before.take().unwrap_or(property_before);
                    if pointer_down {
                        self.property_edit_before = Some(before);
                    } else {
                        self.commit_property_edit(before);
                    }
                } else if let Some(before) = self.property_edit_before.take() {
                    if pointer_down {
                        self.property_edit_before = Some(before);
                    } else {
                        self.commit_property_edit(before);
                    }
                }
            });
        self.properties_open = properties_open;
        if hide_requested {
            self.properties_open = false;
        }
        if grid != self.grid_enabled {
            self.grid_enabled = grid;
        }
    }

    fn update_pan(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        let pointer_pos = ctx.pointer_hover_pos();
        self.hover_screen = pointer_pos;
        let in_canvas = pointer_pos.is_some_and(|pos| canvas_rect.contains(pos));
        let any_down = ctx.input(|input| input.pointer.any_down());
        let middle_pressed =
            ctx.input(|input| input.pointer.button_pressed(egui::PointerButton::Middle));
        let primary_pressed = ctx.input(|input| input.pointer.primary_pressed());
        let space_down = ctx.input(|input| input.key_down(egui::Key::Space));
        let typing = ctx.egui_wants_keyboard_input();

        match self.pointer_mode {
            PointerMode::Panning { last } => {
                if !any_down {
                    self.pointer_mode = PointerMode::Idle;
                } else if let Some(pos) = pointer_pos {
                    self.viewport.offset.x += (pos.x - last.x) as f64;
                    self.viewport.offset.y += (pos.y - last.y) as f64;
                    self.pointer_mode = PointerMode::Panning { last: pos };
                }
            }
            PointerMode::Idle => {
                let start_pan = if middle_pressed && in_canvas {
                    true
                } else if primary_pressed && in_canvas {
                    if self.tool == Tool::Hand {
                        true
                    } else {
                        space_down && !typing
                    }
                } else {
                    false
                };

                if start_pan && let Some(pos) = pointer_pos {
                    self.pointer_mode = PointerMode::Panning { last: pos };
                }
            }
            // Drawing drags are finished by `update_drawing` after the
            // release event is visible to this frame.
            PointerMode::Dragging { .. }
            | PointerMode::Freehanding { .. }
            | PointerMode::Selecting { .. }
            | PointerMode::Moving { .. }
            | PointerMode::Resizing { .. }
            | PointerMode::Rotating => {}
        }

        let panning = matches!(self.pointer_mode, PointerMode::Panning { .. });
        let cursor = if panning {
            egui::CursorIcon::Grabbing
        } else if self.tool == Tool::Hand || (space_down && !typing) {
            egui::CursorIcon::Grab
        } else if drawing_kind(self.tool).is_some() {
            egui::CursorIcon::Crosshair
        } else {
            egui::CursorIcon::Default
        };

        if in_canvas || panning {
            ctx.set_cursor_icon(cursor);
        }
    }

    /// Maps a screen position inside the canvas to world coordinates.
    fn screen_to_world(&self, canvas_rect: egui::Rect, screen: egui::Pos2) -> Point {
        self.viewport.screen_to_world(Point::new(
            (screen.x - canvas_rect.min.x) as f64,
            (screen.y - canvas_rect.min.y) as f64,
        ))
    }

    /// Starts, updates and commits shape drags for the active drawing tool.
    fn update_drawing(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        let pointer_pos = ctx.pointer_hover_pos();
        let primary_down = ctx.input(|input| input.pointer.primary_down());
        let primary_pressed = ctx.input(|input| input.pointer.primary_pressed());
        let space_down = ctx.input(|input| input.key_down(egui::Key::Space));
        let shift_down = ctx.input(|input| input.modifiers.shift);
        let typing = ctx.egui_wants_keyboard_input();

        if let PointerMode::Freehanding { last } = self.pointer_mode {
            let mut last = last;
            if let Some(pos) = pointer_pos {
                let world = self.screen_to_world(canvas_rect, pos);
                if world.distance(last) * self.viewport.zoom >= MIN_FREEHAND_SAMPLE_PIXELS {
                    if self.freehand_points.len() < MAX_FREEHAND_POINTS {
                        self.freehand_points.push(world);
                    }
                    last = world;
                }
            }

            if primary_down {
                self.pointer_mode = PointerMode::Freehanding { last };
            } else {
                self.commit_freehand();
                self.pointer_mode = PointerMode::Idle;
            }
            return;
        }

        if let PointerMode::Dragging {
            kind,
            start,
            current,
        } = self.pointer_mode
        {
            let current = pointer_pos
                .map(|pos| self.screen_to_world(canvas_rect, pos))
                .unwrap_or(current);
            if primary_down {
                self.pointer_mode = PointerMode::Dragging {
                    kind,
                    start,
                    current,
                };
            } else {
                let end = drawing_end(kind, start, current, shift_down);
                self.commit_shape(kind, start, end);
                self.pointer_mode = PointerMode::Idle;
            }
            return;
        }

        if self.tool == Tool::Freehand
            && primary_pressed
            && !space_down
            && !typing
            && let Some(pos) = pointer_pos
            && canvas_rect.contains(pos)
        {
            let world = self.screen_to_world(canvas_rect, pos);
            self.freehand_points.clear();
            self.freehand_points.push(world);
            self.pointer_mode = PointerMode::Freehanding { last: world };
            return;
        }

        if let Some(kind) = drawing_kind(self.tool)
            && primary_pressed
            && !space_down
            && !typing
            && let Some(pos) = pointer_pos
            && canvas_rect.contains(pos)
        {
            let world = self.screen_to_world(canvas_rect, pos);
            self.pointer_mode = PointerMode::Dragging {
                kind,
                start: world,
                current: world,
            };
        }
    }

    fn selected_snapshots(&self) -> Vec<Element> {
        self.document
            .elements
            .iter()
            .filter(|element| self.selected.contains(&element.id))
            .cloned()
            .collect()
    }

    fn selection_bounds(&self) -> Option<Rect> {
        self.document
            .elements
            .iter()
            .filter(|element| self.selected.contains(&element.id))
            .map(Element::world_bounds)
            .reduce(|left, right| left.union(&right))
    }

    fn resize_handle_at(
        &self,
        pointer: egui::Pos2,
        canvas_rect: egui::Rect,
    ) -> Option<ResizeHandle> {
        let bounds = world_rect_screen(canvas_rect, &self.viewport, self.selection_bounds()?);
        handle_screen_positions(bounds)
            .into_iter()
            .find(|(_, position)| position.distance(pointer) <= HANDLE_HIT_RADIUS_PIXELS)
            .map(|(handle, _)| handle)
    }

    fn apply_move_delta(&mut self, delta: Point) {
        for original in &self.transform_original {
            if let Some(element) = self
                .document
                .elements
                .iter_mut()
                .find(|element| element.id == original.id)
            {
                *element = original.translated(delta);
            }
        }
    }

    fn apply_rotation(&mut self, delta: f64) {
        for original in &self.transform_original {
            if let Some(element) = self
                .document
                .elements
                .iter_mut()
                .find(|element| element.id == original.id)
            {
                *element = original.rotated_around(self.rotation_center, delta);
            }
        }
    }

    fn rotation_handle_at(&self, pointer: egui::Pos2, canvas_rect: egui::Rect) -> bool {
        let Some(bounds) = self.selection_bounds() else {
            return false;
        };
        let screen = world_rect_screen(canvas_rect, &self.viewport, bounds);
        rotation_handle_position(screen).distance(pointer) <= HANDLE_HIT_RADIUS_PIXELS
    }

    fn apply_resize_to(&mut self, bounds: Rect) {
        for original in &self.transform_original {
            if let Some(element) = self
                .document
                .elements
                .iter_mut()
                .find(|element| element.id == original.id)
            {
                *element = original.scaled_from(self.resize_start_bounds, bounds);
            }
        }
    }

    /// Applies an active move or resize drag from captured snapshots.
    fn update_transform(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        let primary_down = ctx.input(|input| input.pointer.primary_down());
        let shift_down = ctx.input(|input| input.modifiers.shift);
        let alt_down = ctx.input(|input| input.modifiers.alt);
        let pointer = ctx.pointer_hover_pos();

        match self.pointer_mode {
            PointerMode::Moving { start, current } => {
                let current = pointer
                    .map(|pos| self.screen_to_world(canvas_rect, pos))
                    .unwrap_or(current);
                if primary_down {
                    self.apply_move_delta(current - start);
                    self.pointer_mode = PointerMode::Moving { start, current };
                } else {
                    self.finish_transform();
                    self.pointer_mode = PointerMode::Idle;
                }
            }
            PointerMode::Resizing { handle } => {
                if primary_down {
                    if let Some(pointer) = pointer {
                        let world = self.screen_to_world(canvas_rect, pointer);
                        let bounds = resize_rect(
                            self.resize_start_bounds,
                            handle,
                            world,
                            alt_down,
                            shift_down,
                            MIN_ELEMENT_PIXELS / self.viewport.zoom,
                        );
                        self.apply_resize_to(bounds);
                    }
                    self.pointer_mode = PointerMode::Resizing { handle };
                } else {
                    self.finish_transform();
                    self.pointer_mode = PointerMode::Idle;
                }
            }
            PointerMode::Rotating => {
                if primary_down {
                    if let Some(pointer) = pointer {
                        let world = self.screen_to_world(canvas_rect, pointer);
                        let offset = world - self.rotation_center;
                        let angle = offset.y.atan2(offset.x);
                        let mut delta = angle - self.rotation_start_angle;
                        if shift_down {
                            delta = snap_angle(delta, LINE_ANGLE_STEP);
                        }
                        self.apply_rotation(delta);
                    }
                    self.pointer_mode = PointerMode::Rotating;
                } else {
                    self.finish_transform();
                    self.pointer_mode = PointerMode::Idle;
                }
            }
            _ => {}
        }
    }

    /// Handles topmost click selection, shift-toggle and marquee selection.
    fn update_selection(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        let pointer_pos = ctx.pointer_hover_pos();
        let primary_down = ctx.input(|input| input.pointer.primary_down());
        let primary_pressed = ctx.input(|input| input.pointer.primary_pressed());
        let shift_down = ctx.input(|input| input.modifiers.shift);
        let space_down = ctx.input(|input| input.key_down(egui::Key::Space));
        let typing = ctx.egui_wants_keyboard_input();

        if let PointerMode::Selecting {
            start,
            current,
            additive,
        } = self.pointer_mode
        {
            let current = pointer_pos
                .map(|pos| self.screen_to_world(canvas_rect, pos))
                .unwrap_or(current);
            if primary_down {
                self.pointer_mode = PointerMode::Selecting {
                    start,
                    current,
                    additive,
                };
            } else {
                let marquee = Rect::from_corners(start, current);
                let hits: Vec<ElementId> = self
                    .document
                    .elements
                    .iter()
                    .filter(|element| element.intersects_rect(marquee))
                    .map(|element| element.id)
                    .collect();
                let hits = self.document.expand_selection(&hits);
                if !additive {
                    self.selected.clear();
                }
                self.selected.extend(hits);
                self.pointer_mode = PointerMode::Idle;
            }
            return;
        }

        if self.tool != Tool::Select
            || !primary_pressed
            || space_down
            || typing
            || !pointer_pos.is_some_and(|pos| canvas_rect.contains(pos))
        {
            return;
        }

        let pointer = pointer_pos.expect("checked above");
        if self.rotation_handle_at(pointer, canvas_rect)
            && let Some(bounds) = self.selection_bounds()
        {
            self.transform_original = self.selected_snapshots();
            self.rotation_center = bounds.center();
            let world = self.screen_to_world(canvas_rect, pointer);
            let offset = world - self.rotation_center;
            self.rotation_start_angle = offset.y.atan2(offset.x);
            self.pointer_mode = PointerMode::Rotating;
            return;
        }

        if let Some(handle) = self.resize_handle_at(pointer, canvas_rect)
            && let Some(bounds) = self.selection_bounds()
        {
            self.transform_original = self.selected_snapshots();
            self.resize_start_bounds = bounds;
            self.pointer_mode = PointerMode::Resizing { handle };
            return;
        }

        let world = self.screen_to_world(canvas_rect, pointer);
        let tolerance = HIT_TOLERANCE_PIXELS / self.viewport.zoom;
        let hit = self
            .document
            .paint_order()
            .into_iter()
            .rev()
            .find(|element| element.hit_test(world, tolerance))
            .map(|element| element.id);

        if let Some(id) = hit {
            if shift_down {
                self.toggle_selected(id, true);
                self.pointer_mode = PointerMode::Idle;
            } else {
                if !self.selected.contains(&id) {
                    self.selected = self.document.expand_selection(&[id]).into_iter().collect();
                }
                self.transform_original = self.selected_snapshots();
                self.pointer_mode = PointerMode::Moving {
                    start: world,
                    current: world,
                };
            }
        } else {
            if !shift_down {
                self.selected.clear();
            }
            self.pointer_mode = PointerMode::Selecting {
                start: world,
                current: world,
                additive: shift_down,
            };
        }
    }

    fn toggle_selected(&mut self, id: ElementId, additive: bool) {
        let members = self.document.expand_selection(&[id]);
        if additive {
            let all_selected = members.iter().all(|member| self.selected.contains(member));
            if all_selected {
                for member in members {
                    self.selected.remove(&member);
                }
            } else {
                self.selected.extend(members);
            }
        } else {
            self.selected = members.into_iter().collect();
        }
    }

    /// Draws selection bounds and the active marquee.
    fn paint_selection(
        &self,
        painter: &egui::Painter,
        canvas_rect: egui::Rect,
        accent: egui::Color32,
    ) {
        for id in &self.selected {
            if let Some(element) = self.document.element(*id) {
                let bounds = world_rect_screen(canvas_rect, &self.viewport, element.world_bounds());
                painter.rect_stroke(
                    bounds.expand(2.0),
                    2.0,
                    egui::Stroke::new(1.5, accent),
                    egui::StrokeKind::Outside,
                );
            }
        }

        if let Some(bounds) = self.selection_bounds() {
            let screen = world_rect_screen(canvas_rect, &self.viewport, bounds);
            for (_, position) in handle_screen_positions(screen) {
                let handle =
                    egui::Rect::from_center_size(position, egui::Vec2::splat(HANDLE_SIZE_PIXELS));
                painter.rect_filled(handle, 1.0, accent);
            }
            let top_center = egui::pos2(screen.center().x, screen.top());
            let rotation = rotation_handle_position(screen);
            painter.line_segment([top_center, rotation], egui::Stroke::new(1.0, accent));
            painter.circle_filled(rotation, ROTATION_HANDLE_RADIUS_PIXELS, accent);
        }

        if let PointerMode::Selecting { start, current, .. } = self.pointer_mode {
            let marquee = draft_screen_rect(canvas_rect, &self.viewport, start, current);
            painter.rect_filled(marquee, 0.0, accent.gamma_multiply(0.12));
            painter.rect_stroke(
                marquee,
                0.0,
                egui::Stroke::new(1.0, accent),
                egui::StrokeKind::Inside,
            );
        }
    }

    /// Commits a shape if the drag is large enough on screen.
    fn commit_shape(&mut self, kind: ElementKind, start: Point, end: Point) {
        let rect = Rect::from_corners(start, end);
        if kind == ElementKind::Text {
            let dragged_width = rect.width();
            let width = if dragged_width * self.viewport.zoom < MIN_TEXT_WIDTH_PIXELS {
                DEFAULT_TEXT_WIDTH
            } else {
                dragged_width
            };
            let mut element = Element::text(rect.min.x, rect.min.y, width, DEFAULT_FONT_SIZE);
            element.style.stroke = self.default_stroke();
            let id = self.document.add(element);
            self.editing_text = Some(id);
            self.editing_text_is_new = true;
            self.text_buffer.clear();
            self.text_editor_needs_focus = true;
            return;
        }

        let screen_width = rect.width() * self.viewport.zoom;
        let screen_height = rect.height() * self.viewport.zoom;
        let too_small = if matches!(kind, ElementKind::Line | ElementKind::Arrow) {
            start.distance(end) * self.viewport.zoom < MIN_SHAPE_PIXELS
        } else {
            screen_width < MIN_SHAPE_PIXELS || screen_height < MIN_SHAPE_PIXELS
        };
        if too_small {
            return;
        }

        let mut element = match kind {
            ElementKind::Line => Element::line(start, end),
            ElementKind::Arrow => Element::arrow(start, end),
            _ => Element::new(kind, rect.min.x, rect.min.y, rect.width(), rect.height()),
        };
        element.style.stroke = self.default_stroke();
        self.insert_element(element);
    }

    fn default_stroke(&self) -> DocumentColor {
        match self.theme {
            Theme::Dark => DocumentColor::rgb(235, 235, 235),
            Theme::Light => DocumentColor::BLACK,
        }
    }

    /// Draws and commits the in-canvas text editor. Returns `true` while the
    /// editor should block canvas input for this frame.
    fn update_text_edit(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) -> bool {
        let Some(id) = self.editing_text else {
            return false;
        };
        let Some(element) = self.document.element(id) else {
            self.editing_text = None;
            return false;
        };
        let editor_rect = element_screen_rect(canvas_rect, &self.viewport, element);
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let clicked_out = ctx.input(|input| input.pointer.primary_pressed())
            && ctx
                .pointer_hover_pos()
                .is_some_and(|pos| !editor_rect.expand(8.0).contains(pos));
        if escape || clicked_out {
            self.commit_text_edit(ctx);
            return true;
        }

        let font_size = (element.font_size * self.viewport.zoom).max(6.0) as f32;
        let width = (element.width * self.viewport.zoom).max(30.0) as f32;
        let height = (element.height * self.viewport.zoom).max(font_size as f64 * 1.4) as f32;
        let text_color = document_color(element.style.stroke);
        #[allow(deprecated)]
        let id = egui::Id::new(("text_editor", id.get()));
        let mut buffer = self.text_buffer.clone();
        let mut needs_focus = self.text_editor_needs_focus;

        egui::Area::new(id)
            .fixed_pos(editor_rect.min)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let response = ui.add_sized(
                        [width, height],
                        egui::TextEdit::multiline(&mut buffer)
                            .font(egui::FontId::proportional(font_size))
                            .text_color(text_color)
                            .desired_width(width),
                    );
                    if needs_focus {
                        response.request_focus();
                        needs_focus = false;
                    }
                });
            });

        self.text_buffer = buffer;
        self.text_editor_needs_focus = needs_focus;
        true
    }

    /// Commits or discards the text element being edited.
    fn commit_text_edit(&mut self, ctx: &egui::Context) {
        let Some(id) = self.editing_text.take() else {
            return;
        };
        let was_new = std::mem::take(&mut self.editing_text_is_new);
        let text = std::mem::take(&mut self.text_buffer);
        self.text_editor_needs_focus = false;

        let Some(index) = self
            .document
            .elements
            .iter()
            .position(|element| element.id == id)
        else {
            return;
        };
        let before = self.document.elements[index].clone();

        if text.trim().is_empty() {
            if was_new {
                // Nothing was committed, so the aborted text leaves no entry.
                self.document.elements.remove(index);
            } else if let Ok(transaction) = Transaction::remove(&self.document, &[id]) {
                self.commit_transaction(transaction);
            } else {
                self.history_error = Some("Could not remove empty text".to_owned());
            }
            return;
        }

        let zoom = self.viewport.zoom;
        let mut after = before.clone();
        after.height = measured_text_height(ctx, &text, after.font_size, after.width, zoom);
        after.text = text;
        after.version = after.version.saturating_add(1);

        if was_new {
            // The element existed only for editing; record its creation once
            // with the final content and measured height. The insert command
            // requires the element to be absent, so remove it first and put it
            // back if the commit is rejected.
            self.document.elements.remove(index);
            let transaction = Transaction::insert(index, vec![after.clone()]);
            if !self.commit_transaction(transaction) {
                self.document.elements.insert(index, after);
            }
        } else if after != before {
            match Transaction::replace(vec![before], vec![after]) {
                Ok(transaction) => {
                    self.commit_transaction(transaction);
                }
                Err(error) => self.history_error = Some(format!("Edit failed: {error}")),
            }
        }
    }

    /// Smooths, simplifies and commits the collected freehand stroke.
    fn commit_freehand(&mut self) {
        let smoothed = smooth_polyline(&self.freehand_points, 1);
        let points = simplify_polyline(&smoothed, FREEHAND_SIMPLIFY_PIXELS / self.viewport.zoom);
        let screen_length = points
            .windows(2)
            .map(|pair| pair[0].distance(pair[1]))
            .sum::<f64>()
            * self.viewport.zoom;
        self.freehand_points.clear();

        if screen_length < MIN_SHAPE_PIXELS {
            return;
        }
        if let Some(mut element) = Element::freehand(points) {
            element.style.stroke = self.default_stroke();
            self.insert_element(element);
        }
    }

    /// Draws the document in paint order through the screen-space adapter.
    fn paint_document(&self, painter: &egui::Painter, canvas_rect: egui::Rect) {
        for element in self.document.paint_order() {
            let rect = element_screen_rect(canvas_rect, &self.viewport, element);
            let opacity = element.style.opacity.clamp(0.0, 1.0);
            let fill = document_color_with_opacity(element.style.fill, opacity);
            let stroke_width = (element.style.stroke_width * self.viewport.zoom).max(0.75) as f32;
            let stroke = egui::Stroke::new(
                stroke_width,
                document_color_with_opacity(element.style.stroke, opacity),
            );
            let fill_style = element.style.fill_style;
            let stroke_style = element.style.stroke_style;
            let roundness = (element.style.roundness * self.viewport.zoom).max(0.0) as f32;

            match element.kind {
                ElementKind::Rectangle => {
                    let bounds =
                        Rect::from_xywh(element.x, element.y, element.width, element.height);
                    let corners = [
                        bounds.min,
                        Point::new(bounds.max.x, bounds.min.y),
                        bounds.max,
                        Point::new(bounds.min.x, bounds.max.y),
                    ];
                    let points = element_stroke_screen_points(
                        canvas_rect,
                        &self.viewport,
                        element,
                        corners,
                        true,
                        true,
                    );
                    let stroke_points = if self.clean_rendering {
                        points.clone()
                    } else {
                        element_stroke_screen_points(
                            canvas_rect,
                            &self.viewport,
                            element,
                            corners,
                            true,
                            false,
                        )
                    };
                    let corner_radius = roundness.min(rect.width().min(rect.height()) * 0.5);
                    if element.angle.abs() <= 1e-9 && fill_style == FillStyle::Solid {
                        if fill.a() > 0 {
                            painter.rect_filled(rect, corner_radius, fill);
                        }
                    } else if fill.a() > 0 {
                        paint_fill_path(painter, &points, fill_style, fill);
                    }
                    if self.clean_rendering
                        && element.angle.abs() <= 1e-9
                        && stroke_style == StrokeStyle::Solid
                    {
                        painter.rect_stroke(rect, corner_radius, stroke, egui::StrokeKind::Inside);
                    } else {
                        paint_stroke_path(painter, &stroke_points, true, stroke, stroke_style);
                    }
                }
                ElementKind::Ellipse => {
                    let bounds =
                        Rect::from_xywh(element.x, element.y, element.width, element.height);
                    let local_points = ellipse_local_points(bounds);
                    let points = element_stroke_screen_points(
                        canvas_rect,
                        &self.viewport,
                        element,
                        local_points.iter().copied(),
                        true,
                        true,
                    );
                    let stroke_points = if self.clean_rendering {
                        points.clone()
                    } else {
                        element_stroke_screen_points(
                            canvas_rect,
                            &self.viewport,
                            element,
                            local_points.iter().copied(),
                            true,
                            false,
                        )
                    };
                    if element.angle.abs() <= 1e-9 && fill_style == FillStyle::Solid {
                        let center = rect.center();
                        let radius = rect.size() / 2.0;
                        if fill.a() > 0 {
                            painter.add(egui::Shape::ellipse_filled(center, radius, fill));
                        }
                    } else if fill.a() > 0 {
                        paint_fill_path(painter, &points, fill_style, fill);
                    }
                    if self.clean_rendering
                        && element.angle.abs() <= 1e-9
                        && stroke_style == StrokeStyle::Solid
                    {
                        let center = rect.center();
                        let radius = rect.size() / 2.0;
                        painter.add(egui::Shape::ellipse_stroke(center, radius, stroke));
                    } else {
                        paint_stroke_path(painter, &stroke_points, true, stroke, stroke_style);
                    }
                }
                ElementKind::Diamond => {
                    let bounds =
                        Rect::from_xywh(element.x, element.y, element.width, element.height);
                    let local_points = bounds.diamond_points();
                    let points = element_stroke_screen_points(
                        canvas_rect,
                        &self.viewport,
                        element,
                        local_points,
                        true,
                        true,
                    );
                    let stroke_points = if self.clean_rendering {
                        points.clone()
                    } else {
                        element_stroke_screen_points(
                            canvas_rect,
                            &self.viewport,
                            element,
                            local_points,
                            true,
                            false,
                        )
                    };
                    if fill.a() > 0 {
                        paint_fill_path(painter, &points, fill_style, fill);
                    }
                    paint_stroke_path(painter, &stroke_points, true, stroke, stroke_style);
                }
                ElementKind::Line => {
                    if element.points.len() == 2 {
                        let points = element_stroke_screen_points(
                            canvas_rect,
                            &self.viewport,
                            element,
                            element.points.iter().copied(),
                            false,
                            self.clean_rendering,
                        );
                        paint_stroke_path(painter, &points, false, stroke, stroke_style);
                    }
                }
                ElementKind::Arrow => {
                    if let [start, end] = element.points.as_slice() {
                        let start = element_world_point(element, *start);
                        let end = element_world_point(element, *end);
                        let points = element_stroke_screen_points(
                            canvas_rect,
                            &self.viewport,
                            element,
                            element.points.iter().copied(),
                            false,
                            self.clean_rendering,
                        );
                        paint_stroke_path(painter, &points, false, stroke, stroke_style);
                        if let Some(head) =
                            arrowhead_screen_points(canvas_rect, &self.viewport, start, end)
                        {
                            painter.add(egui::Shape::convex_polygon(
                                head,
                                stroke.color,
                                egui::Stroke::NONE,
                            ));
                        }
                    }
                }
                ElementKind::Freehand => {
                    let points = element_stroke_screen_points(
                        canvas_rect,
                        &self.viewport,
                        element,
                        element.points.iter().copied(),
                        false,
                        self.clean_rendering,
                    );
                    paint_stroke_path(painter, &points, false, stroke, stroke_style);
                }
                ElementKind::Text => {
                    if !element.text.is_empty() {
                        let color = document_color_with_opacity(element.style.stroke, opacity);
                        let font_id = egui::FontId::proportional(
                            (element.font_size * self.viewport.zoom).max(1.0) as f32,
                        );
                        let wrap_width = (element.width * self.viewport.zoom).max(1.0) as f32;
                        let galley =
                            painter.layout(element.text.clone(), font_id, color, wrap_width);
                        let pos = match element.text_align {
                            TextAlign::Left => rect.min,
                            TextAlign::Center => {
                                egui::pos2(rect.center().x - galley.size().x / 2.0, rect.min.y)
                            }
                            TextAlign::Right => {
                                egui::pos2(rect.max.x - galley.size().x, rect.min.y)
                            }
                        };
                        painter.galley(pos, galley, color);
                    }
                }
            }
        }
    }

    /// Draws the live shape preview while dragging.
    fn paint_draft(
        &self,
        painter: &egui::Painter,
        canvas_rect: egui::Rect,
        accent: egui::Color32,
        constrain: bool,
    ) {
        if let PointerMode::Dragging {
            kind,
            start,
            current,
        } = self.pointer_mode
        {
            let rect = draft_screen_rect(canvas_rect, &self.viewport, start, current);
            let fill = accent.gamma_multiply(0.18);
            match kind {
                ElementKind::Rectangle => {
                    painter.rect_filled(rect, 0.0, fill);
                    painter.rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(1.0, accent),
                        egui::StrokeKind::Inside,
                    );
                }
                ElementKind::Ellipse => {
                    let center = rect.center();
                    let radius = rect.size() / 2.0;
                    painter.add(egui::Shape::ellipse_filled(center, radius, fill));
                    painter.add(egui::Shape::ellipse_stroke(
                        center,
                        radius,
                        egui::Stroke::new(1.0, accent),
                    ));
                }
                ElementKind::Diamond => {
                    let bounds = Rect::from_corners(start, current);
                    let points = diamond_screen_points(canvas_rect, &self.viewport, bounds);
                    painter.add(egui::Shape::convex_polygon(
                        points.clone(),
                        fill,
                        egui::Stroke::NONE,
                    ));
                    painter.add(egui::Shape::closed_line(
                        points,
                        egui::Stroke::new(1.0, accent),
                    ));
                }
                ElementKind::Line => {
                    let end = drawing_end(kind, start, current, constrain);
                    painter.line_segment(
                        [
                            world_to_screen_pos(canvas_rect, &self.viewport, start),
                            world_to_screen_pos(canvas_rect, &self.viewport, end),
                        ],
                        egui::Stroke::new(1.0, accent),
                    );
                }
                ElementKind::Arrow => {
                    let end = drawing_end(kind, start, current, constrain);
                    painter.line_segment(
                        [
                            world_to_screen_pos(canvas_rect, &self.viewport, start),
                            world_to_screen_pos(canvas_rect, &self.viewport, end),
                        ],
                        egui::Stroke::new(1.0, accent),
                    );
                    if let Some(head) =
                        arrowhead_screen_points(canvas_rect, &self.viewport, start, end)
                    {
                        painter.add(egui::Shape::convex_polygon(
                            head,
                            accent,
                            egui::Stroke::NONE,
                        ));
                    }
                }
                _ => {}
            }
        } else if let PointerMode::Freehanding { .. } = self.pointer_mode {
            let points: Vec<egui::Pos2> = self
                .freehand_points
                .iter()
                .map(|point| world_to_screen_pos(canvas_rect, &self.viewport, *point))
                .collect();
            if points.len() >= 2 {
                painter.add(egui::Shape::line(points, egui::Stroke::new(1.0, accent)));
            }
        }
    }

    fn update_zoom(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        let in_canvas = ctx
            .pointer_hover_pos()
            .is_some_and(|pos| canvas_rect.contains(pos));
        if !in_canvas {
            return;
        }

        let anchor = pointer_anchor(ctx, canvas_rect);
        let zoom_delta = ctx.input(|input| input.zoom_delta());
        if (zoom_delta - 1.0).abs() > f32::EPSILON {
            self.viewport.zoom_by(zoom_delta as f64, anchor);
        }

        let events: Vec<egui::Event> = ctx.input(|input| input.events.clone());
        for event in events {
            let egui::Event::MouseWheel {
                delta,
                modifiers,
                unit,
                ..
            } = event
            else {
                continue;
            };
            if modifiers.ctrl || modifiers.command {
                continue;
            }

            let scale = match unit {
                egui::MouseWheelUnit::Line => 40.0,
                egui::MouseWheelUnit::Page => canvas_rect.height(),
                egui::MouseWheelUnit::Point => 1.0,
            };
            if modifiers.shift {
                self.viewport.offset.x += (delta.y * scale) as f64;
            } else {
                self.viewport.offset.x += (delta.x * scale) as f64;
                self.viewport.offset.y += (delta.y * scale) as f64;
            }
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(layout::STATUS_BAR_HEIGHT)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new(format!("Tool: {}", self.tool.name()))
                            .small()
                            .weak(),
                    );
                    ui.add_space(12.0);
                    let pointer_text = match self.pointer_world {
                        Some(point) => format!("Pointer: {:.1}, {:.1}", point.x, point.y),
                        None => "Pointer: —".to_owned(),
                    };
                    ui.label(RichText::new(pointer_text).small().weak());
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!("Zoom: {:.0}%", self.viewport.zoom * 100.0))
                            .small()
                            .weak(),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!(
                            "Grid: {}",
                            if self.grid_enabled { "on" } else { "off" }
                        ))
                        .small()
                        .weak(),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!(
                            "Undo: {} · Redo: {}",
                            self.history.undo_len(),
                            self.history.redo_len()
                        ))
                        .small()
                        .weak(),
                    );
                    if let Some(error) = &self.history_error {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(error)
                                .small()
                                .color(ui.visuals().error_fg_color),
                        );
                    }
                    if let Some(feedback) = &self.clipboard_feedback {
                        ui.add_space(12.0);
                        let text = RichText::new(feedback.message()).small();
                        ui.label(if feedback.is_error() {
                            text.color(ui.visuals().error_fg_color)
                        } else {
                            text.weak()
                        });
                    }
                    if let Some(feedback) = &self.file_feedback {
                        ui.add_space(12.0);
                        let text = RichText::new(feedback.message()).small();
                        ui.label(if feedback.is_error() {
                            text.color(ui.visuals().error_fg_color)
                        } else {
                            text.weak()
                        });
                    }
                    ui.add_space(12.0);
                    let dirty = if self.is_dirty() { "•" } else { "" };
                    ui.label(
                        RichText::new(format!("{}{dirty}", self.document_display_name()))
                            .small()
                            .weak(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(concat!("Epsilon ", env!("CARGO_PKG_VERSION"))).weak(),
                        );
                    });
                });
            });
    }

    /// Draws the canvas region and returns its screen rect for input mapping.
    fn canvas(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        let background = ui.visuals().extreme_bg_color;
        let response = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(background))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                self.canvas_rect = rect;

                // Track the pointer in world coordinates for the status bar.
                self.pointer_world = ui.ctx().pointer_hover_pos().and_then(|pos| {
                    if rect.contains(pos) {
                        Some(self.viewport.screen_to_world(Point::new(
                            (pos.x - rect.min.x) as f64,
                            (pos.y - rect.min.y) as f64,
                        )))
                    } else {
                        None
                    }
                });

                let editing_text = self.editing_text.is_some();
                if editing_text {
                    self.update_text_edit(ui.ctx(), rect);
                } else {
                    self.update_context_selection(ui.ctx(), rect);
                    self.update_pan(ui.ctx(), rect);
                    self.update_zoom(ui.ctx(), rect);
                    self.update_drawing(ui.ctx(), rect);
                    self.update_transform(ui.ctx(), rect);
                    self.update_selection(ui.ctx(), rect);
                }

                if self.grid_enabled {
                    let step =
                        grid::grid_step(grid::DEFAULT_GRID_SIZE, self.viewport.zoom, 8.0, 64.0);
                    let min_x = self.viewport.screen_to_world(Point::new(0.0, 0.0)).x;
                    let max_x = self
                        .viewport
                        .screen_to_world(Point::new(rect.width() as f64, 0.0))
                        .x;
                    let min_y = self.viewport.screen_to_world(Point::new(0.0, 0.0)).y;
                    let max_y = self
                        .viewport
                        .screen_to_world(Point::new(0.0, rect.height() as f64))
                        .y;
                    let (min_x, max_x) = if min_x <= max_x {
                        (min_x, max_x)
                    } else {
                        (max_x, min_x)
                    };
                    let (min_y, max_y) = if min_y <= max_y {
                        (min_y, max_y)
                    } else {
                        (max_y, min_y)
                    };

                    let painter = ui.painter();
                    let color = ui.visuals().widgets.noninteractive.bg_stroke.color;
                    let stroke = egui::Stroke::new(1.0, color);

                    let mut x = (min_x / step).floor() * step;
                    while x <= max_x {
                        let screen_x =
                            rect.min.x + self.viewport.world_to_screen(Point::new(x, 0.0)).x as f32;
                        painter.line_segment(
                            [
                                egui::pos2(screen_x, rect.min.y),
                                egui::pos2(screen_x, rect.max.y),
                            ],
                            stroke,
                        );
                        x += step;
                    }

                    let mut y = (min_y / step).floor() * step;
                    while y <= max_y {
                        let screen_y =
                            rect.min.y + self.viewport.world_to_screen(Point::new(0.0, y)).y as f32;
                        painter.line_segment(
                            [
                                egui::pos2(rect.min.x, screen_y),
                                egui::pos2(rect.max.x, screen_y),
                            ],
                            stroke,
                        );
                        y += step;
                    }
                }

                self.paint_document(ui.painter(), rect);
                let accent = ui.visuals().hyperlink_color;
                let shift_down = ui.ctx().input(|input| input.modifiers.shift);
                self.paint_draft(ui.painter(), rect, accent, shift_down);
                self.paint_selection(ui.painter(), rect, accent);

                // Prove the coordinate pipeline: the world origin is projected
                // through the viewport and marked on screen.
                let origin = self.viewport.world_to_screen(Point::ZERO);
                let origin_pos =
                    egui::pos2(rect.min.x + origin.x as f32, rect.min.y + origin.y as f32);
                if rect.contains(origin_pos) {
                    let painter = ui.painter();
                    let color = ui.visuals().text_color();
                    painter.circle_filled(origin_pos, 4.0, color);
                    painter.line_segment(
                        [
                            egui::pos2(origin_pos.x - 10.0, origin_pos.y),
                            egui::pos2(origin_pos.x + 10.0, origin_pos.y),
                        ],
                        egui::Stroke::new(1.0, color),
                    );
                    painter.line_segment(
                        [
                            egui::pos2(origin_pos.x, origin_pos.y - 10.0),
                            egui::pos2(origin_pos.x, origin_pos.y + 10.0),
                        ],
                        egui::Stroke::new(1.0, color),
                    );
                }
            });

        // The canvas is a container, not an interactive widget, so use the
        // container variant. No menu exists while text is being edited.
        if self.editing_text.is_none() && !self.context_actions().is_empty() {
            let ctx = ui.ctx().clone();
            response
                .response
                .container_context_menu(|ui| self.context_menu(ui, &ctx));
        }

        response.response.rect
    }

    /// Lays out every shell region in the required order and returns the canvas
    /// rect. Input handling for the canvas (Stage 2) depends on this rect.
    fn show_shell(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        // Panel order defines nesting: chrome first, canvas last.
        self.menu_bar(ui);
        self.top_toolbar(ui);
        self.status_bar(ui);
        self.tool_panel(ui);
        self.properties_panel(ui);
        self.canvas(ui)
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Keyboard shortcuts")
            .open(&mut self.help_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::Grid::new("epsilon_shortcuts")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for binding in KEYBINDINGS {
                            ui.label(RichText::new(binding.shortcut).strong());
                            ui.label(binding.description);
                            ui.end_row();
                        }
                    });
            });
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("About Epsilon")
            .open(&mut self.about_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.heading("Epsilon");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.add_space(4.0);
                ui.label("A fast, native whiteboard written in Rust.");
                ui.label(RichText::new("Offline-first · no telemetry · GPL-3.0").weak());
            });
    }
}

/// Appends the native extension when the chosen name has none or a different
/// one, matching common Save-As behaviour.
fn with_document_extension(path: PathBuf) -> PathBuf {
    with_extension_if_needed(path, FILE_EXTENSION)
}

fn with_extension_if_needed(mut path: PathBuf, extension: &str) -> PathBuf {
    let has_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|current| current.eq_ignore_ascii_case(extension));
    if !has_extension {
        path.set_extension(extension);
    }
    path
}

fn request_quit(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
}

fn pointer_anchor(ctx: &egui::Context, canvas_rect: egui::Rect) -> Point {
    if let Some(pos) = ctx.pointer_hover_pos()
        && canvas_rect.contains(pos)
    {
        return Point::new(
            (pos.x - canvas_rect.min.x) as f64,
            (pos.y - canvas_rect.min.y) as f64,
        );
    }
    Point::new(
        canvas_rect.width() as f64 * 0.5,
        canvas_rect.height() as f64 * 0.5,
    )
}

impl eframe::App for EpsilonApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let close_requested = ui.ctx().input(|input| input.viewport().close_requested());
        if close_requested {
            // Commit the live text buffer before deciding whether closing would
            // lose data, so editing dirty state is accurate.
            self.commit_text_edit(ui.ctx());
            if self.is_dirty() && !self.allow_close {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if self.pending_action.is_none() {
                    self.pending_action = Some(PendingAction::Quit);
                }
            }
        }

        if self.recovery_offer.is_none()
            && self.is_dirty()
            && !self.history_blocked()
            && self.last_autosave.elapsed() >= AUTOSAVE_INTERVAL
        {
            self.autosave_now();
        }

        self.sync_window_title(ui.ctx());
        let canvas_rect = self.show_shell(ui);
        self.handle_shortcuts(ui.ctx(), canvas_rect);
        self.welcome_window(ui.ctx());
        self.recovery_window(ui.ctx());
        self.unsaved_changes_window(ui.ctx());
        self.about_window(ui.ctx());
        self.help_window(ui.ctx());
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            SETTINGS_KEY,
            &PersistedSettings {
                theme: self.theme,
                tool: self.tool,
                grid_enabled: self.grid_enabled,
                clean_rendering: self.clean_rendering,
                recent: self.recent.clone(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use eframe::App as _;
    use eframe::Storage as _;

    use super::*;

    /// Renders one shell frame into a headless egui context of the given size
    /// and returns the canvas rect plus the full screen rect.
    fn layout_at(width: f32, height: f32, theme: Theme) -> (egui::Rect, egui::Rect) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let ctx = egui::Context::default();
        theme::apply(&ctx, theme);
        let mut app = EpsilonApp {
            theme,
            ..Default::default()
        };
        let mut canvas = None;

        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| canvas = Some(app.show_shell(ui)),
        );
        output.drop_without_applying_deltas();

        (canvas.expect("shell layout runs one frame"), screen)
    }

    fn assert_shell_fits(width: f32, height: f32, theme: Theme) {
        let (canvas, screen) = layout_at(width, height, theme);
        let tolerance = 1.0;

        assert!(
            canvas.width() >= layout::CANVAS_MIN_WIDTH - tolerance
                && canvas.height() >= layout::CANVAS_MIN_HEIGHT - tolerance,
            "canvas {canvas:?} too small in {width}x{height}"
        );
        // Each region consumed its reserved space around the canvas: tool rail
        // left, menu bar + top toolbar above, properties panel right, status bar
        // below. If a panel failed to lay out, the canvas would expand past one
        // of these edges.
        assert!(
            canvas.min.x >= layout::TOOL_RAIL_WIDTH - tolerance,
            "tool rail missing in {width}x{height}: {canvas:?}"
        );
        assert!(
            canvas.min.y >= layout::MENU_BAR_HEIGHT + layout::TOP_TOOLBAR_HEIGHT - tolerance,
            "menu bar/toolbar missing in {width}x{height}: {canvas:?}"
        );
        assert!(
            canvas.max.x <= screen.max.x - layout::PROPERTIES_PANEL_MIN_WIDTH + tolerance,
            "properties panel missing in {width}x{height}: {canvas:?}"
        );
        assert!(
            canvas.max.y <= screen.max.y - layout::STATUS_BAR_HEIGHT + tolerance,
            "status bar missing in {width}x{height}: {canvas:?}"
        );
    }

    #[test]
    fn shell_fits_minimum_window() {
        let [width, height] = layout::MIN_WINDOW_SIZE;
        assert_shell_fits(width, height, Theme::Dark);
    }

    #[test]
    fn shell_fits_large_window() {
        assert_shell_fits(1920.0, 1080.0, Theme::Dark);
    }

    #[test]
    fn switching_theme_updates_context_immediately() {
        let mut app = EpsilonApp::default();
        let ctx = egui::Context::default();
        theme::apply(&ctx, app.theme);

        app.set_theme(&ctx, Theme::Light);
        assert_eq!(app.theme, Theme::Light);
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert!(!ctx.style_of(egui::Theme::Light).visuals.dark_mode);

        app.set_theme(&ctx, Theme::Dark);
        assert_eq!(app.theme, Theme::Dark);
        assert!(ctx.style_of(egui::Theme::Dark).visuals.dark_mode);
    }

    fn run_app_frame(
        app: &mut EpsilonApp,
        ctx: &egui::Context,
        screen: egui::Rect,
        events: Vec<egui::Event>,
    ) {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |ui| {
                let _ = app.show_shell(ui);
            },
        );
        output.drop_without_applying_deltas();
    }

    fn drag_shape_on_canvas(tool: Tool) -> EpsilonApp {
        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let mut app = EpsilonApp {
            tool,
            ..Default::default()
        };

        let frame = |app: &mut EpsilonApp, events: Vec<egui::Event>| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let _ = app.show_shell(ui);
                },
            );
            output.drop_without_applying_deltas();
        };

        frame(&mut app, Vec::new());
        let canvas = app.canvas_rect;
        let start = canvas.center() - egui::vec2(40.0, 30.0);
        let end = canvas.center() + egui::vec2(40.0, 30.0);

        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(end)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        app
    }

    #[test]
    fn rectangle_drag_commits_on_release() {
        let app = drag_shape_on_canvas(Tool::Rectangle);
        assert_eq!(app.document.elements.len(), 1);
        let element = &app.document.elements[0];
        assert_eq!(element.kind, ElementKind::Rectangle);
        let expected_width = 80.0 / app.viewport.zoom;
        let expected_height = 60.0 / app.viewport.zoom;
        assert!((element.width - expected_width).abs() < 0.01);
        assert!((element.height - expected_height).abs() < 0.01);
    }

    #[test]
    fn ellipse_drag_commits_on_release() {
        let app = drag_shape_on_canvas(Tool::Ellipse);
        assert_eq!(app.document.elements.len(), 1);
        let element = &app.document.elements[0];
        assert_eq!(element.kind, ElementKind::Ellipse);
        let expected_width = 80.0 / app.viewport.zoom;
        let expected_height = 60.0 / app.viewport.zoom;
        assert!((element.width - expected_width).abs() < 0.01);
        assert!((element.height - expected_height).abs() < 0.01);
    }

    #[test]
    fn diamond_drag_commits_on_release() {
        let app = drag_shape_on_canvas(Tool::Diamond);
        assert_eq!(app.document.elements.len(), 1);
        let element = &app.document.elements[0];
        assert_eq!(element.kind, ElementKind::Diamond);
        let expected_width = 80.0 / app.viewport.zoom;
        let expected_height = 60.0 / app.viewport.zoom;
        assert!((element.width - expected_width).abs() < 0.01);
        assert!((element.height - expected_height).abs() < 0.01);
    }

    #[test]
    fn line_drag_stores_world_space_endpoints() {
        let app = drag_shape_on_canvas(Tool::Line);
        assert_eq!(app.document.elements.len(), 1);
        let element = &app.document.elements[0];
        assert_eq!(element.kind, ElementKind::Line);
        let [start, end] = element.points.as_slice() else {
            panic!("line should store exactly two endpoints");
        };
        assert!(((end.x - start.x) * app.viewport.zoom - 80.0).abs() < 0.01);
        assert!(((end.y - start.y) * app.viewport.zoom - 60.0).abs() < 0.01);
        assert!((element.x - start.x.min(end.x)).abs() < 0.01);
        assert!((element.y - start.y.min(end.y)).abs() < 0.01);
        assert!((element.width - (end.x - start.x).abs()).abs() < 0.01);
        assert!((element.height - (end.y - start.y).abs()).abs() < 0.01);
    }

    #[test]
    fn arrow_drag_stores_world_space_endpoints() {
        let app = drag_shape_on_canvas(Tool::Arrow);
        assert_eq!(app.document.elements.len(), 1);
        let element = &app.document.elements[0];
        assert_eq!(element.kind, ElementKind::Arrow);
        let [start, end] = element.points.as_slice() else {
            panic!("arrow should store exactly two endpoints");
        };
        assert!(((end.x - start.x) * app.viewport.zoom - 80.0).abs() < 0.01);
        assert!(((end.y - start.y) * app.viewport.zoom - 60.0).abs() < 0.01);
    }

    #[test]
    fn freehand_drag_commits_polyline() {
        let app = drag_shape_on_canvas(Tool::Freehand);
        assert_eq!(app.document.elements.len(), 1);
        let element = &app.document.elements[0];
        assert_eq!(element.kind, ElementKind::Freehand);
        assert!(element.points.len() >= 2);
        assert!(element.points.len() <= MAX_FREEHAND_POINTS);
    }

    #[test]
    fn line_commit_allows_thin_lines_but_discards_short_ones() {
        let mut app = EpsilonApp::default();
        app.commit_shape(ElementKind::Line, Point::ZERO, Point::new(10.0, 0.0));
        assert_eq!(app.document.elements.len(), 1);

        app.commit_shape(ElementKind::Line, Point::ZERO, Point::new(2.0, 0.0));
        assert_eq!(app.document.elements.len(), 1);
    }

    #[test]
    fn line_constraint_applies_only_to_constrained_lines() {
        let start = Point::new(10.0, 20.0);
        let end = Point::new(110.0, 30.0);

        let constrained = drawing_end(ElementKind::Line, start, end, true);
        assert!((constrained.y - start.y).abs() < 1e-9);
        assert_eq!(drawing_end(ElementKind::Rectangle, start, end, true), end);
        assert_eq!(drawing_end(ElementKind::Line, start, end, false), end);
        assert_eq!(
            drawing_end(ElementKind::Arrow, start, end, true),
            constrained
        );
    }

    #[test]
    fn rough_rendering_500_elements_stays_within_budget() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let mut app = EpsilonApp::default();
        for index in 0..500 {
            let x = (index % 25) as f64 * 30.0;
            let y = (index / 25) as f64 * 25.0;
            let mut element = Element::line(Point::new(x, y), Point::new(x + 24.0, y + 18.0));
            element.style.stroke = DocumentColor::rgb(220, 220, 220);
            app.document.add(element);
        }

        let started = std::time::Instant::now();
        run_app_frame(&mut app, &ctx, screen, Vec::new());
        let elapsed = started.elapsed();

        assert_eq!(app.document.elements.len(), 500);
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "500-element rough render took {elapsed:?}"
        );
    }

    #[test]
    fn styled_elements_render_headlessly() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let mut app = EpsilonApp::default();
        let mut rectangle = Element::new(ElementKind::Rectangle, 10.0, 10.0, 80.0, 60.0);
        rectangle.style.fill = DocumentColor::rgb(40, 120, 200);
        rectangle.style.fill_style = FillStyle::CrossHatch;
        rectangle.style.stroke_style = StrokeStyle::Dashed;
        rectangle.style.roundness = 8.0;
        app.document.add(rectangle);
        let mut ellipse = Element::new(ElementKind::Ellipse, 120.0, 10.0, 80.0, 60.0);
        ellipse.style.fill = DocumentColor::rgb(200, 80, 40);
        ellipse.style.fill_style = FillStyle::Hatch;
        ellipse.style.stroke_style = StrokeStyle::Dotted;
        ellipse.style.opacity = 0.5;
        app.document.add(ellipse);
        let mut line = Element::line(Point::new(10.0, 100.0), Point::new(200.0, 150.0));
        line.style.stroke_style = StrokeStyle::Dashed;
        app.document.add(line);

        run_app_frame(&mut app, &ctx, screen, Vec::new());
        assert_eq!(app.document.elements.len(), 3);
    }

    #[test]
    fn selection_properties_report_mixed_values_and_edit_live() {
        let mut app = EpsilonApp::default();
        let first = app
            .document
            .add(Element::new(ElementKind::Rectangle, 0.0, 10.0, 100.0, 40.0));
        let second = app.document.add(Element::new(
            ElementKind::Rectangle,
            100.0,
            10.0,
            100.0,
            40.0,
        ));
        app.selected.extend([first, second]);

        let properties =
            EpsilonApp::selection_properties(&app.document, &app.selected).expect("properties");
        assert_eq!(properties.x, None);
        assert_eq!(properties.y, Some(10.0));
        assert_eq!(properties.width, Some(100.0));
        assert_eq!(properties.height, Some(40.0));
        assert_eq!(properties.angle_degrees, Some(0.0));

        EpsilonApp::set_selection_x(&mut app.document, &app.selected, 0.0);
        EpsilonApp::set_selection_width(&mut app.document, &app.selected, 200.0);
        EpsilonApp::set_selection_angle(&mut app.document, &app.selected, 30.0);

        let first_element = app.document.element(first).expect("first");
        let second_element = app.document.element(second).expect("second");
        assert_eq!(first_element.x, 0.0);
        assert_eq!(second_element.x, 100.0);
        assert_eq!(first_element.width, 200.0);
        assert_eq!(second_element.width, 200.0);
        assert!((first_element.angle.to_degrees() - 30.0).abs() < 1e-9);
        assert!((second_element.angle.to_degrees() - 30.0).abs() < 1e-9);
        app.document
            .elements
            .iter_mut()
            .find(|element| element.id == first)
            .expect("first")
            .style
            .stroke_width = 5.0;

        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        run_app_frame(
            &mut app,
            &ctx,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0)),
            Vec::new(),
        );
    }

    #[test]
    fn style_changes_apply_to_every_selected_element() {
        let mut app = EpsilonApp::default();
        let first = app
            .document
            .add(Element::new(ElementKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
        let second = app
            .document
            .add(Element::new(ElementKind::Ellipse, 20.0, 0.0, 10.0, 10.0));
        app.selected.extend([first, second]);
        let mut style = app.document.elements[0].style;
        style.stroke = DocumentColor::rgb(200, 20, 40);
        style.fill = DocumentColor::rgb(40, 120, 200);
        style.stroke_style = StrokeStyle::Dotted;
        style.fill_style = FillStyle::Hatch;

        EpsilonApp::apply_style_to_selection(&mut app.document, &app.selected, style);

        assert_eq!(app.document.element(first).expect("first").style, style);
        assert_eq!(app.document.element(second).expect("second").style, style);
    }

    #[test]
    fn moving_selection_translates_all_selected_geometry() {
        let mut app = EpsilonApp::default();
        let rectangle =
            app.document
                .add(Element::new(ElementKind::Rectangle, 10.0, 20.0, 40.0, 30.0));
        let line = app
            .document
            .add(Element::line(Point::new(0.0, 0.0), Point::new(10.0, 5.0)));
        app.selected.extend([rectangle, line]);
        app.transform_original = app.selected_snapshots();

        app.apply_move_delta(Point::new(7.0, -3.0));

        let rectangle = app.document.element(rectangle).expect("rectangle");
        assert_eq!((rectangle.x, rectangle.y), (17.0, 17.0));
        let line = app.document.element(line).expect("line");
        assert_eq!(line.points[0], Point::new(7.0, -3.0));
        assert_eq!(line.points[1], Point::new(17.0, 2.0));
    }

    #[test]
    fn resizing_selection_scales_multiple_elements_from_snapshots() {
        let mut app = EpsilonApp::default();
        let first = app
            .document
            .add(Element::new(ElementKind::Rectangle, 0.0, 0.0, 100.0, 50.0));
        let second = app.document.add(Element::new(
            ElementKind::Rectangle,
            100.0,
            0.0,
            100.0,
            50.0,
        ));
        app.selected.extend([first, second]);
        app.transform_original = app.selected_snapshots();
        app.resize_start_bounds = app.selection_bounds().expect("selected bounds");

        app.apply_resize_to(Rect::from_xywh(0.0, 0.0, 400.0, 100.0));

        let first = app.document.element(first).expect("first");
        assert_eq!((first.width, first.height), (200.0, 100.0));
        let second = app.document.element(second).expect("second");
        assert_eq!(second.x, 200.0);
        assert_eq!((second.width, second.height), (200.0, 100.0));
    }

    #[test]
    fn rotation_rotates_selection_around_collective_center() {
        let mut app = EpsilonApp::default();
        let first = app
            .document
            .add(Element::new(ElementKind::Rectangle, 0.0, 0.0, 50.0, 50.0));
        let second = app
            .document
            .add(Element::new(ElementKind::Rectangle, 100.0, 0.0, 50.0, 50.0));
        app.selected.extend([first, second]);
        app.transform_original = app.selected_snapshots();
        app.rotation_center = app.selection_bounds().expect("bounds").center();

        app.apply_rotation(std::f64::consts::PI);

        let first = app.document.element(first).expect("first");
        assert!((first.angle - std::f64::consts::PI).abs() < 1e-9);
        assert!((first.x - 100.0).abs() < 1e-9);
        let second = app.document.element(second).expect("second");
        assert!((second.x - 0.0).abs() < 1e-9);
    }

    #[test]
    fn rotated_selection_keeps_screen_space_handles() {
        let mut app = EpsilonApp::default();
        let id = app
            .document
            .add(Element::new(ElementKind::Rectangle, 0.0, 0.0, 100.0, 50.0));
        app.selected.insert(id);
        app.transform_original = app.selected_snapshots();
        app.rotation_center = app.selection_bounds().expect("bounds").center();
        app.apply_rotation(std::f64::consts::FRAC_PI_2);

        let bounds = app.selection_bounds().expect("rotated bounds");
        let screen = world_rect_screen(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
            &Viewport::default(),
            bounds,
        );
        assert_eq!(handle_screen_positions(screen).len(), 8);
        assert!((screen.width() - 50.0).abs() < 0.01);
        assert!((screen.height() - 100.0).abs() < 0.01);
    }

    #[test]
    fn selection_handles_are_eight_screen_points() {
        let bounds = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 50.0));
        let handles = handle_screen_positions(bounds);
        assert_eq!(handles.len(), 8);
        assert_eq!(handles[0].0, ResizeHandle::NorthWest);
        assert_eq!(handles[0].1, egui::pos2(10.0, 20.0));
        assert_eq!(handles[4].0, ResizeHandle::SouthEast);
        assert_eq!(handles[4].1, egui::pos2(110.0, 70.0));
    }

    #[test]
    fn selection_click_picks_topmost_and_shift_toggles() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let mut app = EpsilonApp::default();
        let mut first_element = Element::new(ElementKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
        first_element.style.fill = DocumentColor::WHITE;
        let mut second_element = Element::new(ElementKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
        second_element.style.fill = DocumentColor::WHITE;
        let first = app.document.add(first_element);
        let second = app.document.add(second_element);

        run_app_frame(&mut app, &ctx, screen, Vec::new());
        let point = app.canvas_rect.min + egui::vec2(50.0, 50.0);
        let press = |modifiers: egui::Modifiers| {
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                },
            ]
        };

        run_app_frame(&mut app, &ctx, screen, press(egui::Modifiers::NONE));
        assert_eq!(app.selected.len(), 1);
        assert!(app.selected.contains(&second));
        assert!(!app.selected.contains(&first));

        let release = vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }];
        run_app_frame(&mut app, &ctx, screen, release);

        app.toggle_selected(second, true);
        assert!(app.selected.is_empty());
        app.toggle_selected(first, true);
        assert!(app.selected.contains(&first));
    }

    #[test]
    fn marquee_selects_intersecting_elements() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let mut app = EpsilonApp::default();
        let id = app
            .document
            .add(Element::new(ElementKind::Rectangle, 0.0, 0.0, 100.0, 100.0));

        run_app_frame(&mut app, &ctx, screen, Vec::new());
        let start = app.canvas_rect.min + egui::vec2(200.0, 200.0);
        let end = app.canvas_rect.min + egui::vec2(50.0, 50.0);
        run_app_frame(
            &mut app,
            &ctx,
            screen,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        run_app_frame(&mut app, &ctx, screen, vec![egui::Event::PointerMoved(end)]);
        run_app_frame(
            &mut app,
            &ctx,
            screen,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );

        assert_eq!(app.selected.len(), 1);
        assert!(app.selected.contains(&id));
    }

    #[test]
    fn text_click_creates_element_and_opens_editor() {
        let mut app = EpsilonApp::default();
        app.commit_shape(
            ElementKind::Text,
            Point::new(10.0, 20.0),
            Point::new(10.0, 20.0),
        );

        assert_eq!(app.document.elements.len(), 1);
        let element = &app.document.elements[0];
        assert_eq!(element.kind, ElementKind::Text);
        assert_eq!(element.width, DEFAULT_TEXT_WIDTH);
        assert_eq!(element.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(app.editing_text, Some(element.id));
    }

    #[test]
    fn text_drag_sets_wrap_width_and_empty_text_is_discarded() {
        let mut app = EpsilonApp::default();
        app.commit_shape(
            ElementKind::Text,
            Point::new(0.0, 0.0),
            Point::new(120.0, 30.0),
        );
        assert_eq!(app.document.elements[0].width, 120.0);

        let ctx = egui::Context::default();
        app.commit_text_edit(&ctx);
        assert!(app.document.elements.is_empty());
        assert!(app.editing_text.is_none());
    }

    #[test]
    fn text_edit_commits_content_and_measured_height() {
        let mut app = EpsilonApp::default();
        app.commit_shape(ElementKind::Text, Point::ZERO, Point::ZERO);
        app.text_buffer = "First line\nSecond line wraps".to_owned();

        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |_ui| {});
        output.drop_without_applying_deltas();
        app.commit_text_edit(&ctx);

        let element = &app.document.elements[0];
        assert_eq!(element.text, "First line\nSecond line wraps");
        assert!(element.height >= element.font_size * 1.25);
        assert!(app.editing_text.is_none());
        assert!(app.text_buffer.is_empty());
    }

    #[test]
    fn escape_commits_the_text_editor() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        let mut app = EpsilonApp::default();
        let id = app.document.add(Element::text(
            10.0,
            20.0,
            DEFAULT_TEXT_WIDTH,
            DEFAULT_FONT_SIZE,
        ));
        app.editing_text = Some(id);
        app.text_buffer = "Edited text".to_owned();

        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 700.0),
                )),
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                let _ = app.show_shell(ui);
            },
        );
        output.drop_without_applying_deltas();

        assert!(app.editing_text.is_none());
        assert_eq!(app.document.elements[0].text, "Edited text");
    }

    #[test]
    fn rectangle_commit_stores_normalized_world_bounds() {
        let mut app = EpsilonApp {
            viewport: Viewport {
                offset: Point::new(20.0, 30.0),
                zoom: 2.0,
            },
            ..Default::default()
        };

        app.commit_shape(
            ElementKind::Rectangle,
            Point::new(10.0, 20.0),
            Point::new(4.0, 8.0),
        );

        assert_eq!(app.document.elements.len(), 1);
        let element = &app.document.elements[0];
        assert_eq!(element.kind, ElementKind::Rectangle);
        assert_eq!(element.x, 4.0);
        assert_eq!(element.y, 8.0);
        assert_eq!(element.width, 6.0);
        assert_eq!(element.height, 12.0);
    }

    #[test]
    fn shape_commit_discards_drags_below_minimum_screen_size() {
        let mut app = EpsilonApp::default();
        app.commit_shape(ElementKind::Ellipse, Point::ZERO, Point::new(1.0, 3.0));
        assert!(app.document.elements.is_empty());

        app.commit_shape(ElementKind::Ellipse, Point::ZERO, Point::new(3.0, 3.0));
        assert_eq!(app.document.elements.len(), 1);
    }

    #[test]
    fn ellipse_screen_rect_matches_viewport_at_extreme_zoom() {
        let canvas = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(800.0, 600.0));
        let element = Element::with_id(
            epsilon_core::document::ElementId::new(1),
            ElementKind::Ellipse,
            100.0,
            200.0,
            50.0,
            30.0,
        );

        for (zoom, offset, expected) in [
            (
                0.1,
                Point::new(10.0, 20.0),
                egui::Rect::from_min_size(egui::pos2(30.0, 60.0), egui::vec2(5.0, 3.0)),
            ),
            (
                10.0,
                Point::new(-100.0, -200.0),
                egui::Rect::from_min_size(egui::pos2(910.0, 1820.0), egui::vec2(500.0, 300.0)),
            ),
        ] {
            let viewport = Viewport { offset, zoom };
            let actual = element_screen_rect(canvas, &viewport, &element);
            assert!((actual.min.x - expected.min.x).abs() < 0.01);
            assert!((actual.min.y - expected.min.y).abs() < 0.01);
            assert!((actual.width() - expected.width()).abs() < 0.01);
            assert!((actual.height() - expected.height()).abs() < 0.01);
        }
    }

    #[test]
    fn diamond_screen_points_touch_element_bounds_at_zoom() {
        let canvas = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(800.0, 600.0));
        let element = Element::with_id(
            epsilon_core::document::ElementId::new(1),
            ElementKind::Diamond,
            100.0,
            200.0,
            50.0,
            30.0,
        );

        for (zoom, offset) in [
            (0.1, Point::new(10.0, 20.0)),
            (10.0, Point::new(-100.0, -200.0)),
        ] {
            let viewport = Viewport { offset, zoom };
            let screen = element_screen_rect(canvas, &viewport, &element);
            let bounds = Rect::from_xywh(element.x, element.y, element.width, element.height);
            let points = diamond_screen_points(canvas, &viewport, bounds);
            let expected = [
                egui::pos2(screen.center().x, screen.top()),
                egui::pos2(screen.right(), screen.center().y),
                egui::pos2(screen.center().x, screen.bottom()),
                egui::pos2(screen.left(), screen.center().y),
            ];

            for (point, expected) in points.into_iter().zip(expected) {
                assert!((point.x - expected.x).abs() < 0.01);
                assert!((point.y - expected.y).abs() < 0.01);
            }
        }
    }

    #[test]
    fn element_screen_rect_matches_viewport_at_extreme_zoom() {
        let canvas = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(800.0, 600.0));
        let element = Element::with_id(
            epsilon_core::document::ElementId::new(1),
            ElementKind::Rectangle,
            100.0,
            200.0,
            50.0,
            30.0,
        );

        for (zoom, offset, expected) in [
            (
                0.1,
                Point::new(10.0, 20.0),
                egui::Rect::from_min_size(egui::pos2(30.0, 60.0), egui::vec2(5.0, 3.0)),
            ),
            (
                10.0,
                Point::new(-100.0, -200.0),
                egui::Rect::from_min_size(egui::pos2(910.0, 1820.0), egui::vec2(500.0, 300.0)),
            ),
        ] {
            let viewport = Viewport { offset, zoom };
            let actual = element_screen_rect(canvas, &viewport, &element);
            assert!((actual.min.x - expected.min.x).abs() < 0.01);
            assert!((actual.min.y - expected.min.y).abs() < 0.01);
            assert!((actual.width() - expected.width()).abs() < 0.01);
            assert!((actual.height() - expected.height()).abs() < 0.01);
        }
    }

    #[test]
    fn draft_screen_rect_normalizes_reverse_drags() {
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let viewport = Viewport::default();
        let forward = draft_screen_rect(
            canvas,
            &viewport,
            Point::new(10.0, 20.0),
            Point::new(4.0, 8.0),
        );
        let reverse = draft_screen_rect(
            canvas,
            &viewport,
            Point::new(4.0, 8.0),
            Point::new(10.0, 20.0),
        );
        assert_eq!(forward, reverse);
        assert_eq!(forward.min, egui::pos2(4.0, 8.0));
        assert_eq!(forward.max, egui::pos2(10.0, 20.0));
    }

    fn run_shortcut(app: &mut EpsilonApp, key: egui::Key, modifiers: egui::Modifiers) {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let canvas_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
        let output = ctx.run_ui(input, |ui| app.handle_shortcuts(ui.ctx(), canvas_rect));
        output.drop_without_applying_deltas();
    }

    #[derive(Default)]
    struct TestStorage(std::collections::HashMap<String, String>);

    impl eframe::Storage for TestStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn settings_round_trip_through_storage() {
        let mut app = EpsilonApp {
            theme: Theme::Light,
            tool: Tool::Arrow,
            clean_rendering: true,
            ..Default::default()
        };
        app.recent.record("/tmp/recent.epsilon");
        let mut storage = TestStorage::default();
        eframe::App::save(&mut app, &mut storage);

        let restored = EpsilonApp::from_storage(Some(&storage));
        assert_eq!(restored.theme, Theme::Light);
        assert_eq!(restored.tool, Tool::Arrow);
        assert!(restored.clean_rendering);
        assert_eq!(
            restored.recent.paths(),
            &[PathBuf::from("/tmp/recent.epsilon")]
        );
    }

    #[test]
    fn missing_settings_use_defaults() {
        let storage = TestStorage::default();
        let restored = EpsilonApp::from_storage(Some(&storage));
        assert_eq!(restored.theme, Theme::Dark);
        assert_eq!(restored.tool, Tool::Select);

        let restored = EpsilonApp::from_storage(None);
        assert_eq!(restored.theme, Theme::Dark);
        assert_eq!(restored.tool, Tool::Select);
    }

    #[test]
    fn corrupt_settings_use_defaults() {
        let mut storage = TestStorage::default();
        storage.set_string(SETTINGS_KEY, "not valid ron {{".to_owned());

        let restored = EpsilonApp::from_storage(Some(&storage));
        assert_eq!(restored.theme, Theme::Dark);
        assert_eq!(restored.tool, Tool::Select);
    }

    #[test]
    fn pointer_mode_pans_with_middle_click() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        let mut app = EpsilonApp::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events: vec![
                egui::Event::PointerMoved(egui::pos2(100.0, 100.0)),
                egui::Event::PointerButton {
                    pos: egui::pos2(100.0, 100.0),
                    button: egui::PointerButton::Middle,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            let _ = app.show_shell(ui);
        });
        output.drop_without_applying_deltas();
        assert!(matches!(app.pointer_mode, PointerMode::Panning { .. }));
    }

    #[test]
    fn pointer_mode_dragging_variant_is_representable() {
        let app = EpsilonApp {
            pointer_mode: PointerMode::Dragging {
                kind: ElementKind::Ellipse,
                start: Point::ZERO,
                current: Point::ZERO,
            },
            ..Default::default()
        };
        assert!(matches!(app.pointer_mode, PointerMode::Dragging { .. }));
    }

    #[test]
    fn egui_memory_is_persisted_for_panel_widths() {
        // Resizable panel sizes live in egui memory; eframe must keep saving it
        // for the properties panel width to survive a restart.
        assert!(EpsilonApp::default().persist_egui_memory());
    }

    #[test]
    fn keyboard_shortcuts_select_tools() {
        let mut app = EpsilonApp::default();

        run_shortcut(&mut app, egui::Key::R, egui::Modifiers::default());
        assert_eq!(app.tool, Tool::Rectangle);

        run_shortcut(&mut app, egui::Key::T, egui::Modifiers::default());
        assert_eq!(app.tool, Tool::Text);

        run_shortcut(&mut app, egui::Key::V, egui::Modifiers::default());
        assert_eq!(app.tool, Tool::Select);
    }

    #[test]
    fn modified_keys_do_not_change_tools() {
        let mut app = EpsilonApp {
            tool: Tool::Hand,
            ..Default::default()
        };

        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        run_shortcut(&mut app, egui::Key::R, ctrl);
        assert_eq!(app.tool, Tool::Hand);
    }

    #[test]
    fn about_action_opens_dialog() {
        let mut app = EpsilonApp::default();
        assert!(!app.about_open);
        app.open_about();
        assert!(app.about_open);
    }

    #[test]
    fn quit_action_requests_window_close() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |ui| request_quit(ui.ctx()));
        let close_requested = output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Close));
        output.drop_without_applying_deltas();
        assert!(close_requested, "quit must send ViewportCommand::Close");
    }

    #[test]
    fn theme_toggle_flips_and_applies_immediately() {
        let ctx = egui::Context::default();
        let mut app = EpsilonApp::default();
        theme::apply(&ctx, app.theme);

        app.toggle_theme(&ctx);
        assert_eq!(app.theme, Theme::Light);
        assert_eq!(ctx.theme(), egui::Theme::Light);

        app.toggle_theme(&ctx);
        assert_eq!(app.theme, Theme::Dark);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
    }

    #[test]
    fn shortcut_bindings_have_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for binding in KEYBINDINGS {
            let signature = format!("{:?}|{:?}", binding.key, binding.modifiers);
            assert!(
                seen.insert(signature),
                "duplicate shortcut binding for {}",
                binding.shortcut
            );
        }
    }

    #[test]
    fn shortcut_bindings_cover_all_tools() {
        for tool in Tool::ALL {
            assert!(
                KEYBINDINGS.iter().any(|binding| {
                    matches!(binding.action, Action::SelectTool(found) if found == tool)
                }),
                "{} has no shortcut binding",
                tool.name()
            );
        }
    }

    fn redo_shortcut_modifiers() -> egui::Modifiers {
        egui::Modifiers {
            shift: true,
            command: true,
            ..egui::Modifiers::NONE
        }
    }

    #[test]
    fn shape_creation_is_undoable_and_redoable_with_shortcuts() {
        let mut app = drag_shape_on_canvas(Tool::Rectangle);
        assert_eq!(app.document.elements.len(), 1);
        assert!(app.can_undo());
        assert!(!app.can_redo());

        run_shortcut(&mut app, egui::Key::Z, egui::Modifiers::COMMAND);
        assert!(app.document.elements.is_empty());
        assert!(!app.can_undo());
        assert!(app.can_redo());

        run_shortcut(&mut app, egui::Key::Z, redo_shortcut_modifiers());
        assert_eq!(app.document.elements.len(), 1);

        run_shortcut(&mut app, egui::Key::Z, egui::Modifiers::COMMAND);
        assert!(app.document.elements.is_empty());
        run_shortcut(&mut app, egui::Key::Y, egui::Modifiers::COMMAND);
        assert_eq!(app.document.elements.len(), 1);
    }

    #[test]
    fn every_drawing_tool_creation_is_undoable() {
        for tool in [
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Diamond,
            Tool::Line,
            Tool::Arrow,
            Tool::Freehand,
        ] {
            let mut app = drag_shape_on_canvas(tool);
            assert_eq!(
                app.document.elements.len(),
                1,
                "{} should create one element",
                tool.name()
            );
            assert!(
                app.can_undo(),
                "{} creation should be undoable",
                tool.name()
            );

            run_shortcut(&mut app, egui::Key::Z, egui::Modifiers::COMMAND);
            assert!(
                app.document.elements.is_empty(),
                "{} undo should remove the element",
                tool.name()
            );

            run_shortcut(&mut app, egui::Key::Y, egui::Modifiers::COMMAND);
            assert_eq!(
                app.document.elements.len(),
                1,
                "{} redo should restore the element",
                tool.name()
            );
        }
    }

    #[test]
    fn text_creation_is_recorded_when_the_editor_commits() {
        let mut app = EpsilonApp::default();
        app.commit_shape(ElementKind::Text, Point::ZERO, Point::ZERO);
        assert!(app.editing_text.is_some());
        assert!(!app.can_undo(), "pending text must not be undoable");
        assert!(!app.undo());

        app.text_buffer = "Undoable text".to_owned();
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |_ui| {});
        output.drop_without_applying_deltas();
        app.commit_text_edit(&ctx);

        assert_eq!(app.document.elements.len(), 1);
        assert!(app.can_undo());
        run_shortcut(&mut app, egui::Key::Z, egui::Modifiers::COMMAND);
        assert!(app.document.elements.is_empty());

        run_shortcut(&mut app, egui::Key::Z, redo_shortcut_modifiers());
        assert_eq!(app.document.elements.len(), 1);
        assert_eq!(app.document.elements[0].text, "Undoable text");
    }

    #[test]
    fn editing_existing_text_is_undoable() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |_ui| {});
        output.drop_without_applying_deltas();

        let mut app = EpsilonApp::default();
        app.commit_shape(ElementKind::Text, Point::ZERO, Point::ZERO);
        app.text_buffer = "First".to_owned();
        app.commit_text_edit(&ctx);
        assert_eq!(app.document.elements[0].text, "First");

        let id = app.document.elements[0].id;
        app.editing_text = Some(id);
        app.editing_text_is_new = false;
        app.text_buffer = "Second".to_owned();
        app.commit_text_edit(&ctx);

        assert_eq!(app.document.elements[0].text, "Second");
        assert_eq!(app.history.undo_len(), 2);

        run_shortcut(&mut app, egui::Key::Z, egui::Modifiers::COMMAND);
        assert_eq!(app.document.elements[0].text, "First");
        run_shortcut(&mut app, egui::Key::Z, redo_shortcut_modifiers());
        assert_eq!(app.document.elements[0].text, "Second");
    }

    #[test]
    fn mixed_stage_three_edits_undo_and_redo_exactly() {
        let mut app = EpsilonApp::default();
        app.commit_shape(
            ElementKind::Rectangle,
            Point::new(0.0, 0.0),
            Point::new(100.0, 80.0),
        );
        let id = app.document.elements[0].id;
        app.selected.insert(id);

        app.transform_original = app.selected_snapshots();
        app.apply_move_delta(Point::new(20.0, 10.0));
        app.finish_transform();

        app.transform_original = app.selected_snapshots();
        app.resize_start_bounds = app.selection_bounds().expect("selection bounds");
        let resized = Rect::from_xywh(
            app.resize_start_bounds.min.x,
            app.resize_start_bounds.min.y,
            200.0,
            120.0,
        );
        app.apply_resize_to(resized);
        app.finish_transform();

        app.transform_original = app.selected_snapshots();
        app.rotation_center = app.selection_bounds().expect("selection bounds").center();
        app.apply_rotation(std::f64::consts::FRAC_PI_4);
        app.finish_transform();

        let before = app.selected_snapshots();
        EpsilonApp::set_selection_x(&mut app.document, &app.selected, 55.0);
        app.commit_property_edit(before);

        let before = app.selected_snapshots();
        let mut style = before[0].style;
        style.stroke = DocumentColor::rgb(200, 30, 30);
        EpsilonApp::apply_style_to_selection(&mut app.document, &app.selected, style);
        app.commit_property_edit(before);

        assert_eq!(app.history.undo_len(), 6);
        let final_document = app.document.clone();

        for step in 0..6 {
            assert!(app.undo(), "undo step {step} should apply");
        }
        assert!(app.document.elements.is_empty());
        assert!(!app.can_undo());

        for step in 0..6 {
            assert!(app.redo(), "redo step {step} should apply");
        }
        assert_eq!(app.document, final_document);
        assert!(!app.can_redo());
    }

    #[test]
    fn new_edit_after_undo_discards_the_redo_branch() {
        let mut app = drag_shape_on_canvas(Tool::Rectangle);
        run_shortcut(&mut app, egui::Key::Z, egui::Modifiers::COMMAND);
        assert!(app.can_redo());

        app.commit_shape(ElementKind::Ellipse, Point::ZERO, Point::new(20.0, 20.0));

        assert!(!app.can_redo());
        assert_eq!(app.document.elements.len(), 1);
        assert_eq!(app.document.elements[0].kind, ElementKind::Ellipse);
    }

    fn run_clipboard_event(app: &mut EpsilonApp, event: egui::Event) -> Vec<egui::OutputCommand> {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events: vec![event],
            ..Default::default()
        };
        let canvas_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
        let output = ctx.run_ui(input, |ui| app.handle_shortcuts(ui.ctx(), canvas_rect));
        let commands = output.platform_output.commands.clone();
        output.drop_without_applying_deltas();
        commands
    }

    fn copied_text(commands: &[egui::OutputCommand]) -> Option<String> {
        commands.iter().find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
    }

    fn file_test_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let dir = std::env::temp_dir().join(format!(
            "epsilon-app-{}-{}-{label}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn save_and_open_round_trip_updates_dirty_state() {
        let dir = file_test_dir("save-open");
        let path = dir.join("scene.epsilon");
        let mut app = selection_app();
        assert!(app.is_dirty(), "unsaved edits are dirty");
        assert!(app.window_title().contains('•'));

        assert!(app.save_to_path(&path));
        assert!(path.exists());
        assert!(!app.is_dirty());
        assert_eq!(app.file_path.as_deref(), Some(path.as_path()));
        assert_eq!(app.window_title(), "scene.epsilon — Epsilon");
        assert!(matches!(
            app.file_feedback,
            Some(FileFeedback::Saved(ref message)) if message == "Saved scene.epsilon"
        ));
        let saved = app.document.clone();

        app.commit_shape(
            ElementKind::Rectangle,
            Point::new(200.0, 200.0),
            Point::new(240.0, 240.0),
        );
        assert!(app.is_dirty());

        let mut opened = EpsilonApp::default();
        assert!(!opened.is_dirty(), "a fresh empty document is clean");
        assert!(opened.open_from_path(&path));
        assert_eq!(opened.document, saved);
        assert!(!opened.is_dirty());
        assert_eq!(opened.file_path.as_deref(), Some(path.as_path()));
        assert_eq!(opened.history.undo_len(), 0);
        assert!(matches!(
            opened.file_feedback,
            Some(FileFeedback::Opened(ref message)) if message == "Opened scene.epsilon"
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_errors_leave_the_current_document_untouched() {
        let dir = file_test_dir("open-error");
        let path = dir.join("broken.epsilon");
        std::fs::write(&path, "not json").expect("seed malformed file");

        let mut app = selection_app();
        let original = app.document.clone();
        assert!(!app.open_from_path(&path));
        assert_eq!(app.document, original);
        assert!(matches!(
            app.file_feedback,
            Some(FileFeedback::Error(ref message)) if message.contains("Open failed")
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failed_save_reports_the_error_and_keeps_the_document_dirty() {
        let dir = file_test_dir("save-error");
        let blocker = dir.join("not-a-directory");
        std::fs::write(&blocker, "x").expect("seed blocker");

        let mut app = selection_app();
        let original = app.document.clone();
        assert!(!app.save_to_path(&blocker.join("scene.epsilon")));
        assert_eq!(app.document, original);
        assert!(app.is_dirty());
        assert!(app.file_path.is_none());
        assert!(matches!(
            app.file_feedback,
            Some(FileFeedback::Error(ref message)) if message.contains("Save failed")
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_as_keeps_or_adds_the_epsilon_extension() {
        assert_eq!(
            with_document_extension(PathBuf::from("scene")),
            PathBuf::from("scene.epsilon")
        );
        assert_eq!(
            with_document_extension(PathBuf::from("scene.EPSILON")),
            PathBuf::from("scene.EPSILON")
        );
        assert_eq!(
            with_document_extension(PathBuf::from("scene.json")),
            PathBuf::from("scene.epsilon")
        );
    }

    #[test]
    fn new_document_resets_scene_and_dirty_state() {
        let mut app = selection_app();
        app.file_path = Some(PathBuf::from("scene.epsilon"));
        app.saved_document = app.document.clone();

        app.new_document();
        assert!(app.document.elements.is_empty());
        assert!(app.selected.is_empty());
        assert!(app.file_path.is_none());
        assert!(!app.is_dirty());
        assert_eq!(app.history.undo_len(), 0);
        assert!(matches!(
            app.file_feedback,
            Some(FileFeedback::New(ref message)) if message == "New document"
        ));
    }

    #[test]
    fn unsaved_guard_cancels_or_discards_new() {
        let ctx = egui::Context::default();
        let mut app = selection_app();
        let original = app.document.clone();

        app.request_new_document();
        assert_eq!(app.pending_action, Some(PendingAction::New));
        assert_eq!(app.document, original, "nothing changes before a choice");
        app.resolve_unsaved(&ctx, UnsavedChoice::Cancel);
        assert_eq!(app.pending_action, None);
        assert_eq!(app.document, original);

        app.request_new_document();
        app.resolve_unsaved(&ctx, UnsavedChoice::Discard);
        assert_eq!(app.pending_action, None);
        assert!(app.document.elements.is_empty());
        assert!(!app.is_dirty());
    }

    #[test]
    fn unsaved_guard_save_writes_the_scene_before_continuing() {
        let dir = file_test_dir("guard-save");
        let path = dir.join("scene.epsilon");
        let mut app = selection_app();
        assert!(app.save_to_path(&path));
        app.commit_shape(
            ElementKind::Rectangle,
            Point::new(200.0, 200.0),
            Point::new(240.0, 240.0),
        );
        let expected_count = app.document.elements.len();

        app.request_new_document();
        assert_eq!(app.pending_action, Some(PendingAction::New));
        app.resolve_unsaved(&egui::Context::default(), UnsavedChoice::Save);

        assert!(app.document.elements.is_empty());
        assert!(app.file_path.is_none());
        assert!(!app.is_dirty());
        let saved = EpsilonFile::load_from_path(&path).expect("guard save wrote the file");
        assert_eq!(saved.document.elements.len(), expected_count);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn autosave_is_offered_and_recovered_after_a_crash() {
        let dir = file_test_dir("recovery");
        let store = RecoveryStore::new(dir.join("recovery"));
        let source = dir.join("scene.epsilon");
        let mut app = selection_app();
        app.recovery_store = Some(store.clone());
        app.file_path = Some(source.clone());
        assert!(app.autosave_now(), "dirty document is autosaved");
        let saved_document = app.document.clone();

        // Simulate a crash: no normal save or recovery cleanup happens.
        let mut next = EpsilonApp {
            recovery_store: Some(store.clone()),
            ..Default::default()
        };
        assert!(next.offer_recovery());
        next.apply_recovery();

        assert_eq!(next.document, saved_document);
        assert_eq!(next.file_path.as_deref(), Some(source.as_path()));
        assert!(next.is_dirty(), "recovered work must prompt before saving");
        assert!(matches!(
            next.file_feedback,
            Some(FileFeedback::Recovered(ref message)) if message.contains("Recovered")
        ));
        assert!(
            store.latest_path().expect("list recovery").is_none(),
            "recovery files are removed after a decision"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recovering_old_autosave_does_not_reuse_live_history() {
        let dir = file_test_dir("recovery-history");
        let store = RecoveryStore::new(dir.join("recovery"));
        let mut app = selection_app();
        app.recovery_store = Some(store.clone());
        assert!(app.autosave_now());

        let mut next = EpsilonApp {
            recovery_store: Some(store.clone()),
            ..Default::default()
        };
        assert!(next.offer_recovery());
        next.apply_recovery();
        assert_eq!(next.history.undo_len(), 0);
        assert!(next.selected.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recovery_can_be_discarded_and_clean_documents_are_not_autosaved() {
        let dir = file_test_dir("recovery-discard");
        let store = RecoveryStore::new(dir.join("recovery"));

        let mut clean = EpsilonApp {
            recovery_store: Some(store.clone()),
            ..Default::default()
        };
        assert!(!clean.autosave_now());
        assert!(store.latest_path().expect("list recovery").is_none());

        let mut app = selection_app();
        app.recovery_store = Some(store.clone());
        assert!(app.autosave_now());
        let mut next = EpsilonApp {
            recovery_store: Some(store.clone()),
            ..Default::default()
        };
        assert!(next.offer_recovery());
        next.recovery_offer = None;
        next.discard_recovery_files();
        assert!(next.document.elements.is_empty());
        assert!(store.latest_path().expect("list recovery").is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_and_opening_record_recent_documents() {
        let dir = file_test_dir("recent");
        let path = dir.join("scene.epsilon");
        let mut app = selection_app();
        assert!(app.save_to_path(&path));
        assert_eq!(app.recent.paths(), std::slice::from_ref(&path));

        // Saving again moves the path to the front without duplicating it.
        assert!(app.save_to_path(&path));
        assert_eq!(app.recent.paths(), std::slice::from_ref(&path));

        let mut restored = EpsilonApp::default();
        assert!(restored.open_from_path(&path));
        assert_eq!(restored.recent.paths()[0], path);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_recent_files_are_removed_without_touching_the_scene() {
        let dir = file_test_dir("recent-missing");
        let missing = dir.join("gone.epsilon");
        let mut app = selection_app();
        let original = app.document.clone();
        app.recent.record(missing.clone());

        app.open_recent(&missing);
        assert!(app.recent.is_empty());
        assert_eq!(app.document, original);
        assert!(matches!(
            app.file_feedback,
            Some(FileFeedback::Error(ref message)) if message.contains("no longer exists")
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recent_open_respects_the_unsaved_guard() {
        let dir = file_test_dir("recent-guard");
        let path = dir.join("scene.epsilon");
        let mut saved = selection_app();
        assert!(saved.save_to_path(&path));
        let saved_document = saved.document.clone();

        let mut app = selection_app();
        app.commit_shape(
            ElementKind::Rectangle,
            Point::new(300.0, 300.0),
            Point::new(340.0, 340.0),
        );
        app.open_recent(&path);
        assert_eq!(
            app.pending_action,
            Some(PendingAction::Open(Some(path.clone())))
        );
        app.resolve_unsaved(&egui::Context::default(), UnsavedChoice::Discard);
        assert_eq!(app.document, saved_document);
        assert_eq!(app.recent.paths()[0], path);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn excalidraw_import_replaces_the_scene_and_reports_skipped_types() {
        let dir = file_test_dir("excalidraw-import");
        let path = dir.join("scene.excalidraw");
        std::fs::write(
            &path,
            r##"{"type":"excalidraw","version":2,"elements":[{"id":"r1","type":"rectangle","x":1,"y":2,"width":30,"height":40,"strokeColor":"#ff0000","backgroundColor":"transparent"},{"type":"embeddable","x":0,"y":0}],"appState":{}}"##,
        )
        .expect("write fixture");

        let mut app = EpsilonApp::default();
        assert!(app.import_excalidraw_from_path(&path));
        assert_eq!(app.document.elements.len(), 1);
        assert_eq!(app.document.elements[0].kind, ElementKind::Rectangle);
        assert!(app.file_path.is_none(), "imported scenes are unsaved");
        assert!(app.is_dirty());
        assert_eq!(app.history.undo_len(), 0);
        assert!(matches!(
            app.file_feedback,
            Some(FileFeedback::New(ref message))
                if message.contains("1 elements") && message.contains("1 unsupported")
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn excalidraw_import_respects_the_unsaved_guard() {
        let dir = file_test_dir("excalidraw-guard");
        let path = dir.join("scene.excalidraw");
        std::fs::write(
            &path,
            r##"{"elements":[{"type":"ellipse","x":5,"y":5,"width":10,"height":10,"strokeColor":"#000000","backgroundColor":"transparent"}]}"##,
        )
        .expect("write fixture");

        let mut app = selection_app();
        app.pending_action = Some(PendingAction::Import(path.clone()));
        app.resolve_unsaved(&egui::Context::default(), UnsavedChoice::Discard);
        assert_eq!(app.pending_action, None);
        assert_eq!(app.document.elements.len(), 1);
        assert_eq!(app.document.elements[0].kind, ElementKind::Ellipse);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn excalidraw_export_writes_a_readable_scene() {
        let dir = file_test_dir("excalidraw-export");
        let path = dir.join("scene.excalidraw");
        let mut app = selection_app();
        let expected = app.document.elements.len();

        assert!(app.export_excalidraw_to_path(&path));
        assert!(path.exists());
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read export"))
                .expect("parse export");
        assert_eq!(value["type"], "excalidraw");
        assert_eq!(
            value["elements"].as_array().expect("elements").len(),
            expected
        );
        assert!(matches!(
            app.file_feedback,
            Some(FileFeedback::Saved(ref message)) if message.contains("Exported")
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn png_export_writes_a_decodable_image() {
        let dir = file_test_dir("png-export");
        let path = dir.join("scene.png");
        let mut app = selection_app();

        assert!(app.export_png_to_path(&path));
        let bytes = std::fs::read(&path).expect("read PNG export");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = tiny_skia::Pixmap::decode_png(&bytes).expect("decode PNG export");
        assert!(decoded.width() > 0 && decoded.height() > 0);
        assert!(matches!(
            app.file_feedback,
            Some(FileFeedback::Saved(ref message)) if message.contains("Exported")
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_shortcuts_are_registered() {
        for (key, modifiers, action) in [
            (egui::Key::N, egui::Modifiers::COMMAND, Action::New),
            (egui::Key::O, egui::Modifiers::COMMAND, Action::Open),
            (egui::Key::S, egui::Modifiers::COMMAND, Action::Save),
            (
                egui::Key::S,
                egui::Modifiers {
                    shift: true,
                    command: true,
                    ..egui::Modifiers::NONE
                },
                Action::SaveAs,
            ),
        ] {
            assert!(
                KEYBINDINGS.iter().any(|binding| binding.key == key
                    && binding.modifiers == modifiers
                    && binding.action == action),
                "missing file shortcut for {key:?}"
            );
        }
    }

    fn selection_app() -> EpsilonApp {
        let mut app = EpsilonApp::default();
        app.commit_shape(
            ElementKind::Rectangle,
            Point::new(10.0, 20.0),
            Point::new(60.0, 80.0),
        );
        app.commit_shape(
            ElementKind::Ellipse,
            Point::new(100.0, 20.0),
            Point::new(150.0, 80.0),
        );
        app.selected = app.document.elements.iter().map(|e| e.id).collect();
        app
    }

    #[test]
    fn copy_writes_selection_json_to_the_system_clipboard() {
        let mut app = selection_app();
        let commands = run_clipboard_event(&mut app, egui::Event::Copy);
        let json = copied_text(&commands).expect("copy must write clipboard text");
        let payload = ClipboardPayload::from_json(&json).expect("clipboard JSON parses");

        assert!(payload.is_supported());
        assert_eq!(payload.elements.len(), 2);
        assert_eq!(app.clipboard_feedback, Some(ClipboardFeedback::Copied(2)));
        assert_eq!(app.clipboard.as_ref().map(|p| p.elements.len()), Some(2));
    }

    #[test]
    fn cut_removes_the_selection_and_undo_restores_it_exactly() {
        let mut app = selection_app();
        let original = app.document.clone();

        let commands = run_clipboard_event(&mut app, egui::Event::Cut);
        let json = copied_text(&commands).expect("cut must also copy");
        assert!(ClipboardPayload::from_json(&json).is_ok());
        assert!(app.document.elements.is_empty());
        assert!(app.selected.is_empty());

        assert_eq!(app.history.undo_len(), 3, "cut adds one history entry");
        assert!(app.undo());
        assert_eq!(app.document, original);
        assert_eq!(app.history.undo_len(), 2);
    }

    #[test]
    fn paste_regenerates_ids_offsets_geometry_and_selects_the_copies() {
        let mut app = selection_app();
        let original_ids: HashSet<ElementId> = app
            .document
            .elements
            .iter()
            .map(|element| element.id)
            .collect();
        let original_positions: Vec<(f64, f64)> = app
            .document
            .elements
            .iter()
            .map(|element| (element.x, element.y))
            .collect();

        let commands = run_clipboard_event(&mut app, egui::Event::Copy);
        let json = copied_text(&commands).expect("copy must write clipboard text");
        run_clipboard_event(&mut app, egui::Event::Paste(json));

        assert_eq!(app.document.elements.len(), 4);
        let pasted = &app.document.elements[2..];
        let pasted_ids: HashSet<ElementId> = pasted.iter().map(|element| element.id).collect();
        assert_eq!(pasted_ids.len(), 2);
        assert!(pasted_ids.is_disjoint(&original_ids));
        assert_eq!(app.selected, pasted_ids);
        for (pasted, (x, y)) in pasted.iter().zip(&original_positions) {
            assert_eq!(pasted.x, x + PASTE_OFFSET);
            assert_eq!(pasted.y, y + PASTE_OFFSET);
        }
        assert_eq!(app.clipboard_feedback, Some(ClipboardFeedback::Pasted(2)));

        // The paste is a single undoable transaction.
        assert_eq!(app.history.undo_len(), 3);
        assert!(app.undo());
        assert_eq!(app.document.elements.len(), 2);
    }

    #[test]
    fn repeated_pastes_cascade_and_menu_paste_uses_the_stored_payload() {
        let mut app = selection_app();
        app.selected = app.document.elements[..1].iter().map(|e| e.id).collect();
        let commands = run_clipboard_event(&mut app, egui::Event::Copy);
        let json = copied_text(&commands).expect("copy must write clipboard text");
        let source_x = app.document.elements[0].x;

        run_clipboard_event(&mut app, egui::Event::Paste(json.clone()));
        assert_eq!(app.document.elements[2].x, source_x + PASTE_OFFSET);

        app.selected.clear();
        assert!(app.paste_from_menu());
        assert_eq!(app.document.elements[3].x, source_x + PASTE_OFFSET * 2.0);
    }

    #[test]
    fn unrelated_clipboard_text_is_ignored() {
        let mut app = selection_app();
        let before = app.document.clone();

        run_clipboard_event(&mut app, egui::Event::Paste("plain text".to_owned()));
        assert_eq!(app.document, before);
        assert_eq!(app.clipboard_feedback, Some(ClipboardFeedback::Unsupported));
        assert!(!app.can_paste());
    }

    #[test]
    fn clipboard_events_are_ignored_while_canvas_text_is_edited() {
        let mut app = selection_app();
        let id = app.document.elements[0].id;
        app.editing_text = Some(id);
        let before = app.document.clone();

        let commands = run_clipboard_event(&mut app, egui::Event::Copy);
        assert!(copied_text(&commands).is_none());
        run_clipboard_event(&mut app, egui::Event::Cut);
        assert_eq!(app.document, before);
        assert!(app.clipboard_feedback.is_none());
    }

    #[test]
    fn delete_key_removes_the_selection_clears_it_and_is_undoable() {
        let mut app = selection_app();
        let original = app.document.clone();

        run_shortcut(&mut app, egui::Key::Delete, egui::Modifiers::NONE);
        assert!(app.document.elements.is_empty());
        assert!(app.selected.is_empty(), "no stale selection after delete");

        assert!(app.undo());
        assert_eq!(app.document, original);
    }

    #[test]
    fn backspace_deletes_the_selection_too() {
        let mut app = selection_app();
        run_shortcut(&mut app, egui::Key::Backspace, egui::Modifiers::NONE);
        assert!(app.document.elements.is_empty());
        assert!(app.selected.is_empty());
    }

    #[test]
    fn delete_is_ignored_while_canvas_text_is_edited() {
        let mut app = selection_app();
        let id = app.document.elements[0].id;
        app.editing_text = Some(id);
        let before = app.document.clone();

        run_shortcut(&mut app, egui::Key::Delete, egui::Modifiers::NONE);
        run_shortcut(&mut app, egui::Key::Backspace, egui::Modifiers::NONE);
        assert_eq!(app.document, before);
    }

    #[test]
    fn select_all_selects_every_element() {
        let mut app = selection_app();
        app.selected.clear();

        run_shortcut(&mut app, egui::Key::A, egui::Modifiers::COMMAND);
        assert_eq!(app.selected.len(), 2);
        for element in &app.document.elements {
            assert!(app.selected.contains(&element.id));
        }

        let mut empty = EpsilonApp::default();
        run_shortcut(&mut empty, egui::Key::A, egui::Modifiers::COMMAND);
        assert!(empty.selected.is_empty());
    }

    #[test]
    fn align_selection_commits_one_transaction_and_undoes() {
        let mut app = selection_app();
        let original = app.document.clone();
        let history_before = app.history.undo_len();

        // Both elements already share a top edge, so this is a no-op.
        assert!(!app.align_selection(ElementAlign::Top));
        assert_eq!(app.history.undo_len(), history_before);

        assert!(app.align_selection(ElementAlign::Left));
        assert_eq!(app.history.undo_len(), history_before + 1);
        assert_eq!(app.selected.len(), 2, "alignment preserves the selection");
        for element in &app.document.elements {
            assert!((element.world_bounds().min.x - 10.0).abs() < 1e-9);
            assert!((element.y - 20.0).abs() < 1e-9, "only x should move");
        }

        assert!(app.undo());
        assert_eq!(app.document, original);
        assert_eq!(app.selected.len(), 2);

        app.selected = HashSet::from([app.document.elements[0].id]);
        assert!(!app.align_selection(ElementAlign::Right));
        assert_eq!(app.history.undo_len(), history_before);
    }

    fn distribution_app() -> EpsilonApp {
        let mut app = EpsilonApp::default();
        for (start, end) in [
            (Point::new(0.0, 0.0), Point::new(40.0, 20.0)),
            (Point::new(10.0, 50.0), Point::new(50.0, 70.0)),
            (Point::new(100.0, 80.0), Point::new(140.0, 100.0)),
        ] {
            app.commit_shape(ElementKind::Rectangle, start, end);
        }
        app.selected = app.document.elements.iter().map(|e| e.id).collect();
        app
    }

    #[test]
    fn distribute_selection_commits_one_transaction_and_undoes() {
        let mut app = distribution_app();
        let original = app.document.clone();
        let history_before = app.history.undo_len();

        assert!(app.distribute_selection(ElementDistribute::Horizontal));
        assert_eq!(app.history.undo_len(), history_before + 1);
        assert_eq!(
            app.selected.len(),
            3,
            "distribution preserves the selection"
        );
        let xs: Vec<f64> = app
            .document
            .elements
            .iter()
            .map(|element| element.world_bounds().min.x)
            .collect();
        assert_eq!(xs, vec![0.0, 50.0, 100.0]);

        assert!(
            !app.distribute_selection(ElementDistribute::Horizontal),
            "already-even gaps must not add history"
        );
        assert_eq!(app.history.undo_len(), history_before + 1);

        assert!(app.undo());
        assert_eq!(app.document, original);

        let removed = app.document.elements[2].id;
        app.selected.remove(&removed);
        assert!(!app.distribute_selection(ElementDistribute::Horizontal));
    }

    #[test]
    fn group_shortcut_wraps_the_selection_and_undoes() {
        let mut app = selection_app();
        let original = app.document.clone();

        run_shortcut(&mut app, egui::Key::G, egui::Modifiers::COMMAND);

        assert_eq!(app.history.undo_len(), 3, "grouping is one undo entry");
        assert_eq!(app.selected.len(), 2, "selection is preserved");
        let first = &app.document.elements[0];
        let second = &app.document.elements[1];
        assert_eq!(first.group_ids.len(), 1);
        assert_eq!(first.group_ids, second.group_ids);
        assert_eq!(
            app.document.expand_selection(&[first.id]),
            vec![first.id, second.id],
            "one member expands to the whole group"
        );

        assert!(app.undo());
        assert_eq!(app.document, original);
        assert!(app.redo());
        assert_eq!(app.document.elements[0].group_ids.len(), 1);
    }

    #[test]
    fn ungroup_shortcut_peels_one_level_and_preserves_element_order() {
        let mut app = selection_app();
        let ids: Vec<ElementId> = app.document.elements.iter().map(|e| e.id).collect();
        app.selected = ids.iter().copied().collect();
        assert!(app.group_selection());
        let grouped = app.document.clone();

        run_shortcut(&mut app, egui::Key::G, shift_command());

        assert!(
            app.document
                .elements
                .iter()
                .all(|element| element.group_ids.is_empty())
        );
        assert_eq!(
            app.document
                .elements
                .iter()
                .map(|element| element.id)
                .collect::<Vec<_>>(),
            ids,
            "ungrouping must not reorder elements"
        );
        assert_eq!(app.selected, ids.into_iter().collect());

        assert!(app.undo());
        assert_eq!(app.document, grouped);
    }

    #[test]
    fn grouped_click_and_shift_click_select_the_whole_group() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let mut app = EpsilonApp::default();
        let mut first_element = Element::new(ElementKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
        first_element.style.fill = DocumentColor::WHITE;
        let mut second_element = Element::new(ElementKind::Rectangle, 200.0, 0.0, 100.0, 100.0);
        second_element.style.fill = DocumentColor::WHITE;
        let first = app.document.add(first_element);
        let second = app.document.add(second_element);
        app.selected = HashSet::from([first, second]);
        assert!(app.group_selection());
        app.selected.clear();

        run_app_frame(&mut app, &ctx, screen, Vec::new());
        let point = app.canvas_rect.min + egui::vec2(50.0, 50.0);
        run_app_frame(
            &mut app,
            &ctx,
            screen,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(app.selected, HashSet::from([first, second]));
        run_app_frame(
            &mut app,
            &ctx,
            screen,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );

        // Shift-clicking any member removes the whole group.
        app.toggle_selected(first, true);
        assert!(app.selected.is_empty());
        app.toggle_selected(first, true);
        assert_eq!(app.selected, HashSet::from([first, second]));

        // Right-clicking a member targets the whole group as well.
        app.selected.clear();
        run_app_frame(
            &mut app,
            &ctx,
            screen,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(app.selected, HashSet::from([first, second]));
    }

    #[test]
    fn duplicate_offsets_by_one_grid_step_and_selects_the_copies() {
        let mut app = selection_app();
        app.selected = app.document.elements[..1].iter().map(|e| e.id).collect();
        let original = app.document.clone();
        let source = &app.document.elements[0];
        let (source_x, source_y) = (source.x, source.y);

        run_shortcut(&mut app, egui::Key::D, egui::Modifiers::COMMAND);

        assert_eq!(app.document.elements.len(), 3);
        assert_eq!(app.history.undo_len(), 3, "duplicate is one undo entry");
        let copy = &app.document.elements[2];
        assert_ne!(copy.id, app.document.elements[0].id);
        assert_eq!(copy.x, source_x + grid::DEFAULT_GRID_SIZE);
        assert_eq!(copy.y, source_y + grid::DEFAULT_GRID_SIZE);
        assert_eq!(app.selected, HashSet::from([copy.id]));
        assert!(
            app.clipboard.is_none(),
            "duplicate must not touch the clipboard"
        );
        assert_eq!(app.paste_count, 0);

        assert!(app.undo());
        assert_eq!(app.document, original);
    }

    #[test]
    fn escape_clears_the_selection_and_returns_to_the_select_tool() {
        let mut app = selection_app();
        app.tool = Tool::Rectangle;
        let before = app.document.clone();

        run_shortcut(&mut app, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(app.selected.is_empty());
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.document, before);
    }

    #[test]
    fn escape_closes_help_before_clearing_the_selection() {
        let mut app = selection_app();
        app.help_open = true;
        app.tool = Tool::Hand;

        run_shortcut(&mut app, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(!app.help_open);
        assert_eq!(app.selected.len(), 2);
        assert_eq!(app.tool, Tool::Hand);
    }

    fn stacked_app() -> EpsilonApp {
        let mut app = EpsilonApp::default();
        for y in [0.0, 40.0, 80.0] {
            app.commit_shape(
                ElementKind::Rectangle,
                Point::new(0.0, y),
                Point::new(30.0, y + 20.0),
            );
        }
        app
    }

    fn paint_ids(app: &EpsilonApp) -> Vec<ElementId> {
        app.document
            .paint_order()
            .into_iter()
            .map(|element| element.id)
            .collect()
    }

    fn shift_command() -> egui::Modifiers {
        egui::Modifiers {
            shift: true,
            command: true,
            ..egui::Modifiers::NONE
        }
    }

    #[test]
    fn bring_to_front_and_send_to_back_preserve_the_selection_and_undo() {
        let mut app = stacked_app();
        let ids = paint_ids(&app);
        let original = app.document.clone();
        app.selected = HashSet::from([ids[0]]);

        run_shortcut(&mut app, egui::Key::CloseBracket, shift_command());
        assert_eq!(paint_ids(&app), vec![ids[1], ids[2], ids[0]]);
        assert_eq!(app.selected, HashSet::from([ids[0]]));
        assert_eq!(app.history.undo_len(), 4);

        assert!(app.undo());
        assert_eq!(app.document, original);
        assert!(app.redo());
        assert_eq!(paint_ids(&app), vec![ids[1], ids[2], ids[0]]);

        app.selected = HashSet::from([ids[2]]);
        run_shortcut(&mut app, egui::Key::OpenBracket, shift_command());
        assert_eq!(paint_ids(&app), vec![ids[2], ids[1], ids[0]]);
        assert_eq!(app.selected, HashSet::from([ids[2]]));

        assert!(app.undo());
        assert_eq!(paint_ids(&app), vec![ids[1], ids[2], ids[0]]);
    }

    #[test]
    fn bring_forward_and_send_backward_move_one_slot() {
        let mut app = stacked_app();
        let ids = paint_ids(&app);
        app.selected = HashSet::from([ids[0]]);

        run_shortcut(&mut app, egui::Key::CloseBracket, egui::Modifiers::COMMAND);
        assert_eq!(paint_ids(&app), vec![ids[1], ids[0], ids[2]]);
        assert_eq!(app.selected, HashSet::from([ids[0]]));

        // An element already at the front cannot move further.
        run_shortcut(&mut app, egui::Key::CloseBracket, egui::Modifiers::COMMAND);
        assert_eq!(paint_ids(&app), vec![ids[1], ids[2], ids[0]]);
        let depth = app.history.undo_len();
        run_shortcut(&mut app, egui::Key::CloseBracket, egui::Modifiers::COMMAND);
        assert_eq!(
            app.history.undo_len(),
            depth,
            "no-op must not record history"
        );

        run_shortcut(&mut app, egui::Key::OpenBracket, egui::Modifiers::COMMAND);
        assert_eq!(paint_ids(&app), vec![ids[1], ids[0], ids[2]]);
    }

    #[test]
    fn z_order_shortcuts_are_registered() {
        for (key, modifiers, action) in [
            (
                egui::Key::CloseBracket,
                shift_command(),
                Action::ZOrder(ZOrderMove::Front),
            ),
            (
                egui::Key::CloseBracket,
                egui::Modifiers::COMMAND,
                Action::ZOrder(ZOrderMove::Forward),
            ),
            (
                egui::Key::OpenBracket,
                egui::Modifiers::COMMAND,
                Action::ZOrder(ZOrderMove::Backward),
            ),
            (
                egui::Key::OpenBracket,
                shift_command(),
                Action::ZOrder(ZOrderMove::Back),
            ),
        ] {
            assert!(
                KEYBINDINGS.iter().any(|binding| binding.key == key
                    && binding.modifiers == modifiers
                    && binding.action == action),
                "{action:?} has no shortcut"
            );
        }
    }

    #[test]
    fn edit_shortcuts_are_registered() {
        for (key, modifiers, action) in [
            (egui::Key::Delete, egui::Modifiers::NONE, Action::Delete),
            (egui::Key::Backspace, egui::Modifiers::NONE, Action::Delete),
            (egui::Key::D, egui::Modifiers::COMMAND, Action::Duplicate),
            (egui::Key::A, egui::Modifiers::COMMAND, Action::SelectAll),
            (
                egui::Key::Escape,
                egui::Modifiers::NONE,
                Action::ClearSelection,
            ),
            (egui::Key::G, egui::Modifiers::COMMAND, Action::Group),
            (egui::Key::G, shift_command(), Action::Ungroup),
        ] {
            assert!(
                KEYBINDINGS.iter().any(|binding| binding.key == key
                    && binding.modifiers == modifiers
                    && binding.action == action),
                "{action:?} has no shortcut"
            );
        }
    }

    #[test]
    fn context_actions_match_the_target_and_always_have_shortcuts() {
        let mut app = EpsilonApp::default();
        assert!(
            app.context_actions().is_empty(),
            "empty canvas has no applicable actions"
        );

        app.commit_shape(
            ElementKind::Rectangle,
            Point::new(0.0, 0.0),
            Point::new(30.0, 20.0),
        );
        assert_eq!(app.context_actions(), vec![ContextAction::SelectAll]);

        let id = app.document.elements[0].id;
        app.selected.insert(id);
        let actions = app.context_actions();
        for expected in [
            ContextAction::Cut,
            ContextAction::Copy,
            ContextAction::Duplicate,
            ContextAction::Delete,
            ContextAction::BringToFront,
            ContextAction::SendToBack,
        ] {
            assert!(actions.contains(&expected), "missing {expected:?}");
        }
        assert!(!actions.contains(&ContextAction::SelectAll));
        assert!(
            !actions.contains(&ContextAction::Paste),
            "clipboard is empty"
        );

        let commands = run_clipboard_event(&mut app, egui::Event::Copy);
        assert!(copied_text(&commands).is_some());
        assert!(app.context_actions().contains(&ContextAction::Paste));

        app.editing_text = Some(id);
        assert!(
            app.context_actions().is_empty(),
            "no menu while editing text"
        );

        for action in ContextAction::ALL {
            assert!(!action.label().is_empty());
            assert!(
                shortcut_for(action.action()).is_some(),
                "{action:?} has no registered shortcut"
            );
        }
    }

    #[test]
    fn right_click_selects_the_target_or_clears_the_selection() {
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let mut app = stacked_app();
        let ids = paint_ids(&app);

        let right_click_at = |app: &mut EpsilonApp, world: Point| {
            let screen = app.viewport.world_to_screen(world);
            let pos = canvas.min + egui::vec2(screen.x as f32, screen.y as f32);
            let ctx = egui::Context::default();
            let output = ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| app.update_context_selection(ui.ctx(), canvas),
            );
            output.drop_without_applying_deltas();
        };

        right_click_at(&mut app, Point::new(0.0, 10.0));
        assert_eq!(app.selected, HashSet::from([ids[0]]));

        // Right-clicking inside an existing multi-selection keeps it.
        app.selected = HashSet::from([ids[0], ids[1]]);
        right_click_at(&mut app, Point::new(0.0, 10.0));
        assert_eq!(app.selected, HashSet::from([ids[0], ids[1]]));

        // Empty canvas space clears the selection so the canvas menu applies.
        right_click_at(&mut app, Point::new(600.0, 400.0));
        assert!(app.selected.is_empty());
    }

    #[test]
    fn clipboard_shortcuts_are_registered() {
        for (key, action) in [
            (egui::Key::C, Action::Copy),
            (egui::Key::X, Action::Cut),
            (egui::Key::V, Action::Paste),
        ] {
            assert!(
                KEYBINDINGS.iter().any(|binding| binding.key == key
                    && binding.modifiers == egui::Modifiers::COMMAND
                    && binding.action == action),
                "{action:?} has no Ctrl shortcut"
            );
        }
    }

    #[test]
    fn help_overlay_toggles_with_f1() {
        let mut app = EpsilonApp::default();
        assert!(!app.help_open);

        run_shortcut(&mut app, egui::Key::F1, egui::Modifiers::default());
        assert!(app.help_open);

        run_shortcut(&mut app, egui::Key::F1, egui::Modifiers::default());
        assert!(!app.help_open);
    }

    #[test]
    fn help_overlay_toggles_with_question_mark() {
        let mut app = EpsilonApp::default();
        let modifiers = egui::Modifiers {
            shift: true,
            ..Default::default()
        };

        run_shortcut(&mut app, egui::Key::Questionmark, modifiers);
        assert!(app.help_open);
    }

    #[test]
    fn help_overlay_closes_with_escape() {
        let mut app = EpsilonApp {
            help_open: true,
            ..Default::default()
        };

        run_shortcut(&mut app, egui::Key::Escape, egui::Modifiers::default());
        assert!(!app.help_open);
    }

    #[test]
    fn properties_panel_defaults_to_open() {
        assert!(EpsilonApp::default().properties_open);
    }

    #[test]
    fn collapsed_properties_panel_renders() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
        let ctx = egui::Context::default();
        theme::apply(&ctx, Theme::Dark);
        let mut app = EpsilonApp {
            properties_open: false,
            ..Default::default()
        };
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                let _ = app.show_shell(ui);
            },
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn shell_renders_at_150_percent_scale() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.5);
        theme::apply(&ctx, Theme::Dark);
        let mut app = EpsilonApp::default();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                let _ = app.show_shell(ui);
            },
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn shell_renders_in_light_theme() {
        assert_shell_fits(1920.0, 1080.0, Theme::Light);
        assert_shell_fits(
            layout::MIN_WINDOW_SIZE[0],
            layout::MIN_WINDOW_SIZE[1],
            Theme::Light,
        );
    }
}
