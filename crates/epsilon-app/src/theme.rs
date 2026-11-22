//! Central shell palette and theme application.
//!
//! Widgets never choose colors themselves: they read `egui::Visuals`, which is
//! set from the palette below whenever the theme changes. That keeps both
//! themes consistent and makes a theme switch take effect on every region in
//! the same frame.

use eframe::egui::{self, Color32, Stroke};

/// Application theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    fn palette(self) -> &'static Palette {
        match self {
            Self::Dark => &DARK,
            Self::Light => &LIGHT,
        }
    }

    /// Visuals for this theme, including the canvas color
    /// (`extreme_bg_color`).
    pub fn visuals(self) -> egui::Visuals {
        let palette = self.palette();
        let mut visuals = match self {
            Self::Dark => egui::Visuals::dark(),
            Self::Light => egui::Visuals::light(),
        };

        visuals.panel_fill = palette.panel;
        visuals.window_fill = palette.window;
        visuals.extreme_bg_color = palette.canvas;
        visuals.text_edit_bg_color = Some(palette.canvas);
        visuals.faint_bg_color = palette.faint;
        visuals.code_bg_color = palette.faint;
        visuals.window_stroke = Stroke::new(1.0, palette.border);
        visuals.weak_text_color = Some(palette.weak_text);
        visuals.hyperlink_color = palette.accent;
        style_widgets(&mut visuals, palette);
        visuals
    }
}

/// Switches the egui context to `theme` and installs Epsilon's palette.
///
/// Setting the preference (rather than leaving it on "follow system") keeps
/// the native system theme from overriding the explicit choice, and makes
/// both the light and dark style slots resolve to Epsilon's visuals.
pub fn apply(ctx: &egui::Context, theme: Theme) {
    ctx.set_theme(match theme {
        Theme::Dark => egui::ThemePreference::Dark,
        Theme::Light => egui::ThemePreference::Light,
    });
    ctx.set_visuals(theme.visuals());
}

/// All colors used by the shell. Both palettes provide the same semantic slots.
struct Palette {
    canvas: Color32,
    panel: Color32,
    window: Color32,
    border: Color32,
    faint: Color32,
    widget: Color32,
    widget_weak: Color32,
    hover: Color32,
    active: Color32,
    text: Color32,
    weak_text: Color32,
    accent: Color32,
}

const DARK: Palette = Palette {
    canvas: Color32::from_rgb(0x16, 0x18, 0x1B),
    panel: Color32::from_rgb(0x1D, 0x1F, 0x23),
    window: Color32::from_rgb(0x22, 0x25, 0x2A),
    border: Color32::from_rgb(0x34, 0x38, 0x3F),
    faint: Color32::from_rgb(0x26, 0x29, 0x2E),
    widget: Color32::from_rgb(0x2A, 0x2D, 0x33),
    widget_weak: Color32::from_rgb(0x24, 0x27, 0x2C),
    hover: Color32::from_rgb(0x34, 0x38, 0x41),
    active: Color32::from_rgb(0x3E, 0x43, 0x4D),
    text: Color32::from_rgb(0xE9, 0xEA, 0xEC),
    weak_text: Color32::from_rgb(0xA6, 0xAB, 0xB2),
    accent: Color32::from_rgb(0x8F, 0x87, 0xFF),
};

const LIGHT: Palette = Palette {
    canvas: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    panel: Color32::from_rgb(0xF3, 0xF4, 0xF6),
    window: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    border: Color32::from_rgb(0xD7, 0xDA, 0xE0),
    faint: Color32::from_rgb(0xEC, 0xEE, 0xF1),
    widget: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    widget_weak: Color32::from_rgb(0xE9, 0xEB, 0xEF),
    hover: Color32::from_rgb(0xE2, 0xE5, 0xEA),
    active: Color32::from_rgb(0xD5, 0xD9, 0xE0),
    text: Color32::from_rgb(0x1D, 0x20, 0x26),
    weak_text: Color32::from_rgb(0x5B, 0x62, 0x6C),
    accent: Color32::from_rgb(0x4C, 0x44, 0xC4),
};

/// Applies the shared widget colors, keeping egui's default corner radii and
/// stroke widths.
fn style_widgets(visuals: &mut egui::Visuals, palette: &Palette) {
    let noninteractive = &mut visuals.widgets.noninteractive;
    noninteractive.bg_fill = palette.panel;
    noninteractive.weak_bg_fill = palette.panel;
    noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    noninteractive.fg_stroke.color = palette.text;

    let states = [
        (
            &mut visuals.widgets.inactive,
            palette.widget,
            palette.widget_weak,
        ),
        (&mut visuals.widgets.hovered, palette.hover, palette.hover),
        (&mut visuals.widgets.active, palette.active, palette.active),
        (&mut visuals.widgets.open, palette.widget, palette.hover),
    ];
    for (state, bg, weak_bg) in states {
        state.bg_fill = bg;
        state.weak_bg_fill = weak_bg;
        state.bg_stroke.color = palette.border;
        state.fg_stroke.color = palette.text;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel_luminance(value: u8) -> f32 {
        let s = f32::from(value) / 255.0;
        if s <= 0.040_45 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }

    fn relative_luminance(color: Color32) -> f32 {
        0.2126 * channel_luminance(color.r())
            + 0.7152 * channel_luminance(color.g())
            + 0.0722 * channel_luminance(color.b())
    }

    fn contrast_ratio(a: Color32, b: Color32) -> f32 {
        let (la, lb) = (relative_luminance(a), relative_luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn text_contrast_meets_wcag_aa() {
        for theme in [Theme::Dark, Theme::Light] {
            let visuals = theme.visuals();
            let text = visuals.text_color();
            let weak = visuals.weak_text_color();
            for background in [visuals.panel_fill, visuals.extreme_bg_color] {
                assert!(
                    contrast_ratio(text, background) >= 7.0,
                    "{theme:?} primary text contrast is too low"
                );
                assert!(
                    contrast_ratio(weak, background) >= 4.5,
                    "{theme:?} weak text contrast is too low"
                );
            }
        }
    }

    #[test]
    fn canvas_is_distinct_from_chrome() {
        for theme in [Theme::Dark, Theme::Light] {
            let visuals = theme.visuals();
            // The canvas must read as a separate surface from the chrome.
            assert_ne!(visuals.panel_fill, visuals.extreme_bg_color);
            assert_ne!(visuals.panel_fill, visuals.window_fill);
        }
    }

    #[test]
    fn applying_theme_switches_context_style() {
        let ctx = egui::Context::default();

        apply(&ctx, Theme::Light);
        assert_eq!(ctx.theme(), egui::Theme::Light);
        let light = ctx.style_of(egui::Theme::Light);
        assert!(!light.visuals.dark_mode);
        assert_eq!(light.visuals.panel_fill, LIGHT.panel);

        apply(&ctx, Theme::Dark);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        let dark = ctx.style_of(egui::Theme::Dark);
        assert!(dark.visuals.dark_mode);
        assert_eq!(dark.visuals.panel_fill, DARK.panel);
    }
}
