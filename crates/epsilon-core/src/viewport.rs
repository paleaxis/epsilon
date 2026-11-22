//! Viewport (camera) model and world/screen transforms.
//!
//! This module is deliberately free of any UI or rendering crate: it only deals
//! in core geometry. The application layer owns the screen size and pointer
//! positions and feeds them here.
//!
//! Coordinate convention:
//!
//! - `Viewport::offset` is the screen-space position of the world origin.
//! - `Viewport::zoom` scales world units into screen (logical) pixels.
//! - `world_to_screen(p) = p * zoom + offset`
//! - `screen_to_world(p) = (p - offset) / zoom`

use crate::geometry::{Point, Rect};

/// Smallest allowed zoom factor.
pub const MIN_ZOOM: f64 = 0.1;
/// Largest allowed zoom factor.
pub const MAX_ZOOM: f64 = 30.0;

/// Camera state for the infinite canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Screen-space position of the world origin.
    pub offset: Point,
    /// World units to screen (logical) pixels. Always within `[MIN_ZOOM, MAX_ZOOM]`.
    pub zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            offset: Point::ZERO,
            zoom: 1.0,
        }
    }
}

impl Viewport {
    /// Creates a viewport at the origin with 100% zoom.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clamps an arbitrary zoom value to the supported range.
    pub fn clamp_zoom(zoom: f64) -> f64 {
        if zoom.is_finite() {
            zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else if zoom == f64::INFINITY {
            MAX_ZOOM
        } else {
            MIN_ZOOM
        }
    }

    /// World point to screen (logical) point.
    pub fn world_to_screen(&self, point: Point) -> Point {
        point * self.zoom + self.offset
    }

    /// Screen (logical) point to world point.
    pub fn screen_to_world(&self, point: Point) -> Point {
        (point - self.offset) * (1.0 / self.zoom)
    }

    /// World rectangle to screen (logical) rectangle.
    pub fn world_rect_to_screen(&self, rect: Rect) -> Rect {
        Rect::from_corners(
            self.world_to_screen(rect.min),
            self.world_to_screen(rect.max),
        )
    }

    /// Screen (logical) rectangle to world rectangle.
    pub fn screen_rect_to_world(&self, rect: Rect) -> Rect {
        Rect::from_corners(
            self.screen_to_world(rect.min),
            self.screen_to_world(rect.max),
        )
    }

    /// Sets the zoom, clamped to the supported range, keeping `anchor` (a
    /// screen point) over the same world point.
    pub fn zoom_at(&mut self, anchor: Point, new_zoom: f64) {
        let new_zoom = Self::clamp_zoom(new_zoom);
        if (new_zoom - self.zoom).abs() <= f64::EPSILON {
            self.zoom = new_zoom;
            return;
        }
        let world_anchor = self.screen_to_world(anchor);
        self.zoom = new_zoom;
        self.offset = anchor - world_anchor * self.zoom;
    }

    /// Multiplies the current zoom by `factor`, anchored at `anchor`.
    pub fn zoom_by(&mut self, factor: f64, anchor: Point) {
        self.zoom_at(anchor, self.zoom * factor);
    }

    /// Replaces the zoom with an absolute value, clamped to the supported range.
    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = Self::clamp_zoom(zoom);
    }

    /// Pans the viewport by a screen-space delta.
    pub fn pan_by(&mut self, delta: Point) {
        self.offset = self.offset + delta;
    }

    /// Adjusts the viewport so `rect` fits inside a screen of `screen_size`,
    /// centered, with a small margin. A degenerate (empty or zero-area) rect
    /// is treated as a single point.
    pub fn fit_to_rect(&mut self, rect: Rect, screen_size: Point) {
        let width = rect.width().max(f64::EPSILON);
        let height = rect.height().max(f64::EPSILON);
        let screen_width = screen_size.x.max(f64::EPSILON);
        let screen_height = screen_size.y.max(f64::EPSILON);

        let zoom = Self::clamp_zoom((screen_width / width).min(screen_height / height));
        // Leave a little breathing room around the fitted content.
        let zoom = Self::clamp_zoom(zoom * 0.9);

        self.zoom = zoom;
        let center = rect.center();
        self.offset = Point::new(
            screen_size.x * 0.5 - center.x * zoom,
            screen_size.y * 0.5 - center.y * zoom,
        );
    }

