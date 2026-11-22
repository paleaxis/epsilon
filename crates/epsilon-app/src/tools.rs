//! Drawing tool model, keyboard shortcuts and tool-rail buttons.
//!
//! Tools are UI state only at this stage: selecting one changes the active
//! tool but does not draw until the Stage 3 drawing engine lands.

use eframe::egui;

/// Buttons are drawn rather than loaded from assets so the binary stays
/// self-contained and icons follow the active theme's foreground color.
pub const BUTTON_SIZE: egui::Vec2 = egui::vec2(40.0, 34.0);
const ICON_SIZE: f32 = 20.0;
/// Icons are designed on a 20×20 grid and scaled to the button.
const ICON_UNITS: f32 = 20.0;
const ICON_STROKE_WIDTH: f32 = 1.5;

/// Available tools, in tool-rail order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Tool {
    #[default]
    Select,
    Hand,
    Rectangle,
    Ellipse,
    Diamond,
    Line,
    Arrow,
    Freehand,
    Text,
}

impl Tool {
    pub const ALL: [Self; 9] = [
        Self::Select,
        Self::Hand,
        Self::Rectangle,
        Self::Ellipse,
        Self::Diamond,
        Self::Line,
        Self::Arrow,
        Self::Freehand,
        Self::Text,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Hand => "Hand",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Diamond => "Diamond",
            Self::Line => "Line",
            Self::Arrow => "Arrow",
            Self::Freehand => "Freehand",
            Self::Text => "Text",
        }
    }

    /// Single-key shortcut letter, shown in tooltips.
    pub fn shortcut(self) -> char {
        match self {
            Self::Select => 'V',
            Self::Hand => 'H',
            Self::Rectangle => 'R',
            Self::Ellipse => 'O',
            Self::Diamond => 'D',
            Self::Line => 'L',
            Self::Arrow => 'A',
            Self::Freehand => 'P',
            Self::Text => 'T',
        }
    }

    pub fn tooltip(self) -> String {
        format!("{} ({})", self.name(), self.shortcut())
    }
}

/// A tool-rail button with a vector icon, selection state, tooltip and
/// accessible name.
pub fn tool_button(ui: &mut egui::Ui, tool: Tool, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(BUTTON_SIZE, egui::Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);

    let fill = if selected || response.has_focus() {
        visuals.bg_fill
    } else if response.hovered() {
        visuals.weak_bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    if fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, visuals.corner_radius, fill);
    }

    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(ICON_SIZE));
    let icon_stroke = egui::Stroke::new(ICON_STROKE_WIDTH, visuals.fg_stroke.color);
    paint_icon(ui.painter(), icon_rect, tool, icon_stroke);

    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            visuals.corner_radius,
            egui::Stroke::new(1.0, ui.visuals().hyperlink_color),
            egui::StrokeKind::Inside,
        );
    }

    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, tool.name())
    });
    response
        .on_hover_text(tool.tooltip())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn paint_icon(painter: &egui::Painter, rect: egui::Rect, tool: Tool, stroke: egui::Stroke) {
    let scale = rect.width() / ICON_UNITS;
    let at = |x: f32, y: f32| egui::pos2(rect.min.x + x * scale, rect.min.y + y * scale);
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([at(a.0, a.1), at(b.0, b.1)], stroke);
    };
    let polyline = |points: &[(f32, f32)], closed: bool| {
        let points: Vec<egui::Pos2> = points.iter().map(|&(x, y)| at(x, y)).collect();
        painter.add(if closed {
            egui::Shape::closed_line(points, stroke)
        } else {
            egui::Shape::line(points, stroke)
        });
    };

    match tool {
        Tool::Select => polyline(
            &[
                (4.5, 2.5),
                (4.5, 16.0),
                (8.2, 12.2),
                (10.6, 17.5),
                (12.9, 16.6),
                (10.5, 11.4),
                (15.5, 11.0),
            ],
            true,
        ),
        Tool::Hand => {
            line((10.0, 4.0), (10.0, 16.0));
            line((4.0, 10.0), (16.0, 10.0));
            for (a, b, c) in [
                ((7.5, 6.5), (10.0, 4.0), (12.5, 6.5)),
                ((7.5, 13.5), (10.0, 16.0), (12.5, 13.5)),
                ((6.5, 7.5), (4.0, 10.0), (6.5, 12.5)),
                ((13.5, 7.5), (16.0, 10.0), (13.5, 12.5)),
            ] {
                line(a, b);
                line(b, c);
            }
        }
        Tool::Rectangle => {
            painter.rect_stroke(
                egui::Rect::from_min_max(at(4.0, 6.0), at(16.0, 14.0)),
                1.5,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Tool::Ellipse => {
            painter.circle_stroke(at(10.0, 10.0), 6.0 * scale, stroke);
        }
        Tool::Diamond => polyline(
            &[(10.0, 3.5), (16.5, 10.0), (10.0, 16.5), (3.5, 10.0)],
            true,
        ),
        Tool::Line => line((4.5, 15.5), (15.5, 4.5)),
        Tool::Arrow => {
            line((4.5, 15.5), (15.5, 4.5));
            line((10.5, 4.5), (15.5, 4.5));
            line((15.5, 4.5), (15.5, 9.5));
        }
        Tool::Freehand => polyline(
            &[
                (3.5, 13.0),
                (6.0, 7.5),
                (9.0, 14.0),
                (12.0, 6.5),
                (16.5, 11.0),
            ],
            false,
        ),
        Tool::Text => {
            line((4.5, 5.0), (15.5, 5.0));
            line((10.0, 5.0), (10.0, 16.0));
            line((7.0, 16.0), (13.0, 16.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_unique_shortcut() {
        let mut shortcuts = std::collections::HashSet::new();
        for tool in Tool::ALL {
            assert!(
                shortcuts.insert(tool.shortcut()),
                "{} reuses shortcut {}",
                tool.name(),
                tool.shortcut()
            );
        }
        assert_eq!(shortcuts.len(), Tool::ALL.len());
    }
}
