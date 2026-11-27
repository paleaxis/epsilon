//! Application icon, generated at startup.
//!
//! The window icon is rasterized from code so the binary carries no external
//! image asset at runtime (task 7.2). The same design is authored as
//! `packaging/epsilon.svg` for the desktop launcher; keep the two in sync.

use eframe::egui;

/// Rendered icon edge length in pixels. Large enough for compositors that pick
/// a high-resolution icon without requiring a separate multi-size asset.
const ICON_SIZE: u32 = 256;

/// Rounded-square background, matching the dark shell palette.
const BACKGROUND: [f32; 3] = [0x1D as f32, 0x20 as f32, 0x26 as f32];
/// The whiteboard surface drawn inside the background.
const BOARD: [f32; 3] = [0xF3 as f32, 0xF4 as f32, 0xF6 as f32];
/// Accent color of the drawn epsilon mark.
const MARK: [f32; 3] = [0x8F as f32, 0x87 as f32, 0xFF as f32];

/// Vertical/horizontal supersampling per pixel. Cheap at startup and enough to
/// keep the rounded corners and the stroke smooth on HiDPI compositors.
const SUBSAMPLES: u32 = 4;

/// Builds the window icon for `ViewportBuilder::with_icon`.
pub fn app_icon() -> egui::IconData {
    egui::IconData {
        rgba: render(ICON_SIZE),
        width: ICON_SIZE,
        height: ICON_SIZE,
    }
}

/// Returns straight (non-premultiplied) RGBA8 pixels for a `size` square icon.
fn render(size: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (size as usize) * (size as usize) * 4];
    let scale = size as f32 / 256.0;
    let board = RoundedRect::new(
        [46.0 * scale, 50.0 * scale],
        [210.0 * scale, 206.0 * scale],
        18.0 * scale,
    );
    let background = RoundedRect::new([0.0; 2], [size as f32, size as f32], 56.0 * scale);
    let mark = epsilon_polyline(scale);
    let stroke_radius = 9.0 * scale;
    let stroke_radius_squared = stroke_radius * stroke_radius;
    // The mark covers a small part of the icon; testing its bounding box first
    // avoids the stroke test for the vast majority of samples. Within the box,
    // per-segment boxes skip the segments that cannot be close enough.
    let mark_bounds = Bounds::of_polyline(&mark, stroke_radius);
    let mark_segments = StrokeSegment::from_polyline(&mark, stroke_radius);
    let samples = (SUBSAMPLES * SUBSAMPLES) as f32;

    for y in 0..size {
        for x in 0..size {
            let offset = ((y as usize) * (size as usize) + x as usize) * 4;
            let pixel_min = [x as f32, y as f32];
            let pixel_max = [x as f32 + 1.0, y as f32 + 1.0];
            let corners = [
                [pixel_min[0], pixel_min[1]],
                [pixel_max[0], pixel_min[1]],
                [pixel_max[0], pixel_max[1]],
                [pixel_min[0], pixel_max[1]],
            ];

            // Both shapes are convex, so four covered corners mean the whole
            // pixel is covered. Most pixels are flat board or flat background;
            // filling them without supersampling is what keeps startup cheap.
            if corners.iter().all(|corner| background.contains(*corner)) {
                let fully_on_board = corners.iter().all(|corner| board.contains(*corner));
                if fully_on_board && !mark_bounds.overlaps(pixel_min, pixel_max) {
                    write_pixel(&mut pixels, offset, [BOARD[0], BOARD[1], BOARD[2], 255.0]);
                    continue;
                }
                if !board.overlaps(pixel_min, pixel_max) {
                    write_pixel(
                        &mut pixels,
                        offset,
                        [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 255.0],
                    );
                    continue;
                }
            }

            let mut rgba = [0.0f32; 4];
            for sy in 0..SUBSAMPLES {
                for sx in 0..SUBSAMPLES {
                    let point = [
                        x as f32 + (sx as f32 + 0.5) / SUBSAMPLES as f32,
                        y as f32 + (sy as f32 + 0.5) / SUBSAMPLES as f32,
                    ];
                    let mut sample = [0.0f32; 4];
                    if background.contains(point) {
                        sample = [BACKGROUND[0], BACKGROUND[1], BACKGROUND[2], 255.0];
                    }
                    let on_board = board.contains(point);
                    if on_board {
                        sample = blend(sample, [BOARD[0], BOARD[1], BOARD[2], 255.0]);
                    }
                    // The mark only paints over the board and lies fully inside
                    // its bounding box, so those two cheap tests gate the
                    // polyline walk.
                    if on_board
                        && mark_bounds.contains(point)
                        && within_stroke(point, &mark_segments, stroke_radius_squared)
                    {
                        sample = blend(sample, [MARK[0], MARK[1], MARK[2], 255.0]);
                    }
                    for channel in 0..4 {
                        rgba[channel] += sample[channel];
                    }
                }
            }
            write_pixel(
                &mut pixels,
                offset,
                [
                    rgba[0] / samples,
                    rgba[1] / samples,
                    rgba[2] / samples,
                    rgba[3] / samples,
                ],
            );
        }
    }
    pixels
}

