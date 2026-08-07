//! Drawing shared by the WebGL and WebGPU examples.
//!
//! Included by both with `#[path]` rather than being a crate: the two examples are
//! separate workspaces built with mutually exclusive features, and the point of the
//! pair is that the *same* drawing code runs on both backends.
//!
//! The tiles mirror what a real editor leans on: filled and stroked geometry with
//! join/cap styles, gradients, dashes, layer blending, clipping, image filters,
//! images, and a runtime (SkSL) shader.

use std::cell::RefCell;

use skia_safe::{
    BlendMode, Canvas, Color, Color4f, Font, FontMgr, FontStyle, Image, Paint, PaintCap,
    PaintJoin, PaintStyle, PathBuilder, PathEffect, Point, RRect, Rect, RuntimeEffect,
    SamplingOptions, TileMode, gradient_shader, image_filters, images,
};

thread_local! {
    // Compiling SkSL and building the raster image are per-frame costs that have no
    // business being per-frame: the shader compile alone is tens of milliseconds, which
    // pushes a frame past the window in which the swapchain texture stays presentable.
    // Safari then shows nothing at all, which is how this was found.
    static EFFECT: RefCell<Option<Option<RuntimeEffect>>> = const { RefCell::new(None) };
    static IMAGE: RefCell<Option<Option<Image>>> = const { RefCell::new(None) };
}

/// Supply an image for the image tile, replacing the built-in raster one.
///
/// Graphite cannot upload a raster image while recording, so a Graphite caller
/// uploads once at startup and hands the texture-backed image over here.
pub fn set_image(image: Image) {
    IMAGE.with(|slot| *slot.borrow_mut() = Some(Some(image)));
}

pub const COLS: usize = 4;
pub const ROWS: usize = 3;
const PAD: f32 = 16.0;

/// The tiles, in drawing order. Exposed so a caller can render them one at a time:
/// when a backend silently drops draws, showing them individually is what tells you
/// which feature is unsupported.
pub const TILES: [fn(&Canvas, Rect, f32); 12] = [
    fill_and_stroke,
    joins_and_caps,
    linear_gradient,
    radial_and_sweep,
    dashed_path,
    bezier_path,
    rounded_rects,
    blend_modes,
    save_layer_and_clip,
    blur_filter,
    image_tile,
    runtime_shader,
];

pub const TILE_NAMES: [&str; 12] = [
    "fill+stroke",
    "joins+caps",
    "linear gradient",
    "radial+sweep",
    "dashed",
    "bezier",
    "rounded rects",
    "blend modes",
    "saveLayer+clip",
    "blur filter",
    "image",
    "runtime shader (SkSL)",
];

/// Draw a single tile filling the whole canvas.
pub fn draw_tile(canvas: &Canvas, index: usize, width: f32, height: f32, t: f32) {
    canvas.clear(Color::from_rgb(0x1e, 0x1e, 0x24));
    let rect = Rect::from_xywh(PAD, PAD, width - PAD * 2.0, height - PAD * 2.0);
    frame(canvas, rect);
    TILES[index % TILES.len()](canvas, rect.with_inset((20.0, 20.0)), t);
}

/// Draw the whole scene, scaled to fit `width` x `height`.
///
/// `t` is seconds since start; a few tiles animate so a still frame cannot hide a
/// backend that only draws once.
pub fn draw(canvas: &Canvas, width: f32, height: f32, t: f32) {
    draw_n(canvas, width, height, t, TILES.len());
}

/// Draw only the first `count` tiles.
///
/// Bisecting by count is how you find a tile that a backend cannot cope with *in
/// combination* with the others -- drawing each one alone can succeed while the same
/// set in a single recording does not.
pub fn draw_n(canvas: &Canvas, width: f32, height: f32, t: f32, count: usize) {
    let indices: Vec<usize> = (0..count.min(TILES.len())).collect();
    draw_selected(canvas, width, height, t, &indices);
}

