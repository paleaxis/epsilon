use std::collections::HashSet;

use eframe::egui::{self, Align, Layout, RichText};

use epsilon_core::document::{
    Color as DocumentColor, Document, Element, ElementId, ElementKind, FillStyle, StrokeStyle,
    TextAlign,
};
use epsilon_core::geometry::{
    Point, Rect, ResizeHandle, arrowhead_points, constrain_angle, resize_rect, rotate_point,
    simplify_polyline, smooth_polyline, snap_angle,
};
use epsilon_core::grid;
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
}

/// Minimum on-screen width/height (or line length) for a drag to be committed.
const MIN_SHAPE_PIXELS: f64 = 3.0;

/// Shift-constrained linear tools snap to 15-degree increments.
const LINE_ANGLE_STEP: f64 = std::f64::consts::PI / 12.0;

/// Arrowhead size in screen pixels, independent of zoom.
const ARROW_HEAD_LENGTH_PIXELS: f64 = 12.0;
const ARROW_HEAD_HALF_WIDTH_PIXELS: f64 = 6.0;

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

fn rotated_shape_screen_points(
    canvas_rect: egui::Rect,
    viewport: &Viewport,
    element: &Element,
    local_points: impl IntoIterator<Item = Point>,
) -> Vec<egui::Pos2> {
    local_points
        .into_iter()
        .map(|point| {
            world_to_screen_pos(canvas_rect, viewport, element_world_point(element, point))
        })
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
    /// Last pointer position in world coordinates, if the pointer is over the
    /// canvas. Drives the status-bar readout.
    pointer_world: Option<Point>,
    /// Whether the canvas grid is visible.
    grid_enabled: bool,
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
            pointer_world: None,
            canvas_rect: egui::Rect::NOTHING,
            grid_enabled: false,
            pointer_mode: PointerMode::Idle,
            hover_screen: None,
            freehand_points: Vec::new(),
            editing_text: None,
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
        let app = Self::from_storage(cc.storage);
        theme::apply(&cc.egui_ctx, app.theme);
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
                        if ui.button("Quit").clicked() {
                            request_quit(ui.ctx());
                        }
                    });
                    ui.menu_button("View", |ui| {
                        self.theme_toggle(ui);
                        let mut grid = self.grid_enabled;
                        if ui.checkbox(&mut grid, "Grid").changed() {
                            self.grid_enabled = grid;
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
    /// keyboard input (a text field, for example).
    fn handle_shortcuts(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }

        let events: Vec<egui::Event> = ctx.input(|input| input.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                continue;
            };

            // Help overlay toggles and closes are handled even while it is open.
            if key == egui::Key::F1 || key == egui::Key::Questionmark {
                let binding = KEYBINDINGS
                    .iter()
                    .find(|binding| binding.key == key && binding.modifiers == modifiers);
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

            if modifiers.ctrl || modifiers.alt || modifiers.command {
                continue;
            }

            if let Some(binding) = KEYBINDINGS
                .iter()
                .find(|binding| binding.key == key && binding.modifiers == modifiers)
            {
                let anchor = pointer_anchor(ctx, canvas_rect);
                match binding.action {
                    Action::SelectTool(tool) => self.tool = tool,
                    Action::ToggleHelp => self.help_open = !self.help_open,
                    Action::ZoomIn => self.viewport.zoom_by(1.25, anchor),
                    Action::ZoomOut => self.viewport.zoom_by(0.8, anchor),
                    Action::ZoomReset => self.viewport = Viewport::default(),
                    Action::ZoomFit => self.viewport = Viewport::default(),
                }
            }
        }
    }

    /// Style controls applied to every selected element.
    fn style_controls(ui: &mut egui::Ui, document: &mut Document, selected: &HashSet<ElementId>) {
        let Some(first) = document
            .elements
            .iter()
            .find(|element| selected.contains(&element.id))
        else {
            return;
        };
        let mut style = first.style;
        let mut changed = false;

        ui.horizontal(|ui| {
            let mut stroke = document_color(style.stroke);
            if ui.color_edit_button_srgba(&mut stroke).changed() {
                style.stroke = document_color_from_egui(stroke);
                changed = true;
            }
            ui.label("Stroke");
        });
        ui.horizontal(|ui| {
            let mut fill = document_color(style.fill);
            if ui.color_edit_button_srgba(&mut fill).changed() {
                style.fill = document_color_from_egui(fill);
                changed = true;
            }
            ui.label("Fill");
        });
        ui.horizontal(|ui| {
            ui.label("Width");
            if ui
                .add(
                    egui::DragValue::new(&mut style.stroke_width)
                        .range(0.1..=30.0)
                        .speed(0.1),
                )
                .changed()
            {
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Opacity");
            if ui
                .add(egui::Slider::new(&mut style.opacity, 0.0..=1.0).show_value(false))
                .changed()
            {
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Stroke");
            egui::ComboBox::from_id_salt("stroke_style")
                .selected_text(format!("{:?}", style.stroke_style))
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
                .selected_text(format!("{:?}", style.fill_style))
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
            ui.label("Roundness");
            if ui
                .add(
                    egui::DragValue::new(&mut style.roundness)
                        .range(0.0..=64.0)
                        .speed(0.5),
                )
                .changed()
            {
                changed = true;
            }
        });

        if changed {
            Self::apply_style_to_selection(document, selected, style);
        }
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
        egui::Panel::right("properties_panel")
            .resizable(true)
            .default_size(layout::PROPERTIES_PANEL_WIDTH)
            .size_range(layout::PROPERTIES_PANEL_MIN_WIDTH..=layout::PROPERTIES_PANEL_MAX_WIDTH)
            .show_collapsible(ui, &mut self.properties_open, |ui| {
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
                            Self::style_controls(ui, &mut self.document, &self.selected);
                        }
                    });
                ui.add_space(4.0);
                egui::CollapsingHeader::new("Canvas")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.checkbox(&mut grid, "Show grid");
                    });
            });
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
                    self.pointer_mode = PointerMode::Idle;
                    self.transform_original.clear();
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
                    self.pointer_mode = PointerMode::Idle;
                    self.transform_original.clear();
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
                    self.pointer_mode = PointerMode::Idle;
                    self.transform_original.clear();
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
                    self.selected.clear();
                    self.selected.insert(id);
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
        if additive {
            if !self.selected.remove(&id) {
                self.selected.insert(id);
            }
        } else {
            self.selected.clear();
            self.selected.insert(id);
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
        self.document.add(element);
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
        let text = std::mem::take(&mut self.text_buffer);
        self.text_editor_needs_focus = false;

        if text.trim().is_empty() {
            self.document.remove(id);
            return;
        }

        let zoom = self.viewport.zoom;
        if let Some(element) = self
            .document
            .elements
            .iter_mut()
            .find(|element| element.id == id)
        {
            element.height =
                measured_text_height(ctx, &text, element.font_size, element.width, zoom);
            element.text = text;
            element.version = element.version.saturating_add(1);
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
            self.document.add(element);
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
                    let points =
                        rotated_shape_screen_points(canvas_rect, &self.viewport, element, corners);
                    let corner_radius = roundness.min(rect.width().min(rect.height()) * 0.5);
                    if element.angle.abs() <= 1e-9 && fill_style == FillStyle::Solid {
                        if fill.a() > 0 {
                            painter.rect_filled(rect, corner_radius, fill);
                        }
                    } else if fill.a() > 0 {
                        paint_fill_path(painter, &points, fill_style, fill);
                    }
                    if element.angle.abs() <= 1e-9 && stroke_style == StrokeStyle::Solid {
                        painter.rect_stroke(rect, corner_radius, stroke, egui::StrokeKind::Inside);
                    } else {
                        paint_stroke_path(painter, &points, true, stroke, stroke_style);
                    }
                }
                ElementKind::Ellipse => {
                    let bounds =
                        Rect::from_xywh(element.x, element.y, element.width, element.height);
                    let points = rotated_shape_screen_points(
                        canvas_rect,
                        &self.viewport,
                        element,
                        ellipse_local_points(bounds),
                    );
                    if element.angle.abs() <= 1e-9 && fill_style == FillStyle::Solid {
                        let center = rect.center();
                        let radius = rect.size() / 2.0;
                        if fill.a() > 0 {
                            painter.add(egui::Shape::ellipse_filled(center, radius, fill));
                        }
                    } else if fill.a() > 0 {
                        paint_fill_path(painter, &points, fill_style, fill);
                    }
                    if element.angle.abs() <= 1e-9 && stroke_style == StrokeStyle::Solid {
                        let center = rect.center();
                        let radius = rect.size() / 2.0;
                        painter.add(egui::Shape::ellipse_stroke(center, radius, stroke));
                    } else {
                        paint_stroke_path(painter, &points, true, stroke, stroke_style);
                    }
                }
                ElementKind::Diamond => {
                    let bounds =
                        Rect::from_xywh(element.x, element.y, element.width, element.height);
                    let points = rotated_shape_screen_points(
                        canvas_rect,
                        &self.viewport,
                        element,
                        bounds.diamond_points(),
                    );
                    if fill.a() > 0 {
                        paint_fill_path(painter, &points, fill_style, fill);
                    }
                    paint_stroke_path(painter, &points, true, stroke, stroke_style);
                }
                ElementKind::Line => {
                    if let [start, end] = element.points.as_slice() {
                        let points = vec![
                            world_to_screen_pos(
                                canvas_rect,
                                &self.viewport,
                                element_world_point(element, *start),
                            ),
                            world_to_screen_pos(
                                canvas_rect,
                                &self.viewport,
                                element_world_point(element, *end),
                            ),
                        ];
                        paint_stroke_path(painter, &points, false, stroke, stroke_style);
                    }
                }
                ElementKind::Arrow => {
                    if let [start, end] = element.points.as_slice() {
                        let start = element_world_point(element, *start);
                        let end = element_world_point(element, *end);
                        let points = vec![
                            world_to_screen_pos(canvas_rect, &self.viewport, start),
                            world_to_screen_pos(canvas_rect, &self.viewport, end),
                        ];
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
                    let points: Vec<egui::Pos2> = element
                        .points
                        .iter()
                        .map(|point| {
                            world_to_screen_pos(
                                canvas_rect,
                                &self.viewport,
                                element_world_point(element, *point),
                            )
                        })
                        .collect();
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
        let canvas_rect = self.show_shell(ui);
        self.handle_shortcuts(ui.ctx(), canvas_rect);
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
            ..Default::default()
        };
        let mut storage = TestStorage::default();
        eframe::App::save(&mut app, &mut storage);

        let restored = EpsilonApp::from_storage(Some(&storage));
        assert_eq!(restored.theme, Theme::Light);
        assert_eq!(restored.tool, Tool::Arrow);
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