fn write_pixel(pixels: &mut [u8], offset: usize, color: [f32; 4]) {
    for channel in 0..4 {
        pixels[offset + channel] = color[channel].round() as u8;
    }
}

/// Axis-aligned rectangle with rounded corners, in icon pixel coordinates.
struct RoundedRect {
    min: [f32; 2],
    max: [f32; 2],
    center: [f32; 2],
    corner: [f32; 2],
    radius_squared: f32,
}

impl RoundedRect {
    fn new(min: [f32; 2], max: [f32; 2], radius: f32) -> Self {
        let half = [(max[0] - min[0]) / 2.0, (max[1] - min[1]) / 2.0];
        let radius = radius.min(half[0]).min(half[1]);
        Self {
            min,
            max,
            center: [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0],
            corner: [half[0] - radius, half[1] - radius],
            radius_squared: radius * radius,
        }
    }

    fn contains(&self, [x, y]: [f32; 2]) -> bool {
        let dx = ((x - self.center[0]).abs() - self.corner[0]).max(0.0);
        let dy = ((y - self.center[1]).abs() - self.corner[1]).max(0.0);
        dx * dx + dy * dy <= self.radius_squared
    }

    /// Whether any part of the axially aligned `[min, max]` box lies inside the
    /// shape's bounding rectangle.
    fn overlaps(&self, min: [f32; 2], max: [f32; 2]) -> bool {
        min[0] <= self.max[0]
            && max[0] >= self.min[0]
            && min[1] <= self.max[1]
            && max[1] >= self.min[1]
    }
}

/// Bounding box of a polyline expanded by `radius`, used to skip the distance
/// computation for samples that cannot touch the stroke.
struct Bounds {
    min: [f32; 2],
    max: [f32; 2],
}

impl Bounds {
    fn contains(&self, [x, y]: [f32; 2]) -> bool {
        x >= self.min[0] && y >= self.min[1] && x <= self.max[0] && y <= self.max[1]
    }

    fn overlaps(&self, min: [f32; 2], max: [f32; 2]) -> bool {
        min[0] <= self.max[0]
            && max[0] >= self.min[0]
            && min[1] <= self.max[1]
            && max[1] >= self.min[1]
    }

    fn of_polyline(points: &[[f32; 2]], radius: f32) -> Self {
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for [x, y] in points {
            min[0] = min[0].min(*x);
            min[1] = min[1].min(*y);
            max[0] = max[0].max(*x);
            max[1] = max[1].max(*y);
        }
        Self {
            min: [min[0] - radius, min[1] - radius],
            max: [max[0] + radius, max[1] + radius],
        }
    }
}

/// One polyline segment with its bounds expanded by the stroke radius. A point
/// outside those bounds cannot be within the radius of the segment.
struct StrokeSegment {
    start: [f32; 2],
    end: [f32; 2],
    bounds: Bounds,
}

impl StrokeSegment {
    fn from_polyline(points: &[[f32; 2]], radius: f32) -> Vec<Self> {
        points
            .windows(2)
            .map(|segment| Self {
                start: segment[0],
                end: segment[1],
                bounds: Bounds::of_polyline(segment, radius),
            })
            .collect()
    }
}

/// The epsilon mark as a smoothed polyline. Chaikin smoothing keeps the curve
/// soft without a font or path dependency.
fn epsilon_polyline(scale: f32) -> Vec<[f32; 2]> {
    let mut points = vec![
        [176.0, 70.0],
        [150.0, 62.0],
        [112.0, 70.0],
        [96.0, 100.0],
        [96.0, 112.0],
        [118.0, 126.0],
        [148.0, 128.0],
        [118.0, 130.0],
        [96.0, 144.0],
        [96.0, 156.0],
        [112.0, 186.0],
        [150.0, 194.0],
        [176.0, 186.0],
    ];
    for _ in 0..2 {
        points = chaikin(&points);
    }
    points
        .into_iter()
        .map(|[x, y]| [x * scale, y * scale])
        .collect()
}