/// Draw an arbitrary subset, laid out as if the full grid were present.
///
/// Bisecting by *which* tiles, not just how many, is what separates "this feature
/// fails after any other content" from "it fails only after a particular one".
pub fn draw_selected(canvas: &Canvas, width: f32, height: f32, t: f32, indices: &[usize]) {
    canvas.clear(Color::from_rgb(0x1e, 0x1e, 0x24));

    let tile_w = (width - PAD * (COLS as f32 + 1.0)) / COLS as f32;
    let tile_h = (height - PAD * (ROWS as f32 + 1.0)) / ROWS as f32;

    for &i in indices {
        let tile = &TILES[i % TILES.len()];
        let col = (i % COLS) as f32;
        let row = (i / COLS) as f32;
        let rect = Rect::from_xywh(
            PAD + col * (tile_w + PAD),
            PAD + row * (tile_h + PAD),
            tile_w,
            tile_h,
        );

        let count = canvas.save();
        canvas.clip_rect(rect, None, true);
        frame(canvas, rect);
        tile(canvas, rect.with_inset((10.0, 10.0)), t);
        canvas.restore_to_count(count);
    }
}

/// A faint outline so an empty tile is obvious rather than invisible.
fn frame(canvas: &Canvas, rect: Rect) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    paint.set_color(Color::from_argb(0x40, 0xff, 0xff, 0xff));
    canvas.draw_rrect(RRect::new_rect_xy(rect, 8.0, 8.0), &paint);
}

fn fill_and_stroke(canvas: &Canvas, r: Rect, _t: f32) {
    let mut fill = Paint::default();
    fill.set_anti_alias(true);
    fill.set_color(Color::from_rgb(0x4f, 0x9d, 0xff));
    canvas.draw_rect(Rect::from_xywh(r.left, r.top, r.width() * 0.45, r.height()), &fill);

    let mut stroke = Paint::default();
    stroke.set_anti_alias(true);
    stroke.set_style(PaintStyle::Stroke);
    stroke.set_stroke_width(6.0);
    stroke.set_color(Color::from_rgb(0xff, 0xb3, 0x4f));
    canvas.draw_rect(
        Rect::from_xywh(r.left + r.width() * 0.55, r.top + 3.0, r.width() * 0.45 - 3.0, r.height() - 6.0),
        &stroke,
    );
}

fn joins_and_caps(canvas: &Canvas, r: Rect, _t: f32) {
    let joins = [PaintJoin::Miter, PaintJoin::Round, PaintJoin::Bevel];
    let caps = [PaintCap::Butt, PaintCap::Round, PaintCap::Square];
    let step = r.height() / 3.0;

    for (i, (join, cap)) in joins.iter().zip(caps.iter()).enumerate() {
        let y = r.top + step * (i as f32 + 0.5);
        let mut builder = PathBuilder::new();
        builder.move_to((r.left + 6.0, y + step * 0.25));
        builder.line_to((r.center_x(), y - step * 0.3));
        builder.line_to((r.right - 6.0, y + step * 0.25));
        let path = builder.detach();

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(9.0);
        paint.set_stroke_join(*join);
        paint.set_stroke_cap(*cap);
        paint.set_color(Color::from_rgb(0x7a, 0xe5, 0x82));
        canvas.draw_path(&path, &paint);
    }
}

fn linear_gradient(canvas: &Canvas, r: Rect, t: f32) {
    let shift = (t * 0.35).sin() * 0.25;
    let shader = gradient_shader::linear(
        (Point::new(r.left, r.top), Point::new(r.right, r.bottom)),
        [
            Color4f::new(0.31, 0.62, 1.0, 1.0),
            Color4f::new(0.85, 0.35, 0.85, 1.0),
            Color4f::new(1.0, 0.70, 0.31, 1.0),
        ]
        .as_ref(),
        Some([0.0, (0.5 + shift).clamp(0.05, 0.95), 1.0].as_ref()),
        TileMode::Clamp,
        None,
        None,
    );

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_shader(shader);
    canvas.draw_rrect(RRect::new_rect_xy(r, 10.0, 10.0), &paint);
}

