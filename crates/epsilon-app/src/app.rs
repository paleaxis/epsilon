use eframe::egui::{self, Align, Layout, RichText};

use epsilon_core::geometry::Point;
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
    /// Last pointer position in world coordinates, if the pointer is over the
    /// canvas. Drives the status-bar readout.
    pointer_world: Option<Point>,
    /// Last canvas rect, used by toolbar zoom controls.
    canvas_rect: egui::Rect,
    /// Whether a pan gesture is currently active.
    panning: bool,
    /// Last pointer position (screen space) seen during a pan gesture.
    pan_last: Option<egui::Pos2>,
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
            pointer_world: None,
            canvas_rect: egui::Rect::NOTHING,
            panning: false,
            pan_last: None,
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
            });
        if hide_requested {
            self.properties_open = false;
        }
    }

    fn update_pan(&mut self, ctx: &egui::Context, canvas_rect: egui::Rect) {
        let pointer_pos = ctx.pointer_hover_pos();
        let in_canvas = pointer_pos.is_some_and(|pos| canvas_rect.contains(pos));
        let any_down = ctx.input(|input| input.pointer.any_down());
        let middle_pressed =
            ctx.input(|input| input.pointer.button_pressed(egui::PointerButton::Middle));
        let primary_pressed = ctx.input(|input| input.pointer.primary_pressed());
        let space_down = ctx.input(|input| input.key_down(egui::Key::Space));
        let typing = ctx.egui_wants_keyboard_input();

        if self.panning {
            if !any_down {
                self.panning = false;
                self.pan_last = None;
            } else if let Some(pos) = pointer_pos {
                if let Some(last) = self.pan_last {
                    self.viewport.offset.x += (pos.x - last.x) as f64;
                    self.viewport.offset.y += (pos.y - last.y) as f64;
                }
                self.pan_last = Some(pos);
            }
        } else {
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

            if start_pan {
                self.panning = true;
                self.pan_last = pointer_pos;
            }
        }

        let cursor = if self.panning {
            egui::CursorIcon::Grabbing
        } else if self.tool == Tool::Hand || (space_down && !typing) {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::Default
        };

        if in_canvas || self.panning {
            ctx.set_cursor_icon(cursor);
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
