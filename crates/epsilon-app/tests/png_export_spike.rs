//! Task 6.1 raster-backend spike.
//!
//! Renders one styled rectangle and one text run to a real PNG using a CPU
//! path rasterizer (`tiny-skia`) and glyph coverage (`fontdue` with egui's
//! embedded Ubuntu font). This validates the chosen deterministic backend
//! without creating a GPU/GL offscreen surface.

use epsilon_core::document::{Color, Document, Element, ElementKind, ElementStyle};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

fn fill_rectangle(pixmap: &mut Pixmap, x: f32, y: f32, width: f32, height: f32, color: Color) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);
    paint.anti_alias = true;
    let mut builder = PathBuilder::new();
    builder.push_rect(tiny_skia::Rect::from_xywh(x, y, width, height).expect("valid rect"));
    let path = builder.finish().expect("rectangle path");
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

/// Blends an 8-bit glyph coverage mask into premultiplied RGBA pixels.
fn blend_glyph(
    pixmap: &mut Pixmap,
    origin_x: i32,
    origin_y: i32,
    width: usize,
    height: usize,
    coverage: &[u8],
    color: Color,
) {
    let pixmap_width = pixmap.width() as i32;
    let pixmap_height = pixmap.height() as i32;
    let row_stride = pixmap.width() as usize;
    let data = pixmap.data_mut();
    for row in 0..height {
        for column in 0..width {
            let alpha = coverage[row * width + column] as u32;
            if alpha == 0 {
                continue;
            }
            let x = origin_x + column as i32;
            let y = origin_y + row as i32;
            if x < 0 || y < 0 || x >= pixmap_width || y >= pixmap_height {
                continue;
            }
            let offset = (y as usize * row_stride + x as usize) * 4;
            let inverse = 255 - alpha;
            for (channel, value) in [color.r, color.g, color.b].into_iter().enumerate() {
                let source = value as u32 * alpha / 255;
                data[offset + channel] =
                    (source + data[offset + channel] as u32 * inverse / 255) as u8;
            }
            data[offset + 3] = 255;
        }
    }
}

fn draw_text(
    pixmap: &mut Pixmap,
    font: &fontdue::Font,
    text: &str,
    x: f32,
    baseline: f32,
    size: f32,
    color: Color,
) {
    let mut pen_x = x;
    for character in text.chars() {
        let (metrics, coverage) = font.rasterize(character, size);
        let glyph_x = pen_x.round() as i32 + metrics.xmin;
        let glyph_y = baseline.round() as i32 - (metrics.ymin + metrics.height as i32);
        blend_glyph(
            pixmap,
            glyph_x,
            glyph_y,
            metrics.width,
            metrics.height,
            &coverage,
            color,
        );
        pen_x += metrics.advance_width;
    }
}

#[test]
fn cpu_backend_renders_rectangle_and_text_to_png() {
    // The document is the source of truth; this spike uses its geometry and
    // style directly rather than capturing UI panels.
    let mut document = Document::new();
    let mut rectangle = Element::new(ElementKind::Rectangle, 24.0, 28.0, 220.0, 110.0);
    rectangle.style = ElementStyle {
        stroke: Color::rgb(30, 30, 30),
        fill: Color::rgb(165, 216, 255),
        stroke_width: 3.0,
        ..ElementStyle::default()
    };
    document.add(rectangle);
    let id = document.add(Element::new(ElementKind::Text, 36.0, 178.0, 260.0, 40.0));
    if let Some(text) = document
        .elements
        .iter_mut()
        .find(|element| element.id == id)
    {
        text.text = "Epsilon PNG".to_owned();
        text.font_size = 36.0;
        text.style.stroke = Color::rgb(20, 20, 20);
    }

    let width = 320;
    let height = 240;
    let mut pixmap = Pixmap::new(width, height).expect("allocate pixmap");
    pixmap.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));

    let rectangle = &document.elements[0];
    fill_rectangle(
        &mut pixmap,
        rectangle.x as f32,
        rectangle.y as f32,
        rectangle.width as f32,
        rectangle.height as f32,
        rectangle.style.fill,
    );

    let definitions = eframe::egui::FontDefinitions::default();
    let font_data = definitions
        .font_data
        .get("Ubuntu-Light")
        .expect("egui embeds the default Ubuntu font");
    let bytes = font_data.font.as_ref().as_ref().to_vec();
    let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
        .expect("parse embedded Ubuntu font");
    draw_text(
        &mut pixmap,
        &font,
        "Epsilon PNG",
        36.0,
        204.0,
        36.0,
        Color::rgb(20, 20, 20),
    );

    let path = std::env::temp_dir().join(format!(
        "epsilon-png-spike-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    ));
    pixmap.save_png(&path).expect("write PNG");
    let bytes = std::fs::read(&path).expect("read PNG");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");

    let decoded = Pixmap::decode_png(&bytes).expect("decode PNG");
    assert_eq!((decoded.width(), decoded.height()), (width, height));
    let non_background = decoded
        .pixels()
        .iter()
        .filter(|pixel| pixel.red() != 255 || pixel.green() != 255 || pixel.blue() != 255)
        .count();
    assert!(
        non_background > 500,
        "expected rectangle/text pixels, got {non_background}"
    );
    assert!(
        path.metadata().expect("PNG metadata").len() > 100,
        "PNG should contain image data"
    );

    std::fs::remove_file(&path).ok();
}
