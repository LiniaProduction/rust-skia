//! Graphite/Dawn in a Web Worker, built the way the Linia renderer is built: pthreads,
//! shared memory, an OffscreenCanvas per app and no requestAnimationFrame.
//!
//! JavaScript owns the WebGPU objects. It creates the device, configures one canvas
//! context per app, uploads images with `copyExternalImageToTexture`, and passes all of
//! them in through emdawnwebgpu's `WebGPU.importJs*`. Skia gets the handles on input and
//! never creates a device or a surface of its own, so other WebGPU code can share both.

use std::cell::RefCell;
use std::time::Instant;

use skia_safe::{
    AlphaType, Color, ColorType, Data, ISize, Image, ImageInfo, Paint, Rect, Surface,
    graphite::{self, Context, InsertRecordingInfo, Mipmapped, Recorder, dawn},
};

#[path = "../../wasm-scene/scene.rs"]
mod scene;

type Handle = *mut std::ffi::c_void;

unsafe extern "C" {
    fn spike_create_instance() -> Handle;
    fn spike_device_queue(device: Handle) -> Handle;
    fn spike_current_texture(surface: Handle) -> Handle;
    fn spike_texture_release(texture: Handle);
    fn spike_instance_process_events(instance: Handle);
}

struct Gpu {
    instance: Handle,
    context: Context,
    images: Vec<Image>,
}

pub struct App {
    id: i32,
    recorder: Recorder,
    surface: Handle,
    offscreen: Option<Surface>,
    width: i32,
    height: i32,
    frames: u64,
    reported: bool,
}

static PTHREAD_SUM: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn pthread_sum() -> f64 {
    PTHREAD_SUM.load(std::sync::atomic::Ordering::SeqCst) as f64
}

thread_local! {
    static GPU: RefCell<Option<Gpu>> = const { RefCell::new(None) };
}

fn with_gpu<R>(f: impl FnOnce(&mut Gpu) -> R) -> Option<R> {
    GPU.with(|g| g.borrow_mut().as_mut().map(f))
}