fn radial_and_sweep(canvas: &Canvas, r: Rect, t: f32) {
    let half = Rect::from_xywh(r.left, r.top, r.width() * 0.5 - 4.0, r.height());
    let radial = gradient_shader::radial(
        Point::new(half.center_x(), half.center_y()),
        half.width().min(half.height()) * 0.55,
        [Color4f::new(1.0, 1.0, 1.0, 1.0), Color4f::new(0.2, 0.4, 0.9, 1.0)].as_ref(),
        None,
        TileMode::Clamp,
        None,
        None,
    );
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_shader(radial);
    canvas.draw_oval(half, &paint);

    let half2 = Rect::from_xywh(r.center_x() + 4.0, r.top, r.width() * 0.5 - 4.0, r.height());
    let sweep = gradient_shader::sweep(
        Point::new(half2.center_x(), half2.center_y()),
        [
            Color4f::new(1.0, 0.3, 0.3, 1.0),
            Color4f::new(0.3, 1.0, 0.5, 1.0),
            Color4f::new(0.3, 0.5, 1.0, 1.0),
            Color4f::new(1.0, 0.3, 0.3, 1.0),
        ]
        .as_ref(),
        None,
        TileMode::Clamp,
        Some((t.to_degrees() * 0.15, t.to_degrees() * 0.15 + 360.0)),
        None,
        None,
    );
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_shader(sweep);
    canvas.draw_oval(half2, &paint);
}

fn dashed_path(canvas: &Canvas, r: Rect, t: f32) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(4.0);
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_color(Color::from_rgb(0xff, 0xd9, 0x66));
    paint.set_path_effect(PathEffect::dash(&[14.0, 8.0], t * 24.0 % 22.0));

    let inset = r.with_inset((4.0, 4.0));
    canvas.draw_rrect(RRect::new_rect_xy(inset, 14.0, 14.0), &paint);
    canvas.draw_line(
        (inset.left, inset.center_y()),
        (inset.right, inset.center_y()),
        &paint,
    );
}

fn bezier_path(canvas: &Canvas, r: Rect, t: f32) {
    let wobble = (t * 1.2).sin() * r.height() * 0.18;
    let mut builder = PathBuilder::new();
    builder.move_to((r.left, r.center_y()));
    builder.cubic_to(
        (r.left + r.width() * 0.3, r.top + wobble),
        (r.right - r.width() * 0.3, r.bottom - wobble),
        (r.right, r.center_y()),
    );
    let path = builder.snapshot();

    let mut stroke = Paint::default();
    stroke.set_anti_alias(true);
    stroke.set_style(PaintStyle::Stroke);
    stroke.set_stroke_width(5.0);
    stroke.set_stroke_cap(PaintCap::Round);
    stroke.set_color(Color::from_rgb(0x9d, 0x8c, 0xff));
    canvas.draw_path(&path, &stroke);

    // Same path filled, to exercise the fill rasterizer on a curve.
    builder.line_to((r.right, r.bottom));
    builder.line_to((r.left, r.bottom));
    builder.close();
    let path = builder.detach();
    let mut fill = Paint::default();
    fill.set_anti_alias(true);
    fill.set_color(Color::from_argb(0x40, 0x9d, 0x8c, 0xff));
    canvas.draw_path(&path, &fill);
}

fn rounded_rects(canvas: &Canvas, r: Rect, _t: f32) {
    let step = r.height() / 3.0;
    for i in 0..3 {
        let radius = 2.0 + i as f32 * 12.0;
        let rect = Rect::from_xywh(r.left, r.top + step * i as f32 + 2.0, r.width(), step - 6.0);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::from_rgb(0x35, 0xc4, 0xb4 - (i as u8) * 0x20));
        canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
    }
}

