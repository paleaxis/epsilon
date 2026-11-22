//! Foundational 2D geometry shared by the canvas engine, document model and
//! exporters.
//!
//! Conventions:
//!
//! - World space is measured in logical pixels at 100% zoom and uses a
//!   **y-down** axis, matching the Excalidraw document format.
//! - Coordinates are `f64`. Excalidraw stores numbers as IEEE-754 doubles and an
//!   infinite canvas needs the extra precision at deep zoom levels; conversion
//!   to `f32` happens only at the rendering boundary.

use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// A point (position) or displacement (vector) in 2D space.
///
/// Using one type for both is a deliberate simplification: the engine performs
/// little vector algebra and having distinct `Point`/`Vector` types would add
/// conversions without preventing real bugs.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Dot product with `other`.
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Euclidean length.
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    /// Euclidean distance to `other`.
    pub fn distance(self, other: Self) -> f64 {
        (self - other).length()
    }

    /// Unit vector in the same direction, or [`None`] for a zero-length vector.
    pub fn normalized(self) -> Option<Self> {
        let length = self.length();
        (length > f64::EPSILON).then(|| self * (1.0 / length))
    }

    /// Linear interpolation: `t = 0` yields `self`, `t = 1` yields `other`.
    pub fn lerp(self, other: Self, t: f64) -> Self {
        self + (other - self) * t
    }

    /// Conversion for rendering. Safe for on-screen coordinates.
    pub fn to_f32(self) -> [f32; 2] {
        [self.x as f32, self.y as f32]
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Point {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Point {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<f64> for Point {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl Neg for Point {
    type Output = Self;

    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

/// An axis-aligned rectangle, stored as normalized minimum/maximum corners.
///
/// All constructors normalize their input, so `min.x <= max.x` and
/// `min.y <= max.y` always hold. A zero-area rectangle is "empty" but still a
/// valid location (for example the bounds of a single point).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    /// Builds a rectangle from two opposite corners in any order.
    pub fn from_corners(a: Point, b: Point) -> Self {
        Self {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// Builds a rectangle from an origin and a size. Negative sizes are
    /// normalized rather than rejected.
    pub fn from_xywh(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::from_corners(Point::new(x, y), Point::new(x + width, y + height))
    }

    /// Smallest rectangle containing every point. Returns [`None`] if empty.
    pub fn from_points(points: impl IntoIterator<Item = Point>) -> Option<Self> {
        let mut iter = points.into_iter();
        let first = iter.next()?;
        Some(iter.fold(
            Self {
                min: first,
                max: first,
            },
            |acc, point| Self {
                min: Point::new(acc.min.x.min(point.x), acc.min.y.min(point.y)),
                max: Point::new(acc.max.x.max(point.x), acc.max.y.max(point.y)),
            },
        ))
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn size(&self) -> Point {
        self.max - self.min
    }

    pub fn center(&self) -> Point {
        self.min.lerp(self.max, 0.5)
    }

    /// Vertices of the diamond inscribed in this rectangle, clockwise from the
    /// top edge midpoint.
    pub fn diamond_points(&self) -> [Point; 4] {
        let center = self.center();
        [
            Point::new(center.x, self.min.y),
            Point::new(self.max.x, center.y),
            Point::new(center.x, self.max.y),
            Point::new(self.min.x, center.y),
        ]
    }

    /// `true` if either dimension has zero area.
    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    /// Inclusive containment test: points exactly on an edge are inside.
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    /// `true` if the rectangles overlap or merely touch.
    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }

    /// Bounding rectangle of both rectangles.
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// Grows the rectangle by `margin` on every side. Negative margins shrink
    /// it, and shrinking past zero collapses the rectangle to a point.
    pub fn expand(&self, margin: f64) -> Self {
        let center = self.center();
        let half_width = (self.width() / 2.0 + margin).max(0.0);
        let half_height = (self.height() / 2.0 + margin).max(0.0);
        Self::from_corners(
            Point::new(center.x - half_width, center.y - half_height),
            Point::new(center.x + half_width, center.y + half_height),
        )
    }

    pub fn translate(&self, by: Point) -> Self {
        Self {
            min: self.min + by,
            max: self.max + by,
        }
    }
}

/// Rotates `point` around `center` by `angle` radians. Positive angles are
/// clockwise in the y-down world convention.
pub fn rotate_point(point: Point, center: Point, angle: f64) -> Point {
    let offset = point - center;
    let (sin, cos) = angle.sin_cos();
    center
        + Point::new(
            offset.x * cos - offset.y * sin,
            offset.x * sin + offset.y * cos,
        )
}

/// Even-odd containment test for an arbitrary polygon.
pub fn point_in_polygon(point: Point, polygon: &[Point]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for &current in polygon {
        let crosses = (current.y > point.y) != (previous.y > point.y)
            && point.x
                < (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y)
                    + current.x;
        if crosses {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

/// Shortest distance from `point` to the segment `start..end`.
pub fn distance_to_segment(point: Point, start: Point, end: Point) -> f64 {
    let segment = end - start;
    let length_squared = segment.dot(segment);
    if length_squared <= f64::EPSILON {
        return point.distance(start);
    }
    let t = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

/// Shortest distance from `point` to the nearest edge of `rect`. Points inside
/// return their distance to the closest side; outside points measure to the
/// clamped nearest point on the rectangle.
pub fn distance_to_rect(point: Point, rect: Rect) -> f64 {
    if rect.contains(point) {
        return (point.x - rect.min.x)
            .min(rect.max.x - point.x)
            .min(point.y - rect.min.y)
            .min(rect.max.y - point.y);
    }
    let nearest = Point::new(
        point.x.clamp(rect.min.x, rect.max.x),
        point.y.clamp(rect.min.y, rect.max.y),
    );
    point.distance(nearest)
}

/// Shortest distance from `point` to a polyline, optionally closing the loop.
pub fn distance_to_polyline(point: Point, points: &[Point], closed: bool) -> f64 {
    if points.is_empty() {
        return f64::INFINITY;
    }
    if points.len() == 1 {
        return point.distance(points[0]);
    }

    let mut distance = points
        .windows(2)
        .map(|pair| distance_to_segment(point, pair[0], pair[1]))
        .fold(f64::INFINITY, f64::min);
    if closed {
        distance = distance.min(distance_to_segment(
            point,
            points[points.len() - 1],
            points[0],
        ));
    }
    distance
}

/// Smooths an open polyline with a three-point moving average, preserving its
/// endpoints. Extra passes increase smoothing.
pub fn smooth_polyline(points: &[Point], passes: usize) -> Vec<Point> {
    if points.len() < 3 || passes == 0 {
        return points.to_vec();
    }

    let mut current = points.to_vec();
    for _ in 0..passes {
        let mut smoothed = Vec::with_capacity(current.len());
        smoothed.push(current[0]);
        for window in current.windows(3) {
            smoothed.push((window[0] + window[1] + window[2]) * (1.0 / 3.0));
        }
        smoothed.push(current[current.len() - 1]);
        current = smoothed;
    }
    current
}

/// Reduces an open polyline with Ramer–Douglas–Peucker, keeping endpoints and
/// any point farther than `tolerance` from the simplified span.
pub fn simplify_polyline(points: &[Point], tolerance: f64) -> Vec<Point> {
    if points.len() <= 2 || !tolerance.is_finite() || tolerance <= 0.0 {
        return points.to_vec();
    }

    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut spans = vec![(0, points.len() - 1)];

    while let Some((first, last)) = spans.pop() {
        if last <= first + 1 {
            continue;
        }
        let mut farthest = 0.0;
        let mut farthest_index = first;
        for (index, point) in points.iter().enumerate().take(last).skip(first + 1) {
            let distance = distance_to_segment(*point, points[first], points[last]);
            if distance > farthest {
                farthest = distance;
                farthest_index = index;
            }
        }
        if farthest > tolerance {
            keep[farthest_index] = true;
            spans.push((first, farthest_index));
            spans.push((farthest_index, last));
        }
    }

    points
        .iter()
        .zip(keep)
        .filter_map(|(point, keep)| keep.then_some(*point))
        .collect()
}

/// Tip and base vertices of an arrowhead ending at `end`.
///
/// The head is clamped to half the shaft and its width to the head length, so
/// short arrows keep a visible shaft and never extend behind `start`.
pub fn arrowhead_points(
    start: Point,
    end: Point,
    head_length: f64,
    half_width: f64,
) -> Option<[Point; 3]> {
    let delta = end - start;
    let length = delta.length();
    let direction = delta.normalized()?;
    if !head_length.is_finite() || head_length <= 0.0 {
        return None;
    }

    // Clamp the head to half the shaft so very short arrows keep a visible
    // shaft instead of letting the head extend back past the start point.
    let head_length = head_length.min(length * 0.5);
    let half_width = half_width.max(0.0).min(head_length * 0.75);
    let base = end - direction * head_length;
    let normal = Point::new(-direction.y, direction.x);
    Some([end, base + normal * half_width, base - normal * half_width])
}

/// Rotates `end` around `origin` to the nearest multiple of `step_radians`,
/// preserving its distance from `origin`.
///
/// Zero-length segments and non-positive or non-finite steps are returned
/// unchanged.
pub fn constrain_angle(origin: Point, end: Point, step_radians: f64) -> Point {
    let delta = end - origin;
    let Some(direction) = delta.normalized() else {
        return end;
    };
    if !step_radians.is_finite() || step_radians <= 0.0 {
        return end;
    }

    let angle = direction.y.atan2(direction.x);
    let snapped = (angle / step_radians).round() * step_radians;
    origin + Point::new(snapped.cos(), snapped.sin()) * delta.length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn point_arithmetic() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(3.0, 4.0);
        assert_eq!(a + b, Point::new(4.0, 6.0));
        assert_eq!(b - a, Point::new(2.0, 2.0));
        assert_eq!(a * 2.0, Point::new(2.0, 4.0));
        assert_eq!(-a, Point::new(-1.0, -2.0));
        assert_eq!(a.dot(b), 11.0);
    }

    #[test]
    fn point_length_and_normalization() {
        let v = Point::new(3.0, 4.0);
        assert!(approx(v.length(), 5.0));
        let unit = v.normalized().expect("non-zero vector normalizes");
        assert!(approx(unit.length(), 1.0));
        assert!(approx(unit.x, 0.6));
        assert!(approx(unit.y, 0.8));
        assert_eq!(Point::ZERO.normalized(), None);
    }

    #[test]
    fn point_lerp() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 20.0);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, 0.5), Point::new(5.0, 10.0));
    }

    #[test]
    fn rect_from_corners_normalizes_order() {
        let rect = Rect::from_corners(Point::new(10.0, 20.0), Point::new(0.0, 5.0));
        assert_eq!(rect.min, Point::new(0.0, 5.0));
        assert_eq!(rect.max, Point::new(10.0, 20.0));
        assert!(approx(rect.width(), 10.0));
        assert!(approx(rect.height(), 15.0));
    }

    #[test]
    fn rect_from_negative_size_normalizes() {
        let rect = Rect::from_xywh(10.0, 10.0, -4.0, -6.0);
        assert_eq!(rect.min, Point::new(6.0, 4.0));
        assert_eq!(rect.max, Point::new(10.0, 10.0));
    }

    #[test]
    fn rect_center_and_size() {
        let rect = Rect::from_xywh(0.0, 0.0, 20.0, 10.0);
        assert_eq!(rect.center(), Point::new(10.0, 5.0));
        assert_eq!(rect.size(), Point::new(20.0, 10.0));
        assert!(!rect.is_empty());
        assert!(Rect::default().is_empty());
    }

    #[test]
    fn diamond_points_inscribe_rectangle_edges() {
        let rect = Rect::from_xywh(10.0, 20.0, 80.0, 40.0);
        assert_eq!(
            rect.diamond_points(),
            [
                Point::new(50.0, 20.0),
                Point::new(90.0, 40.0),
                Point::new(50.0, 60.0),
                Point::new(10.0, 40.0),
            ]
        );

        let points = rect.diamond_points();
        assert_eq!(Rect::from_points(points), Some(rect));
    }

    #[test]
    fn rect_contains_includes_edges() {
        let rect = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        assert!(rect.contains(Point::new(5.0, 5.0)));
        assert!(rect.contains(Point::new(0.0, 0.0)));
        assert!(rect.contains(Point::new(10.0, 10.0)));
        assert!(!rect.contains(Point::new(10.1, 5.0)));
        assert!(!rect.contains(Point::new(-0.1, 5.0)));
    }

    #[test]
    fn rect_intersects_counts_touching_edges() {
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let overlapping = Rect::from_xywh(9.0, 9.0, 5.0, 5.0);
        let touching = Rect::from_xywh(10.0, 0.0, 5.0, 5.0);
        let disjoint = Rect::from_xywh(11.0, 0.0, 5.0, 5.0);
        assert!(a.intersects(&overlapping));
        assert!(a.intersects(&touching));
        assert!(!a.intersects(&disjoint));
    }

    #[test]
    fn rect_union_and_expand() {
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let b = Rect::from_xywh(20.0, 5.0, 10.0, 10.0);
        let union = a.union(&b);
        assert_eq!(union.min, Point::new(0.0, 0.0));
        assert_eq!(union.max, Point::new(30.0, 15.0));

        let grown = a.expand(5.0);
        assert_eq!(grown.min, Point::new(-5.0, -5.0));
        assert_eq!(grown.max, Point::new(15.0, 15.0));

        let collapsed = a.expand(-100.0);
        assert!(collapsed.is_empty());
        assert_eq!(collapsed.center(), a.center());
    }

    #[test]
    fn rect_from_points() {
        assert_eq!(Rect::from_points([]), None);
        let rect = Rect::from_points([
            Point::new(2.0, 3.0),
            Point::new(-1.0, 8.0),
            Point::new(4.0, -2.0),
        ])
        .expect("non-empty iterator");
        assert_eq!(rect.min, Point::new(-1.0, -2.0));
        assert_eq!(rect.max, Point::new(4.0, 8.0));
    }

    #[test]
    fn rect_translate() {
        let rect = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let moved = rect.translate(Point::new(3.0, -4.0));
        assert_eq!(moved.min, Point::new(3.0, -4.0));
        assert_eq!(moved.size(), rect.size());
    }

    #[test]
    fn distance_to_segment_clamps_to_endpoints() {
        let start = Point::new(0.0, 0.0);
        let end = Point::new(10.0, 0.0);
        assert!(approx(
            distance_to_segment(Point::new(5.0, 3.0), start, end),
            3.0
        ));
        assert!(approx(
            distance_to_segment(Point::new(-4.0, 3.0), start, end),
            5.0
        ));
        assert!(approx(
            distance_to_segment(Point::new(10.0, 0.0), start, end),
            0.0
        ));
        assert!(approx(distance_to_segment(start, start, start), 0.0));
    }

    #[test]
    fn rotate_point_uses_y_down_clockwise_angles() {
        let center = Point::new(50.0, 10.0);
        let rotated = rotate_point(Point::new(50.0, 50.0), center, -std::f64::consts::FRAC_PI_2);
        assert!(approx(rotated.x, 90.0));
        assert!(approx(rotated.y, 10.0));
    }

    #[test]
    fn distance_to_rect_handles_inside_and_outside() {
        let rect = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        assert!(approx(distance_to_rect(Point::new(5.0, 5.0), rect), 5.0));
        assert!(approx(distance_to_rect(Point::new(0.0, 5.0), rect), 0.0));
        assert!(approx(distance_to_rect(Point::new(13.0, 14.0), rect), 5.0));
    }

    #[test]
    fn polygon_and_polyline_queries() {
        let square = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        assert!(point_in_polygon(Point::new(5.0, 5.0), &square));
        assert!(!point_in_polygon(Point::new(15.0, 5.0), &square));
        assert!(approx(
            distance_to_polyline(Point::new(5.0, 3.0), &square, true),
            3.0
        ));
        assert!(approx(
            distance_to_polyline(Point::new(5.0, 3.0), &square, false),
            3.0
        ));
    }

    #[test]
    fn smoothing_preserves_endpoints_and_reduces_jitter() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 4.0),
            Point::new(2.0, -4.0),
            Point::new(3.0, 4.0),
            Point::new(4.0, 0.0),
        ];
        let smoothed = smooth_polyline(&points, 1);

        assert_eq!(smoothed.len(), points.len());
        assert_eq!(smoothed[0], points[0]);
        assert_eq!(smoothed[smoothed.len() - 1], points[points.len() - 1]);
        assert!(smoothed[1].y.abs() < points[1].y.abs());
    }

    #[test]
    fn simplify_polyline_keeps_corners_and_drops_collinear_points() {
        let straight = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.01),
            Point::new(2.0, 0.0),
            Point::new(3.0, 0.02),
            Point::new(4.0, 0.0),
        ];
        assert_eq!(
            simplify_polyline(&straight, 0.1),
            vec![Point::new(0.0, 0.0), Point::new(4.0, 0.0)]
        );

        let corner = [
            Point::new(0.0, 0.0),
            Point::new(5.0, 0.0),
            Point::new(5.0, 5.0),
        ];
        assert_eq!(simplify_polyline(&corner, 0.1), corner);
    }

    #[test]
    fn arrowhead_points_scale_with_short_shafts() {
        let head = arrowhead_points(Point::ZERO, Point::new(100.0, 0.0), 12.0, 6.0)
            .expect("non-zero arrow");
        assert_eq!(head[0], Point::new(100.0, 0.0));
        assert!(approx(head[1].x, 88.0));
        assert!(approx(head[1].y, 6.0));
        assert!(approx(head[2].x, 88.0));
        assert!(approx(head[2].y, -6.0));

        let short =
            arrowhead_points(Point::ZERO, Point::new(4.0, 0.0), 12.0, 6.0).expect("short arrow");
        assert!(approx(short[1].x, 2.0));
        assert!(approx(short[1].y, 1.5));
        assert!(short[1].x >= 0.0 && short[2].x >= 0.0);

        assert_eq!(arrowhead_points(Point::ZERO, Point::ZERO, 12.0, 6.0), None);
        assert_eq!(
            arrowhead_points(Point::ZERO, Point::new(1.0, 0.0), 0.0, 6.0),
            None
        );
    }

    #[test]
    fn constrain_angle_snaps_to_fifteen_degree_steps() {
        let origin = Point::new(10.0, 20.0);
        let step = std::f64::consts::PI / 12.0;

        let horizontal = constrain_angle(origin, Point::new(110.0, 30.0), step);
        assert!(approx(horizontal.x - origin.x, (100.0f64).hypot(10.0)));
        assert!(approx(horizontal.y, origin.y));

        let diagonal = constrain_angle(origin, origin + Point::new(100.0, 100.0), step);
        assert!(approx(diagonal.x - origin.x, 100.0));
        assert!(approx(diagonal.y - origin.y, 100.0));

        let near_vertical = constrain_angle(origin, origin + Point::new(10.0, -100.0), step);
        assert!(approx(near_vertical.x, origin.x));
        assert!(approx(near_vertical.y - origin.y, -(100.0f64).hypot(10.0)));
    }

    #[test]
    fn constrain_angle_preserves_zero_and_invalid_steps() {
        let origin = Point::new(4.0, 5.0);
        let end = Point::new(9.0, 7.0);

        assert_eq!(constrain_angle(origin, origin, 0.1), origin);
        assert_eq!(constrain_angle(origin, end, 0.0), end);
        assert_eq!(constrain_angle(origin, end, -0.1), end);
        assert_eq!(constrain_angle(origin, end, f64::NAN), end);
    }
}