    /// Alias for [`Self::fit_to_rect`], matching the roadmap wording.
    pub fn fit_bounds(&mut self, rect: Rect, screen_size: Point) {
        self.fit_to_rect(rect, screen_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn default_viewport_is_identity() {
        let viewport = Viewport::default();
        assert_eq!(viewport.offset, Point::ZERO);
        assert_eq!(viewport.zoom, 1.0);
        let point = Point::new(12.5, -7.25);
        assert_eq!(viewport.world_to_screen(point), point);
        assert_eq!(viewport.screen_to_world(point), point);
    }

    #[test]
    fn world_screen_round_trip() {
        let viewport = Viewport {
            offset: Point::new(37.0, -12.5),
            zoom: 2.25,
        };
        for point in [
            Point::ZERO,
            Point::new(1.0, 2.0),
            Point::new(-100.0, 42.5),
            Point::new(1e6, -1e6),
        ] {
            let screen = viewport.world_to_screen(point);
            let world = viewport.screen_to_world(screen);
            assert!(approx(world.x, point.x), "{world:?} != {point:?}");
            assert!(approx(world.y, point.y), "{world:?} != {point:?}");
        }
    }

    #[test]
    fn screen_world_round_trip() {
        let viewport = Viewport {
            offset: Point::new(-5.0, 9.0),
            zoom: 0.5,
        };
        for point in [Point::ZERO, Point::new(100.0, -33.0), Point::new(-1.0, 1.0)] {
            let world = viewport.screen_to_world(point);
            let screen = viewport.world_to_screen(world);
            assert!(approx(screen.x, point.x));
            assert!(approx(screen.y, point.y));
        }
    }

    #[test]
    fn zoom_is_clamped() {
        assert_eq!(Viewport::clamp_zoom(0.01), MIN_ZOOM);
        assert_eq!(Viewport::clamp_zoom(100.0), MAX_ZOOM);
        assert_eq!(Viewport::clamp_zoom(1.5), 1.5);
        assert_eq!(Viewport::clamp_zoom(f64::NAN), MIN_ZOOM);
        assert_eq!(Viewport::clamp_zoom(f64::INFINITY), MAX_ZOOM);
    }

    #[test]
    fn cursor_anchored_zoom_keeps_anchor_world_point() {
        let mut viewport = Viewport {
            offset: Point::new(10.0, 20.0),
            zoom: 1.0,
        };
        let anchor = Point::new(123.0, 45.0);
        let world_before = viewport.screen_to_world(anchor);

        viewport.zoom_at(anchor, 4.0);

        let world_after = viewport.screen_to_world(anchor);
        assert!(approx(world_after.x, world_before.x));
        assert!(approx(world_after.y, world_before.y));
        assert_eq!(viewport.zoom, 4.0);
    }

    #[test]
    fn zoom_by_uses_factor_and_anchor() {
        let mut viewport = Viewport::default();
        let anchor = Point::new(10.0, 10.0);
        viewport.zoom_by(2.0, anchor);
        assert_eq!(viewport.zoom, 2.0);
        viewport.zoom_by(2.0, anchor);
        assert_eq!(viewport.zoom, 4.0);
    }

    #[test]
    fn zoom_at_clamps_extreme_values() {
        let mut viewport = Viewport::default();
        viewport.zoom_at(Point::ZERO, 1000.0);
        assert_eq!(viewport.zoom, MAX_ZOOM);
        viewport.zoom_at(Point::ZERO, 0.001);
        assert_eq!(viewport.zoom, MIN_ZOOM);
    }

    #[test]
    fn fit_to_rect_centers_content() {
        let mut viewport = Viewport::default();
        let rect = Rect::from_xywh(10.0, 20.0, 100.0, 50.0);
        let screen = Point::new(800.0, 600.0);

        viewport.fit_to_rect(rect, screen);

        let screen_rect = viewport.world_rect_to_screen(rect);
        assert!(screen_rect.min.x >= 0.0);
        assert!(screen_rect.max.x <= screen.x);
        assert!(screen_rect.min.y >= 0.0);
        assert!(screen_rect.max.y <= screen.y);

        // Center of the rect lands at the center of the screen.
        let center = viewport.world_to_screen(rect.center());
        assert!(approx(center.x, screen.x * 0.5));
        assert!(approx(center.y, screen.y * 0.5));
    }

    #[test]
    fn fit_bounds_alias_matches_fit_to_rect() {
        let mut a = Viewport::default();
        let mut b = Viewport::default();
        let rect = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let screen = Point::new(200.0, 100.0);
        a.fit_to_rect(rect, screen);
        b.fit_bounds(rect, screen);
        assert_eq!(a, b);
    }

    #[test]
    fn fit_to_rect_handles_degenerate_rect() {
        let mut viewport = Viewport::default();
        let rect = Rect::from_xywh(5.0, 5.0, 0.0, 0.0);
        let screen = Point::new(400.0, 300.0);
        viewport.fit_to_rect(rect, screen);
        assert!(viewport.zoom.is_finite());
        let center = viewport.world_to_screen(Point::new(5.0, 5.0));
        assert!(approx(center.x, screen.x * 0.5));
        assert!(approx(center.y, screen.y * 0.5));
    }

    #[test]
    fn pan_by_translates_offset() {
        let mut viewport = Viewport::default();
        viewport.pan_by(Point::new(12.0, -4.0));
        assert_eq!(viewport.offset, Point::new(12.0, -4.0));
    }
}