/// The blend modes the blend tile shows, in order.
///
/// The last three cannot be expressed by fixed-function blending -- they need the
/// destination read back -- which is exactly the distinction under investigation.
pub const BLEND_MODES: [BlendMode; 6] = [
    BlendMode::SrcOver,
    BlendMode::Plus,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Darken,
    BlendMode::SoftLight,
];

thread_local! {
    static BLEND_FILTER: RefCell<Option<usize>> = const { RefCell::new(None) };
}

/// Restrict the blend tile to a single mode, by index into [`BLEND_MODES`].
pub fn set_blend_filter(index: Option<usize>) {
    BLEND_FILTER.with(|f| *f.borrow_mut() = index);
}

fn blend_modes(canvas: &Canvas, r: Rect, _t: f32) {
    let filter = BLEND_FILTER.with(|f| *f.borrow());
    let modes: Vec<BlendMode> = match filter {
        Some(i) => vec![BLEND_MODES[i % BLEND_MODES.len()]],
        None => BLEND_MODES.to_vec(),
    };
    let cell_w = r.width() / 3.0;
    let cell_h = r.height() / 2.0;

    for (i, mode) in modes.iter().enumerate() {
        let x = r.left + (i % 3) as f32 * cell_w;
        let y = r.top + (i / 3) as f32 * cell_h;

        let mut base = Paint::default();
        base.set_anti_alias(true);
        base.set_color(Color::from_rgb(0x30, 0x60, 0xc0));
        canvas.draw_oval(Rect::from_xywh(x + 2.0, y + 2.0, cell_w * 0.7, cell_h * 0.7), &base);

        let mut over = Paint::default();
        over.set_anti_alias(true);
        over.set_color(Color::from_rgb(0xe0, 0x70, 0x30));
        over.set_blend_mode(*mode);
        canvas.draw_oval(
            Rect::from_xywh(x + cell_w * 0.25, y + cell_h * 0.25, cell_w * 0.7, cell_h * 0.7),
            &over,
        );
    }
}

fn save_layer_and_clip(canvas: &Canvas, r: Rect, t: f32) {
    let mut clip = PathBuilder::new();
    let cx = r.center_x();
    let cy = r.center_y();
    let radius = r.width().min(r.height()) * 0.45;
    for i in 0..5 {
        let angle = t * 0.4 + i as f32 * std::f32::consts::TAU / 5.0;
        let p = (cx + angle.cos() * radius, cy + angle.sin() * radius);
        if i == 0 {
            clip.move_to(p);
        } else {
            clip.line_to(p);
        }
    }
    clip.close();
    let clip = clip.detach();

    let count = canvas.save();
    canvas.clip_path(&clip, None, true);

    // A layer with reduced alpha, so the clip and the layer compose visibly.
    let mut layer_paint = Paint::default();
    layer_paint.set_alpha(0xb0);
    canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&layer_paint));

    let mut a = Paint::default();
    a.set_anti_alias(true);
    a.set_color(Color::from_rgb(0xff, 0x6b, 0x6b));
    canvas.draw_rect(Rect::from_xywh(r.left, r.top, r.width(), r.height() * 0.6), &a);

    let mut b = Paint::default();
    b.set_anti_alias(true);
    b.set_color(Color::from_rgb(0x4e, 0xcd, 0xc4));
    canvas.draw_oval(r.with_inset((r.width() * 0.15, r.height() * 0.15)), &b);

    canvas.restore();
    canvas.restore_to_count(count);
}

