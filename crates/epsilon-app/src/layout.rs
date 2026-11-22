//! Shell layout constants.
//!
//! The window minimum size and the panel sizes are defined together so the
//! canvas always has usable room at the smallest supported window. The
//! invariants are checked at compile time below.

/// Minimum inner window size (logical points), enforced by the native window.
pub const MIN_WINDOW_SIZE: [f32; 2] = [720.0, 480.0];

/// Left tool rail (fixed width).
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
