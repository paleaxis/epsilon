//! Adaptive grid helpers for the canvas.
//!
//! The grid itself is drawn by the application layer, but the spacing and
//! snapping math lives here so it can be unit tested without egui.

/// Default distance between grid lines in world units.
pub const DEFAULT_GRID_SIZE: f64 = 20.0;

/// Selects a grid step (in world units) whose on-screen spacing stays within
/// `[min_screen_spacing, max_screen_spacing]`.
///
/// The step is derived from `base_size` by powers of two, so snapping remains
/// stable as the zoom changes and lines never become unusably dense or sparse.
pub fn grid_step(
    base_size: f64,
    zoom: f64,
    min_screen_spacing: f64,
    max_screen_spacing: f64,
) -> f64 {
    if !base_size.is_finite() || base_size <= 0.0 || !zoom.is_finite() || zoom <= 0.0 {
        return base_size.abs().max(f64::EPSILON);
    }

    let min_screen = min_screen_spacing.max(f64::EPSILON);
    let max_screen = max_screen_spacing.max(min_screen);

    let mut step = base_size;
    // Make the grid coarser while it is too dense on screen.
    while step * zoom < min_screen && step < f64::MAX / 2.0 {
        step *= 2.0;
    }
    // Make the grid finer while it is too sparse on screen, but never below a
    // small fraction of the base size.
    let floor = base_size / 1024.0;
    while step * zoom > max_screen && step > floor {
        step /= 2.0;
    }

    step
}

/// Snaps a scalar world coordinate to the nearest multiple of `step`.
pub fn snap_scalar(value: f64, step: f64) -> f64 {
    if !step.is_finite() || step <= 0.0 {
        return value;
    }
    (value / step).round() * step
}

/// Snaps a point to the grid.
pub fn snap_point(point: crate::geometry::Point, step: f64) -> crate::geometry::Point {
    crate::geometry::Point::new(snap_scalar(point.x, step), snap_scalar(point.y, step))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[test]
    fn default_zoom_keeps_base_step() {
        assert_eq!(grid_step(20.0, 1.0, 8.0, 64.0), 20.0);
    }

    #[test]
    fn low_zoom_coarsens_step() {
        let step = grid_step(20.0, 0.1, 8.0, 64.0);
        assert!(step > 20.0);
        assert!(step * 0.1 >= 8.0);
    }

    #[test]
    fn high_zoom_refines_step() {
        let step = grid_step(20.0, 10.0, 8.0, 64.0);
        assert!(step < 20.0);
        assert!(step * 10.0 <= 64.0);
    }

    #[test]
    fn screen_spacing_stays_in_range() {
        for zoom in [0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0] {
            let step = grid_step(20.0, zoom, 8.0, 64.0);
            let spacing = step * zoom;
            assert!(
                (8.0..=64.0).contains(&spacing),
                "zoom {zoom}: step {step}, spacing {spacing}"
            );
        }
    }

    #[test]
    fn snap_scalar_rounds_to_nearest_step() {
        assert_eq!(snap_scalar(0.0, 20.0), 0.0);
        assert_eq!(snap_scalar(9.0, 20.0), 0.0);
        assert_eq!(snap_scalar(11.0, 20.0), 20.0);
        assert_eq!(snap_scalar(-9.0, 20.0), 0.0);
        assert_eq!(snap_scalar(-11.0, 20.0), -20.0);
    }

    #[test]
    fn snap_point_uses_same_step() {
        let point = Point::new(13.0, -7.0);
        assert_eq!(snap_point(point, 10.0), Point::new(10.0, -10.0));
    }

    #[test]
    fn invalid_step_returns_input() {
        assert_eq!(snap_scalar(5.0, 0.0), 5.0);
        assert_eq!(snap_scalar(5.0, f64::NAN), 5.0);
    }
}
