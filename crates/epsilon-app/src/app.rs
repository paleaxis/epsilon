use eframe::egui::{self, Align, Layout, RichText};

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
/// palette and the working menu/toolbar actions. Properties (1.5), live status
/// readouts (1.6) and the shortcut help overlay (1.7) are filled in later.
#[derive(Default)]
pub struct EpsilonApp {
    about_open: bool,
    theme: Theme,
    tool: Tool,
}

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
                ui.add_space(4.0);

                for tool in Tool::ALL {
                    let selected = self.tool == tool;
                    let clicked = ui
                        .vertical_centered(|ui| tools::tool_button(ui, tool, selected).clicked())
                        .inner;
                    if clicked {
                        self.tool = tool;
                    }
                    ui.add_space(2.0);
                }
            });
    }

    /// Applies single-key tool shortcuts, unless a widget currently owns
    /// keyboard input (a text field, for example).
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }

        let shortcut = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.ctrl && !modifiers.alt && !modifiers.command => {
                    Tool::from_key(*key)
                }
                _ => None,
            })
        });

        if let Some(tool) = shortcut {
            self.tool = tool;
        }
    }

    fn properties_panel(&self, ui: &mut egui::Ui) {
        // The resize range is fixed rather than derived from the current
        // window: transient small frames at startup would otherwise clamp and
        // overwrite the persisted panel width. The compile-time budget in
        // `layout.rs` guarantees the canvas minimum at the window minimum.
        egui::Panel::right("properties_panel")
            .resizable(true)
            .default_size(layout::PROPERTIES_PANEL_WIDTH)
            .size_range(layout::PROPERTIES_PANEL_MIN_WIDTH..=layout::PROPERTIES_PANEL_MAX_WIDTH)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("PROPERTIES").small().strong());
                ui.separator();
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No selection").strong());
                    ui.add_space(4.0);
                    ui.label(RichText::new("Select an element to edit its properties.").weak());
                });
            });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(layout::STATUS_BAR_HEIGHT)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("Ready").weak());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(concat!("Epsilon ", env!("CARGO_PKG_VERSION"))).weak(),
                        );
                    });
                });
            });
    }

    /// Draws the canvas region and returns its screen rect for input mapping.
    fn canvas(&self, ui: &mut egui::Ui) -> egui::Rect {
        let background = ui.visuals().extreme_bg_color;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(background))
            .show(ui, |ui| {
                ui.add_space((ui.available_height() * 0.3).max(0.0));
                ui.vertical_centered(|ui| {
                    ui.heading(RichText::new("Canvas").weak());
                    ui.add_space(6.0);
                    ui.label(RichText::new("The infinite canvas arrives in Stage 2.").weak());
                });
            })
            .response
            .rect
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

impl eframe::App for EpsilonApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ui.ctx());
        let _canvas_rect = self.show_shell(ui);
        self.about_window(ui.ctx());
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
        let output = ctx.run_ui(input, |ui| app.handle_shortcuts(ui.ctx()));
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
    fn shell_renders_in_light_theme() {
        assert_shell_fits(1920.0, 1080.0, Theme::Light);
        assert_shell_fits(
            layout::MIN_WINDOW_SIZE[0],
            layout::MIN_WINDOW_SIZE[1],
            Theme::Light,
        );
    }
}
