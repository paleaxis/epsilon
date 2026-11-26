//! One non-blocking surface for user-facing problems.
//!
//! Errors used to be appended to an already crowded status bar, where a long
//! message was clipped and vanished on the next action. Every failure now also
//! lands here as a toast over the canvas: errors and warnings stay long enough
//! to read and can be dismissed, while the message itself keeps its actionable
//! text. Toasts never take focus and never block drawing.

use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

/// How long each kind stays visible before disappearing on its own.
const WARNING_LIFETIME: Duration = Duration::from_secs(8);
const ERROR_LIFETIME: Duration = Duration::from_secs(12);
/// Upper bound on stacked toasts; the oldest is dropped first.
const MAX_NOTICES: usize = 4;
const TOAST_WIDTH: f32 = 340.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoticeKind {
    Warning,
    Error,
}

impl NoticeKind {
    fn lifetime(self) -> Duration {
        match self {
            Self::Warning => WARNING_LIFETIME,
            Self::Error => ERROR_LIFETIME,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Warning => "Warning",
            Self::Error => "Error",
        }
    }

    fn accent(self, visuals: &egui::Visuals) -> egui::Color32 {
        match self {
            Self::Warning => visuals.warn_fg_color,
            Self::Error => visuals.error_fg_color,
        }
    }
}

#[derive(Debug, Clone)]
struct Notice {
    id: u64,
    kind: NoticeKind,
    message: String,
    created: Instant,
}

/// Queue of toasts owned by the app and drawn at the end of each frame.
#[derive(Debug, Default)]
pub(crate) struct Notices {
    items: Vec<Notice>,
    next_id: u64,
}

impl Notices {
    pub(crate) fn error(&mut self, message: impl Into<String>) {
        self.push(NoticeKind::Error, message);
    }

    pub(crate) fn warning(&mut self, message: impl Into<String>) {
        self.push(NoticeKind::Warning, message);
    }

    fn push(&mut self, kind: NoticeKind, message: impl Into<String>) {
        let message = message.into();
        // Repeating a failure that is still on screen refreshes it instead of
        // stacking duplicate cards.
        if let Some(existing) = self
            .items
            .iter_mut()
            .find(|notice| notice.kind == kind && notice.message == message)
        {
            existing.created = Instant::now();
            return;
        }
        self.next_id += 1;
        self.items.push(Notice {
            id: self.next_id,
            kind,
            message,
            created: Instant::now(),
        });
        if self.items.len() > MAX_NOTICES {
            self.items.remove(0);
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    fn prune(&mut self, now: Instant) {
        self.items
            .retain(|notice| now.duration_since(notice.created) < notice.kind.lifetime());
    }

    /// Draws the stack top-right, below the toolbar. Called once per frame from
    /// the shell after the windows so toasts sit above everything else.
    pub(crate) fn show(&mut self, ctx: &egui::Context) {
        self.prune(Instant::now());
        if self.items.is_empty() {
            return;
        }

        let mut dismissed = None;
        egui::Area::new(egui::Id::unique("epsilon.notices"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 56.0))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(TOAST_WIDTH);
                for notice in &self.items {
                    let visuals = ui.visuals().clone();
                    let accent = notice.kind.accent(&visuals);
                    egui::Frame::NONE
                        .fill(visuals.panel_fill)
                        .stroke(egui::Stroke::new(1.0, accent))
                        .corner_radius(visuals.widgets.noninteractive.corner_radius)
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_max_width(TOAST_WIDTH - 16.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(notice.kind.label())
                                        .small()
                                        .strong()
                                        .color(accent),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("×").on_hover_text("Dismiss").clicked()
                                        {
                                            dismissed = Some(notice.id);
                                        }
                                    },
                                );
                            });
                            ui.label(RichText::new(&notice.message).small());
                        });
                    ui.add_space(6.0);
                }
            });

        if let Some(id) = dismissed {
            self.items.retain(|notice| notice.id != id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_messages_refresh_instead_of_stacking() {
        let mut notices = Notices::default();
        notices.error("Save failed: read-only file system");
        notices.error("Save failed: read-only file system");
        assert_eq!(notices.len(), 1);
    }

    #[test]
    fn the_oldest_toast_is_dropped_past_the_limit() {
        let mut notices = Notices::default();
        for index in 0..MAX_NOTICES + 2 {
            notices.error(format!("failure {index}"));
        }
        assert_eq!(notices.len(), MAX_NOTICES);
        assert_eq!(notices.items[0].message, "failure 2");
        assert_eq!(
            notices.items[MAX_NOTICES - 1].message,
            format!("failure {}", MAX_NOTICES + 1)
        );
    }

    #[test]
    fn toasts_expire_according_to_severity() {
        let mut notices = Notices::default();
        notices.error("bad");
        notices.warning("careful");

        let after_warning = Instant::now() + WARNING_LIFETIME + Duration::from_millis(1);
        notices.prune(after_warning);
        assert_eq!(notices.len(), 1);

        let after_all = Instant::now() + ERROR_LIFETIME + Duration::from_millis(1);
        notices.prune(after_all);
        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn toasts_render_in_a_headless_frame() {
        let ctx = egui::Context::default();
        let mut notices = Notices::default();
        notices.error("Save failed for /tmp/scene.epsilon: permission denied");

        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| notices.show(ui.ctx()),
        );
        output.drop_without_applying_deltas();

        assert_eq!(notices.len(), 1);
    }

    #[test]
    fn warnings_and_errors_keep_distinct_cards() {
        let mut notices = Notices::default();
        notices.warning("Clipboard has no Epsilon elements");
        notices.error("Clipboard has no Epsilon elements");
        assert_eq!(notices.len(), 2);
    }
}
