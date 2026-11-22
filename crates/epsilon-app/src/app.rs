use eframe::egui;

/// Root application object.
///
/// Stage 0 only proves that the native shell links and launches. The real
/// layout (toolbar, tool panel, canvas, properties panel, status bar) is built
/// in Stage 1.
pub struct EpsilonApp;

impl EpsilonApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self
    }
}

impl eframe::App for EpsilonApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(32.0);
                ui.heading("Epsilon");
                ui.label(format!(
                    "v{} · native whiteboard foundation",
                    env!("CARGO_PKG_VERSION")
                ));
                ui.add_space(8.0);
                ui.label("The canvas interface arrives in Stage 1.");
            });
        });
    }
}
