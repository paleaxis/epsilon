use eframe::egui::{self, Align, Layout, RichText};

use epsilon_core::document::{Color as DocumentColor, Document, Element, ElementKind};
use epsilon_core::geometry::{Point, Rect};
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

/// Minimum on-screen width/height for a shape drag to be committed.
const MIN_SHAPE_PIXELS: f64 = 3.0;

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
}

/// Element kind created by a drag with the given tool.
fn drawing_kind(tool: Tool) -> Option<ElementKind> {
    match tool {
        Tool::Rectangle => Some(ElementKind::Rectangle),
        Tool::Ellipse => Some(ElementKind::Ellipse),
        Tool::Diamond => Some(ElementKind::Diamond),
        _ => None,
    }
}

/// Projects an element's world bounds into screen space.
fn element_screen_rect(
    canvas_rect: egui::Rect,
    viewport: &Viewport,
    element: &Element,
) -> egui::Rect {
    let min = viewport.world_to_screen(Point::new(element.x, element.y));
    let max = viewport.world_to_screen(Point::new(
        element.x + element.width,
        element.y + element.height,
    ));
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

/// Projects an element's inscribed diamond into screen space.
fn diamond_screen_points(
    canvas_rect: egui::Rect,
    viewport: &Viewport,
    bounds: Rect,
) -> Vec<egui::Pos2> {
    bounds
        .diamond_points()
        .into_iter()
        .map(|point| {
            let screen = viewport.world_to_screen(point);
            egui::pos2(
                canvas_rect.min.x + screen.x as f32,
                canvas_rect.min.y + screen.y as f32,
            )
        })
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

fn document_color(color: DocumentColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
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
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("No selection").strong());
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("Select an element to edit its properties.").weak(),
                            );
                        });
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
            PointerMode::Dragging { .. } => {}
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
        let typing = ctx.egui_wants_keyboard_input();

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
                self.commit_shape(kind, start, current);
                self.pointer_mode = PointerMode::Idle;
            }
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

    /// Commits a shape if the drag is large enough on screen.
    fn commit_shape(&mut self, kind: ElementKind, start: Point, end: Point) {
        let rect = Rect::from_corners(start, end);
        let screen_width = rect.width() * self.viewport.zoom;
        let screen_height = rect.height() * self.viewport.zoom;
        if screen_width < MIN_SHAPE_PIXELS || screen_height < MIN_SHAPE_PIXELS {
            return;
        }

        let mut element = Element::new(kind, rect.min.x, rect.min.y, rect.width(), rect.height());
        element.style.stroke = match self.theme {
            Theme::Dark => DocumentColor::rgb(235, 235, 235),
            Theme::Light => DocumentColor::BLACK,
        };
        self.document.add(element);
    }

    /// Draws the document in paint order through the screen-space adapter.
    fn paint_document(&self, painter: &egui::Painter, canvas_rect: egui::Rect) {
        for element in self.document.paint_order() {
            let rect = element_screen_rect(canvas_rect, &self.viewport, element);
            let fill = document_color(element.style.fill);
            let stroke_width = (element.style.stroke_width * self.viewport.zoom).max(0.75) as f32;
            let stroke = egui::Stroke::new(stroke_width, document_color(element.style.stroke));

            match element.kind {
                ElementKind::Rectangle => {
                    if fill.a() > 0 {
                        painter.rect_filled(rect, 0.0, fill);
                    }
                    painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
                }
                ElementKind::Ellipse => {
                    let center = rect.center();
                    let radius = rect.size() / 2.0;
                    if fill.a() > 0 {
                        painter.add(egui::Shape::ellipse_filled(center, radius, fill));
                    }
                    painter.add(egui::Shape::ellipse_stroke(center, radius, stroke));
                }
                ElementKind::Diamond => {
                    let bounds =
                        Rect::from_xywh(element.x, element.y, element.width, element.height);
                    let points = diamond_screen_points(canvas_rect, &self.viewport, bounds);
                    if fill.a() > 0 {
                        painter.add(egui::Shape::convex_polygon(
                            points.clone(),
                            fill,
                            egui::Stroke::NONE,
                        ));
                    }
                    painter.add(egui::Shape::closed_line(points, stroke));
                }
                _ => {}
            }
        }
    }

    /// Draws the live shape preview while dragging.
    fn paint_draft(&self, painter: &egui::Painter, canvas_rect: egui::Rect, accent: egui::Color32) {
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
                _ => {}
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

                self.update_pan(ui.ctx(), rect);
                self.update_zoom(ui.ctx(), rect);
                self.update_drawing(ui.ctx(), rect);

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
                self.paint_draft(ui.painter(), rect, ui.visuals().hyperlink_color);

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