#[unsafe(no_mangle)]
pub extern "C" fn gpu_init(device: Handle) -> i32 {
    if device.is_null() {
        eprintln!("[spike] gpu_init: null device");
        return -1;
    }
    let instance = unsafe { spike_create_instance() };
    let queue = unsafe { spike_device_queue(device) };
    let backend = unsafe { dawn::BackendContext::new(instance, device, queue) };
    let Some(context) = dawn::make_context(&backend, None) else {
        eprintln!("[spike] make_context failed");
        return -2;
    };
    GPU.with(|g| {
        *g.borrow_mut() = Some(Gpu {
            instance,
            context,
            images: Vec::new(),
        })
    });

    std::thread::spawn(|| {
        let sum: u64 = (0..1_000_000u64).sum();
        PTHREAD_SUM.store(sum, std::sync::atomic::Ordering::SeqCst);
    });

    println!("[spike] gpu_init ok");
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn app_create(id: i32, surface: Handle, width: i32, height: i32, offscreen: i32) -> *mut App {
    let Some(recorder) = with_gpu(|g| g.context.make_recorder(None)).flatten() else {
        eprintln!("[spike] app {id}: make_recorder failed");
        return std::ptr::null_mut();
    };
    let mut app = App {
        id,
        recorder,
        surface,
        offscreen: None,
        width,
        height,
        frames: 0,
        reported: false,
    };
    if id == 0 {
        if let Some(texture) = scene::checker_image().and_then(|raster| graphite::images::texture_from_image(&mut app.recorder, &raster)) {
            scene::set_image(texture);
        }
    }
    if offscreen != 0 {
        app.offscreen = make_offscreen(&mut app.recorder, width, height);
    }
    println!("[spike] app {id} created {width}x{height} offscreen={}", app.offscreen.is_some());
    Box::into_raw(Box::new(app))
}

fn make_offscreen(recorder: &mut Recorder, width: i32, height: i32) -> Option<Surface> {
    let info = ImageInfo::new((width, height), ColorType::BGRA8888, AlphaType::Premul, None);
    graphite::surfaces::render_target(recorder, &info, Mipmapped::No, None, Some("offscreen"))
}

#[unsafe(no_mangle)]
pub extern "C" fn app_resize(app: *mut App, width: i32, height: i32) {
    let app = unsafe { &mut *app };
    app.width = width;
    app.height = height;
    if app.offscreen.is_some() {
        app.offscreen = make_offscreen(&mut app.recorder, width, height);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn app_destroy(app: *mut App) {
    drop(unsafe { Box::from_raw(app) });
}

/// Wrap a texture JavaScript filled with `copyExternalImageToTexture`.
/// Returns the image index, usable by every app.
#[unsafe(no_mangle)]
pub extern "C" fn image_wrap_texture(app: *mut App, texture: Handle, premul: i32) -> i32 {
    let app = unsafe { &mut *app };
    let backend = unsafe { dawn::backend_texture(texture) };
    let alpha = if premul != 0 { AlphaType::Premul } else { AlphaType::Unpremul };
    let Some(image) = graphite::images::wrap_texture(&mut app.recorder, &backend, ColorType::RGBA8888, alpha, None)
    else {
        eprintln!("[spike] wrap_texture failed");
        return -1;
    };
    push_image(image)
}

/// Upload RGBA pixels that live in wasm memory: the fallback path.
#[unsafe(no_mangle)]
pub extern "C" fn image_upload_pixels(app: *mut App, pixels: *const u8, width: i32, height: i32) -> i32 {
    let app = unsafe { &mut *app };
    let t0 = Instant::now();
    let len = (width * height * 4) as usize;
    let data = unsafe { Data::new_bytes(std::slice::from_raw_parts(pixels, len)) };
    let info = ImageInfo::new((width, height), ColorType::RGBA8888, AlphaType::Premul, None);
    let Some(raster) = skia_safe::images::raster_from_data(&info, data, (width * 4) as usize) else {
        return -1;
    };
    let t1 = Instant::now();
    let Some(image) = graphite::images::texture_from_image(&mut app.recorder, &raster) else {
        eprintln!("[spike] texture_from_image failed");
        return -2;
    };
    println!(
        "[spike] upload_pixels {width}x{height}: copy-in {:.2}ms texture_from_image {:.2}ms",
        (t1 - t0).as_secs_f64() * 1000.0,
        t1.elapsed().as_secs_f64() * 1000.0
    );
    push_image(image)
}

fn push_image(image: Image) -> i32 {
    with_gpu(|g| {
        g.images.push(image);
        (g.images.len() - 1) as i32
    })
    .unwrap_or(-1)
}

/// `mode` 0: scene; 1: one rectangle; 2: all imported images tiled.
/// `phase` 1 records only, 2 inserts and submits only, 3 both.
#[unsafe(no_mangle)]
pub extern "C" fn app_frame(app: *mut App, time: f32, mode: i32, phase: i32) -> i32 {
    let app = unsafe { &mut *app };
    with_gpu(|gpu| frame(gpu, app, time, mode, phase)).unwrap_or(-100)
}

thread_local! {
    static SHARED: RefCell<Option<Image>> = const { RefCell::new(None) };
    static PENDING: RefCell<Vec<(Handle, graphite::Recording)>> = const { RefCell::new(Vec::new()) };
}

fn frame(gpu: &mut Gpu, app: &mut App, time: f32, mode: i32, phase: i32) -> i32 {
    if phase & 1 != 0 {
        let texture = unsafe { spike_current_texture(app.surface) };
        if texture.is_null() {
            return -1;
        }
        let backend = unsafe { dawn::backend_texture(texture) };
        let Some(mut target) =
            graphite::surfaces::wrap_backend_texture(&mut app.recorder, &backend, ColorType::BGRA8888, None, None)
        else {
            report(app, "wrap_backend_texture failed");
            unsafe { spike_texture_release(texture) };
            return -2;
        };
        let (w, h) = (app.width as f32, app.height as f32);
        let id = app.id;
        let images = &gpu.images;
        if mode == 3 {
            let shared = SHARED.with(|s| s.borrow().clone());
            target.canvas().clear(Color::DARK_GRAY);
            if let Some(image) = shared {
                target.canvas().draw_image_rect(&image, None, Rect::from_xywh(0.0, 0.0, w, h), &Paint::default());
            }
        } else {
        match app.offscreen.as_mut() {
            Some(off) => {
                draw(off, mode, w, h, time, id, images);
                if let Some(image) = graphite::surfaces::as_image(off) {
                    if id == 0 {
                        SHARED.with(|s| *s.borrow_mut() = Some(image.clone()));
                    }
                    target.canvas().draw_image(&image, (0, 0), Some(&Paint::default()));
                }
            }
            None => draw(&mut target, mode, w, h, time, id, images),
        }
        }
        drop(target);
        let Some(recording) = app.recorder.snap() else {
            report(app, "snap failed");
            unsafe { spike_texture_release(texture) };
            return -3;
        };
        PENDING.with(|p| p.borrow_mut().push((texture, recording)));
    }
    if phase & 2 != 0 {
        let pending = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
        for (texture, mut recording) in pending {
            let status = gpu.context.insert_recording(&InsertRecordingInfo::new(&mut recording));
            if status != graphite::InsertStatus::Success {
                report(app, &format!("insert_recording {status:?}"));
            }
            unsafe { spike_texture_release(texture) };
        }
        if !gpu.context.submit(None) {
            report(app, "submit false");
        }
        unsafe { spike_instance_process_events(gpu.instance) };
        gpu.context.check_async_work_completion();
    }
    app.frames += 1;
    0
}

fn draw(surface: &mut Surface, mode: i32, w: f32, h: f32, time: f32, id: i32, images: &[Image]) {
    let canvas = surface.canvas();
    match mode {
        1 => {
            canvas.clear(Color::from_rgb(0x20, 0x20, 0x28));
            let mut paint = Paint::default();
            paint.set_color(if id % 2 == 0 { Color::RED } else { Color::BLUE });
            canvas.draw_rect(Rect::from_xywh(w * 0.25, h * 0.25, w * 0.5, h * 0.5), &paint);
        }
        2 => {
            canvas.clear(Color::from_rgb(0x20, 0x20, 0x28));
            let n = images.len().max(1) as f32;
            let cell = w / n;
            for (i, image) in images.iter().enumerate() {
                let ISize { width, height } = image.dimensions();
                let scale = (cell / width as f32).min(h / height as f32);
                let dst = Rect::from_xywh(i as f32 * cell, 0.0, width as f32 * scale, height as f32 * scale);
                canvas.draw_image_rect(image, None, dst, &Paint::default());
            }
        }
        _ => scene::draw(canvas, w, h, time),
    }
}

fn budget_line(gpu: &Gpu, app: &App, what: &str) {
    let mb = |b: usize| b as f64 / 1048576.0;
    println!(
        "[budget] {what}: recorder used={:.1}MB purgeable={:.1}MB max={:.1}MB | context used={:.1}MB purgeable={:.1}MB max={:.1}MB",
        mb(app.recorder.current_budgeted_bytes()),
        mb(app.recorder.current_purgeable_bytes()),
        mb(app.recorder.max_budgeted_bytes()),
        mb(gpu.context.current_budgeted_bytes()),
        mb(gpu.context.current_purgeable_bytes()),
        mb(gpu.context.max_budgeted_bytes()),
    );
}

thread_local! {
    static PROBE: RefCell<(Vec<Surface>, Vec<Image>)> = const { RefCell::new((Vec::new(), Vec::new())) };
}

/// One step of the budget probe; JavaScript awaits the GPU between steps.
/// 0: set budget; 1: allocate+draw+submit; 2: snapshot images, drop surfaces; 3: drop images;
/// 4: pump async work; 5: deferred cleanup(0); 6: free_gpu_resources; 7: report only.
#[unsafe(no_mangle)]
pub extern "C" fn budget_step(app: *mut App, step: i32, count: i32, size: i32, budget_mb: i32) {
    let app = unsafe { &mut *app };
    with_gpu(|gpu| {
        let what = PROBE.with(|p| {
            let (surfaces, images) = &mut *p.borrow_mut();
            match step {
                0 => {
                    if budget_mb > 0 {
                        app.recorder.set_max_budgeted_bytes(budget_mb as usize * 1048576);
                    }
                    "budget set".to_string()
                }
                1 => {
                    let mut failed = 0;
                    for i in 0..count {
                        match make_offscreen(&mut app.recorder, size, size) {
                            Some(mut s) => {
                                s.canvas().clear(Color::from_rgb((i * 37) as u8, 90, 160));
                                surfaces.push(s);
                            }
                            None => failed += 1,
                        }
                    }
                    if let Some(mut recording) = app.recorder.snap() {
                        gpu.context.insert_recording(&InsertRecordingInfo::new(&mut recording));
                    }
                    gpu.context.submit(None);
                    format!("allocated+submitted {} failed {failed}", surfaces.len())
                }
                2 => {
                    images.extend(surfaces.iter().filter_map(graphite::surfaces::as_image));
                    surfaces.clear();
                    format!("surfaces dropped, {} snapshots", images.len())
                }
                3 => {
                    images.clear();
                    "snapshots dropped".to_string()
                }
                4 => {
                    unsafe { spike_instance_process_events(gpu.instance) };
                    gpu.context.check_async_work_completion();
                    "async pumped".to_string()
                }
                5 => {
                    app.recorder.perform_deferred_cleanup(std::time::Duration::ZERO, None);
                    gpu.context.perform_deferred_cleanup(std::time::Duration::ZERO, None);
                    "deferred cleanup(0)".to_string()
                }
                6 => {
                    app.recorder.free_gpu_resources();
                    gpu.context.free_gpu_resources();
                    "free_gpu_resources".to_string()
                }
                _ => "report".to_string(),
            }
        });
        budget_line(gpu, app, &what);
    });
}

fn report(app: &mut App, what: &str) {
    if !app.reported {
        app.reported = true;
        eprintln!("[spike] app {} frame {}: {what}", app.id, app.frames);
    }
}

fn main() {}