/// One round of corner cutting for a closed loop. The epsilon mark is open, so
/// the endpoints are preserved.
fn chaikin(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let mut smoothed = Vec::with_capacity(points.len() * 2);
    smoothed.push(points[0]);
    for pair in points.windows(2) {
        let [ax, ay] = pair[0];
        let [bx, by] = pair[1];
        smoothed.push([0.75 * ax + 0.25 * bx, 0.75 * ay + 0.25 * by]);
        smoothed.push([0.25 * ax + 0.75 * bx, 0.25 * ay + 0.75 * by]);
    }
    smoothed.push(points[points.len() - 1]);
    smoothed
}

/// Whether `point` lies within `radius_squared` of any segment. Comparing
/// squared distances avoids a square root per segment, and the per-segment
/// bounds skip most segments for points inside the mark's bounding box.
fn within_stroke([px, py]: [f32; 2], segments: &[StrokeSegment], radius_squared: f32) -> bool {
    let point = [px, py];
    segments.iter().any(|segment| {
        if !segment.bounds.contains(point) {
            return false;
        }
        let [ax, ay] = segment.start;
        let [bx, by] = segment.end;
        let dx = bx - ax;
        let dy = by - ay;
        let length_squared = dx * dx + dy * dy;
        let t = if length_squared <= f32::EPSILON {
            0.0
        } else {
            (((px - ax) * dx + (py - ay) * dy) / length_squared).clamp(0.0, 1.0)
        };
        let offset_x = px - (ax + t * dx);
        let offset_y = py - (ay + t * dy);
        offset_x * offset_x + offset_y * offset_y <= radius_squared
    })
}

/// Source-over blend of a straight-alpha sample onto an opaque/transparent one.
fn blend(destination: [f32; 4], source: [f32; 4]) -> [f32; 4] {
    let source_alpha = source[3] / 255.0;
    let destination_alpha = destination[3] / 255.0;
    let out_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if out_alpha <= f32::EPSILON {
        return [0.0; 4];
    }
    let mut out = [0.0; 4];
    for channel in 0..3 {
        out[channel] = (source[channel] * source_alpha
            + destination[channel] * destination_alpha * (1.0 - source_alpha))
            / out_alpha;
    }
    out[3] = out_alpha * 255.0;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y as usize) * (size as usize) + x as usize) * 4;
        [
            rgba[offset],
            rgba[offset + 1],
            rgba[offset + 2],
            rgba[offset + 3],
        ]
    }

    #[test]
    fn icon_has_expected_dimensions() {
        let icon = app_icon();
        assert_eq!(icon.width, ICON_SIZE);
        assert_eq!(icon.height, ICON_SIZE);
        assert_eq!(icon.rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
    }

    #[test]
    fn corners_are_transparent_and_surface_is_opaque() {
        let size = 64;
        let rgba = render(size);
        assert_eq!(pixel(&rgba, size, 0, 0)[3], 0);
        assert_eq!(pixel(&rgba, size, size - 1, size - 1)[3], 0);
        // A point inside the whiteboard surface is fully opaque and light.
        assert_eq!(pixel(&rgba, size, size / 2, size / 2)[3], 255);
        let has_light_surface = (0..size).any(|y| {
            (0..size).any(|x| {
                let [r, g, b, a] = pixel(&rgba, size, x, y);
                a == 255 && r > 200 && g > 200 && b > 200
            })
        });
        assert!(
            has_light_surface,
            "expected the whiteboard surface to render"
        );
        // A point near the edge but inside the rounded background is dark.
        let edge = pixel(&rgba, size, size / 2, 3);
        assert_eq!(edge[3], 255);
        assert!(edge[0] < 60 && edge[1] < 60 && edge[2] < 60);
    }

    #[test]
    fn icon_embeds_the_accent_mark() {
        let size = 128;
        let rgba = render(size);
        let accent = [MARK[0] as u8, MARK[1] as u8, MARK[2] as u8];
        let found = (0..size).any(|y| {
            (0..size).any(|x| {
                let [r, g, b, a] = pixel(&rgba, size, x, y);
                a == 255 && (r, g, b) == (accent[0], accent[1], accent[2])
            })
        });
        assert!(found, "expected the accent stroke to contribute pixels");
    }

    #[test]
    fn rendering_is_deterministic() {
        assert_eq!(render(32), render(32));
    }
}
