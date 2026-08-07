//! Graphite/Dawn (WebGPU) example.
//!
//! JavaScript owns device creation, because `requestDevice` is asynchronous and wasm
//! is not. Everything after that lives here: the surface is created from the canvas
//! selector on this side, since emdawnwebgpu cannot import a JS texture.
//!
//! The drawing itself is shared verbatim with the WebGL example — that is the point of
//! having both.

use std::{ffi::CString, os::raw::c_char};

use skia_safe::{
    AlphaType, Canvas, Color, ColorType, ImageInfo, Paint, Rect, Surface,
    graphite::Mipmapped,
    graphite::{self, Context, InsertRecordingInfo, Recorder, dawn},
};

#[path = "../../wasm-scene/scene.rs"]
mod scene;

/// Opaque WebGPU handles, produced by the C++ shim next door.
type WgpuHandle = *mut std::ffi::c_void;

unsafe extern "C" {
    fn demo_wgpu_create_instance() -> WgpuHandle;
    fn demo_wgpu_device() -> WgpuHandle;
    fn demo_wgpu_queue(device: WgpuHandle) -> WgpuHandle;
    fn demo_wgpu_create_surface(instance: WgpuHandle, selector: *const c_char) -> WgpuHandle;
    fn demo_wgpu_configure(surface: WgpuHandle, device: WgpuHandle, width: i32, height: i32);
    fn demo_wgpu_current_texture(surface: WgpuHandle) -> WgpuHandle;
    fn demo_wgpu_release_texture(texture: WgpuHandle);
}

pub struct State {
    context: Context,
    /// One recorder for the lifetime of the app, not one per frame: a Recorder owns
    /// the resource and pipeline caches, so recreating it every frame throws them
    /// away and re-does the work each time.
    recorder: Recorder,
    /// Scene drawing goes here, not straight to the swapchain texture.
    ///
    /// Graphite splits a frame with saveLayer into several render passes, and Safari
    /// drops the whole frame when more than one of them touches the texture from
    /// getCurrentTexture(). Drawing offscreen and blitting once keeps the swapchain
    /// texture down to a single pass, whatever Skia does internally.
    offscreen: Surface,
    /// Frames repeat 60 times a second; a failure that prints every frame buries
    /// everything else in the console.
    reported: bool,
    surface: WgpuHandle,
    device: WgpuHandle,
    width: i32,
    height: i32,
}

