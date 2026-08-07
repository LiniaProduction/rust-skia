//! Ganesh/WebGL example — the reference half of the pair.
//!
//! Draws exactly the same scene as the WebGPU example, so any visual difference is
//! the backend and not the drawing code.

use skia_safe::{
    Surface,
    gpu::{self, DirectContext, gl::FramebufferInfo},
};

#[path = "../../wasm-scene/scene.rs"]
mod scene;

unsafe extern "C" {
    fn emscripten_GetProcAddress(name: *const std::os::raw::c_char) -> *const std::ffi::c_void;
}

pub struct State {
    context: DirectContext,
    framebuffer_info: FramebufferInfo,
    surface: Surface,
    width: i32,
    height: i32,
}

fn create_surface(
    context: &mut DirectContext,
    framebuffer_info: FramebufferInfo,
    width: i32,
    height: i32,
) -> Surface {
    let target = gpu::backend_render_targets::make_gl((width, height), 1, 8, framebuffer_info);
    gpu::surfaces::wrap_backend_render_target(
        context,
        &target,
        gpu::SurfaceOrigin::BottomLeft,
        skia_safe::ColorType::RGBA8888,
        None,
        None,
    )
    .expect("could not wrap the WebGL framebuffer")
}

#[unsafe(no_mangle)]
pub extern "C" fn init(width: i32, height: i32) -> *mut State {
    unsafe {
        gl::load_with(|addr| {
            let addr = std::ffi::CString::new(addr).unwrap();
            emscripten_GetProcAddress(addr.as_ptr()) as *const _
        });
    }

    let Some(interface) = gpu::gl::Interface::new_native() else {
        eprintln!("no WebGL interface — was the context created before init()?");
        return std::ptr::null_mut();
    };
    let Some(mut context) = gpu::direct_contexts::make_gl(interface, None) else {
        eprintln!("Ganesh rejected the WebGL interface");
        return std::ptr::null_mut();
    };

    let framebuffer_info = {
        let mut fboid: gl::types::GLint = 0;
        unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };
        FramebufferInfo {
            fboid: fboid.try_into().unwrap(),
            format: gpu::gl::Format::RGBA8.into(),
            protected: gpu::Protected::No,
        }
    };

    let surface = create_surface(&mut context, framebuffer_info, width, height);
    Box::into_raw(Box::new(State {
        context,
        framebuffer_info,
        surface,
        width,
        height,
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn resize(state: *mut State, width: i32, height: i32) {
    let state = unsafe { &mut *state };
    state.width = width;
    state.height = height;
    state.surface = create_surface(&mut state.context, state.framebuffer_info, width, height);
}

#[unsafe(no_mangle)]
pub extern "C" fn render(state: *mut State, time: f32) {
    let state = unsafe { &mut *state };
    scene::draw(
        state.surface.canvas(),
        state.width as f32,
        state.height as f32,
        time,
    );
    state
        .context
        .flush_and_submit_surface(&mut state.surface, None);
}

fn main() {}
