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
    ColorType, Surface,
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
    let Some(context) = dawn::make_context(&backend_context, None) else {
        eprintln!("Graphite rejected the WebGPU backend context");
        return std::ptr::null_mut();
    };

    Box::into_raw(Box::new(State {
        context,
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
#[unsafe(no_mangle)]
pub extern "C" fn render(state: *mut State, time: f32) {
    let state = unsafe { &mut *state };

    // The swapchain texture is only valid for this frame.
    let texture = unsafe { demo_wgpu_current_texture(state.surface) };
    if texture.is_null() {
        return;
    }

    let Some(mut recorder) = state.context.make_recorder(None) else {
        unsafe { demo_wgpu_release_texture(texture) };
        return;
    };

    if let Some(mut surface) = wrap(&mut recorder, texture) {
        scene::draw(
            surface.canvas(),
            state.width as f32,
            state.height as f32,
            time,
        );

        if let Some(mut recording) = recorder.snap() {
            let info = InsertRecordingInfo::new(&mut recording);
            state.context.insert_recording(&info);
            state.context.submit(None);
        }
    }

    unsafe { demo_wgpu_release_texture(texture) };
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
