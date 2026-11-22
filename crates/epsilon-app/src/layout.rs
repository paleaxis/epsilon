//! Shell layout constants.
//!
//! The window minimum size and the panel sizes are defined together so the
//! canvas always has usable room at the smallest supported window. The
//! invariants are checked at compile time below.

/// Minimum inner window size (logical points), enforced by the native window.
pub const MIN_WINDOW_SIZE: [f32; 2] = [720.0, 480.0];

/// Left tool rail (fixed width; tool buttons arrive in task 1.3).
pub const TOOL_RAIL_WIDTH: f32 = 64.0;

/// Right properties panel initial width.
pub const PROPERTIES_PANEL_WIDTH: f32 = 264.0;
/// Right properties panel minimum width.
pub const PROPERTIES_PANEL_MIN_WIDTH: f32 = 200.0;
/// Right properties panel maximum width.
pub const PROPERTIES_PANEL_MAX_WIDTH: f32 = 400.0;

/// Space the canvas must keep for itself at the minimum window size.
pub const CANVAS_MIN_WIDTH: f32 = 240.0;
/// Space the canvas must keep for itself vertically at the minimum window size.
pub const CANVAS_MIN_HEIGHT: f32 = 240.0;

pub const MENU_BAR_HEIGHT: f32 = 26.0;
pub const TOP_TOOLBAR_HEIGHT: f32 = 44.0;
pub const STATUS_BAR_HEIGHT: f32 = 26.0;

const _: () = assert!(
    TOOL_RAIL_WIDTH + PROPERTIES_PANEL_MAX_WIDTH + CANVAS_MIN_WIDTH <= MIN_WINDOW_SIZE[0],
    "shell panels must leave room for the minimum canvas width"
);

const _: () = assert!(
    MENU_BAR_HEIGHT + TOP_TOOLBAR_HEIGHT + STATUS_BAR_HEIGHT + CANVAS_MIN_HEIGHT
        <= MIN_WINDOW_SIZE[1],
    "shell bars must leave room for the minimum canvas height"
);

const _: () = assert!(
    PROPERTIES_PANEL_MIN_WIDTH <= PROPERTIES_PANEL_WIDTH
        && PROPERTIES_PANEL_WIDTH <= PROPERTIES_PANEL_MAX_WIDTH,
    "properties panel default width must be within its resize range"
);

/// Widest the properties panel may become given the width left over after the
/// tool rail, so the canvas is never squeezed below its minimum width.
pub fn properties_panel_max_width(available_width: f32) -> f32 {
    (available_width - CANVAS_MIN_WIDTH)
        .clamp(PROPERTIES_PANEL_MIN_WIDTH, PROPERTIES_PANEL_MAX_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_panel_can_grow_at_minimum_window() {
        let available = MIN_WINDOW_SIZE[0] - TOOL_RAIL_WIDTH;
        assert_eq!(
            properties_panel_max_width(available),
            PROPERTIES_PANEL_MAX_WIDTH
        );
    }

    #[test]
    fn properties_panel_never_starves_canvas() {
        assert_eq!(
            properties_panel_max_width(CANVAS_MIN_WIDTH + 10.0),
            PROPERTIES_PANEL_MIN_WIDTH
        );
        assert_eq!(properties_panel_max_width(0.0), PROPERTIES_PANEL_MIN_WIDTH);
    }
}