fn blur_filter(canvas: &Canvas, r: Rect, t: f32) {
    let sigma = 3.0 + (t * 0.8).sin().abs() * 6.0;
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from_rgb(0xff, 0x9f, 0x1c));
    paint.set_image_filter(image_filters::blur((sigma, sigma), None, None, None));
    canvas.draw_rrect(
        RRect::new_rect_xy(r.with_inset((r.width() * 0.2, r.height() * 0.2)), 12.0, 12.0),
        &paint,
    );

    let mut sharp = Paint::default();
    sharp.set_anti_alias(true);
    sharp.set_style(PaintStyle::Stroke);
    sharp.set_stroke_width(2.0);
    sharp.set_color(Color::from_argb(0x80, 0xff, 0xff, 0xff));
    canvas.draw_rrect(RRect::new_rect_xy(r.with_inset((r.width() * 0.2, r.height() * 0.2)), 12.0, 12.0), &sharp);
}

fn image_tile(canvas: &Canvas, r: Rect, t: f32) {
    let cached = IMAGE.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(checker_image)
            .clone()
    });
    if let Some(image) = cached {
        let angle = t * 12.0;
        let count = canvas.save();
        canvas.translate((r.center_x(), r.center_y()));
        canvas.rotate(angle, None);
        canvas.translate((-r.width() * 0.35, -r.height() * 0.35));
        canvas.draw_image_rect_with_sampling_options(
            &image,
            None,
            Rect::from_xywh(0.0, 0.0, r.width() * 0.7, r.height() * 0.7),
            SamplingOptions::default(),
            &Paint::default(),
        );
        canvas.restore_to_count(count);
    }
}

/// A small procedurally generated image — avoids shipping an asset with the example.
fn checker_image() -> Option<Image> {
    const SIZE: usize = 64;
    let mut pixels = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let on = ((x / 8) + (y / 8)) % 2 == 0;
            let i = (y * SIZE + x) * 4;
            let (r, g, b) = if on { (0xf2, 0xf2, 0xf7) } else { (0x53, 0x6d, 0xfe) };
            pixels[i] = r;
            pixels[i + 1] = g;
            pixels[i + 2] = b;
            pixels[i + 3] = 0xff;
        }
    }

    let info = skia_safe::ImageInfo::new(
        (SIZE as i32, SIZE as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let data = skia_safe::Data::new_copy(&pixels);
    images::raster_from_data(&info, data, SIZE * 4)
}

fn runtime_shader(canvas: &Canvas, r: Rect, t: f32) {
    const SKSL: &str = "
        uniform float2 uSize;
        uniform float  uTime;

        half4 main(float2 p) {
            float2 uv = p / uSize;
            float wave = 0.5 + 0.5 * sin(uv.x * 12.0 + uTime * 2.0)
                                  * cos(uv.y * 9.0 - uTime * 1.3);
            return half4(half(wave * 0.35), half(0.45 + wave * 0.4), half(0.85), 1.0);
        }
    ";

    let cached = EFFECT.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| RuntimeEffect::make_for_shader(SKSL, None).ok())
            .clone()
    });
    let Some(effect) = cached else {
        return;
    };

    let mut uniforms = Vec::new();
    uniforms.extend_from_slice(&r.width().to_ne_bytes());
    uniforms.extend_from_slice(&r.height().to_ne_bytes());
    uniforms.extend_from_slice(&t.to_ne_bytes());

    let Some(shader) = effect.make_shader(skia_safe::Data::new_copy(&uniforms), &[], None) else {
        return;
    };

    let count = canvas.save();
    canvas.translate((r.left, r.top));
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_shader(shader);
    canvas.draw_rrect(
        RRect::new_rect_xy(Rect::from_wh(r.width(), r.height()), 10.0, 10.0),
        &paint,
    );
    canvas.restore_to_count(count);

    // Text needs a typeface, and wasm builds of rust-skia use Skia's *empty* font
    // manager — there are no system fonts to fall back on. Draw a label only if the
    // embedding page supplied one; otherwise this tile is shader-only.
    let font_mgr = FontMgr::new();
    if let Some(typeface) = font_mgr.match_family_style("", FontStyle::default()) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::WHITE);
        canvas.draw_str("SkSL", (r.left + 8.0, r.bottom - 8.0), &Font::from_typeface(typeface, 16.0), &paint);
    }
}