/// Set up Skia on top of the device JavaScript already created.
///
/// Returns null if anything failed; the console output says what.
#[unsafe(no_mangle)]
pub extern "C" fn init(width: i32, height: i32) -> *mut State {
    let device = unsafe { demo_wgpu_device() };
    if device.is_null() {
        eprintln!("no WebGPU device — did JS set Module.preinitializedWebGPUDevice?");
        return std::ptr::null_mut();
    }

    let instance = unsafe { demo_wgpu_create_instance() };
    let queue = unsafe { demo_wgpu_queue(device) };

    let selector = CString::new("#canvas").unwrap();
    let surface = unsafe { demo_wgpu_create_surface(instance, selector.as_ptr()) };
    if surface.is_null() {
        eprintln!("could not create a WebGPU surface for '#canvas'");
        return std::ptr::null_mut();
    }
    unsafe { demo_wgpu_configure(surface, device, width, height) };

    // SAFETY: all three handles come from the shim above and belong to one device.
    let backend_context = unsafe { dawn::BackendContext::new(instance, device, queue) };
    let Some(mut context) = dawn::make_context(&backend_context, None) else {
        eprintln!("Graphite rejected the WebGPU backend context");
        return std::ptr::null_mut();
    };

    let Some(recorder) = context.make_recorder(None) else {
        eprintln!("could not create a Graphite recorder");
        return std::ptr::null_mut();
    };

    let mut recorder = recorder;
    let image_info = ImageInfo::new(
        (width, height),
        ColorType::BGRA8888,
        AlphaType::Premul,
        None,
    );
    let Some(offscreen) = graphite::surfaces::render_target(
        &mut recorder,
        &image_info,
        Mipmapped::No,
        None,
        Some("scene"),
    ) else {
        eprintln!("could not create the offscreen surface");
        return std::ptr::null_mut();
    };

    // Upload the scene's raster image once, here.
    //
    // Graphite draws from textures only, and it will not upload one while a recording is
    // being built: a raster SkImage reaching the canvas fails with "Couldn't convert
    // SkImage to a Graphite-backed representation" and the tile silently stays empty.
    // The upload is recorded into this same recorder, so it is submitted with the first
    // frame, before any draw that samples it.
    match scene::checker_image() {
        Some(raster) => match graphite::images::texture_from_image(&mut recorder, &raster) {
            Some(texture) => scene::set_image(texture),
            None => eprintln!("[example] could not upload the scene image to a texture"),
        },
        None => eprintln!("[example] could not build the scene's raster image"),
    }

    Box::into_raw(Box::new(State {
        context,
        recorder,
        offscreen,
        reported: false,
        surface,
        device,
        width,
        height,
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn resize(state: *mut State, width: i32, height: i32) {
    let state = unsafe { &mut *state };
    state.width = width;
    state.height = height;
    unsafe { demo_wgpu_configure(state.surface, state.device, width, height) };
}

/// Draw one frame. `time` is seconds since start.
///
/// `mode` 0 draws the full scene; mode 1 draws a single filled rectangle. The
/// minimal mode separates two very different failures: if even one rectangle is
/// dropped the problem is in context or surface setup, and if it draws while the
/// scene does not, some specific drawing feature is unsupported.
/// Restrict the blend tile to one mode; -1 restores all of them.
#[unsafe(no_mangle)]
pub extern "C" fn set_blend_filter(index: i32) {
    scene::set_blend_filter(if index < 0 { None } else { Some(index as usize) });
}

/// Draw a chosen subset: `mask` bit N selects tile N.
#[unsafe(no_mangle)]
pub extern "C" fn render_mask(state: *mut State, time: f32, mask: u32) {
    let s = unsafe { &mut *state };
    let (w, h) = (s.width as f32, s.height as f32);
    let indices: Vec<usize> = (0..12).filter(|i| mask & (1 << i) != 0).collect();
    with_frame(s, |canvas| {
        scene::draw_selected(canvas, w, h, time, &indices)
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn render_mode(state: *mut State, time: f32, mode: i32) {
    if mode == 1 {
        render_minimal(state);
    } else if mode >= 100 {
        let n = (mode - 100) as usize;
        let s = unsafe { &mut *state };
        let (w, h) = (s.width as f32, s.height as f32);
        with_frame(s, |canvas| scene::draw_n(canvas, w, h, time, n));
    } else if mode >= 10 {
        render_single_tile(state, time, (mode - 10) as usize);
    } else {
        render(state, time);
    }
}

fn render_single_tile(state: *mut State, time: f32, index: usize) {
    let s = unsafe { &mut *state };
    let (w, h) = (s.width as f32, s.height as f32);
    with_frame(s, |canvas| scene::draw_tile(canvas, index, w, h, time));
}

/// Everything a frame needs around the actual drawing: this frame's swapchain
/// texture, a recorder, the Skia surface wrapping it, and the submit afterwards.
fn with_frame(state: &mut State, draw: impl FnOnce(&Canvas)) {
    // The swapchain texture is only valid for this frame.
    let texture = unsafe { demo_wgpu_current_texture(state.surface) };
    if texture.is_null() {
        return;
    }

    let Some(mut surface) = wrap(&mut state.recorder, texture) else {
        report_once(state, "wrap_backend_texture returned None");
        unsafe { demo_wgpu_release_texture(texture) };
        return;
    };

    draw(state.offscreen.canvas());

    // The only draw that touches the swapchain texture.
    match graphite::surfaces::as_image(&state.offscreen) {
        Some(image) => {
            surface
                .canvas()
                .draw_image(&image, (0.0, 0.0), Some(&Paint::default()));
        }
        None => report_once(state, "surfaces::as_image returned None"),
    }

    // Every one of these can fail quietly and leave a blank canvas, so none of them
    // is ignored: a dropped frame should say which step dropped it.
    match state.recorder.snap() {
        Some(mut recording) => {
            let info = InsertRecordingInfo::new(&mut recording);
            let status = state.context.insert_recording(&info);
            if status != graphite::InsertStatus::Success {
                report_once(state, &format!("insert_recording: {status:?}"));
            }
            if !state.context.submit(None) {
                report_once(state, "submit returned false");
            }
        }
        None => report_once(state, "recorder.snap() returned None"),
    }

    unsafe { demo_wgpu_release_texture(texture) };
}

fn render_minimal(state: *mut State) {
    let s = unsafe { &mut *state };
    let (w, h) = (s.width as f32, s.height as f32);
    with_frame(s, |canvas| {
        canvas.clear(Color::from_rgb(0x20, 0x20, 0x28));
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::from_rgb(0x4f, 0x9d, 0xff));
        canvas.draw_rect(Rect::from_xywh(w * 0.25, h * 0.25, w * 0.5, h * 0.5), &paint);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn render(state: *mut State, time: f32) {
    let s = unsafe { &mut *state };
    let (w, h) = (s.width as f32, s.height as f32);
    with_frame(s, |canvas| scene::draw(canvas, w, h, time));
}

fn report_once(state: &mut State, what: &str) {
    if !state.reported {
        state.reported = true;
        eprintln!("[example] frame dropped: {what}");
    }
}

fn wrap(recorder: &mut Recorder, texture: WgpuHandle) -> Option<Surface> {
    // SAFETY: `texture` is this frame's swapchain texture from the shim.
    let backend_texture = unsafe { dawn::backend_texture(texture) };
    // BGRA8Unorm is what the shim configures the surface with.
    graphite::surfaces::wrap_backend_texture(recorder, &backend_texture, ColorType::BGRA8888, None, None)
}

fn main() {
    // Nothing to do: JavaScript drives init/render.
}
