//! Dawn (WebGPU) backend support for Graphite.
//!
//! On `wasm32-unknown-emscripten` the WebGPU implementation is the browser's, reached
//! through Emscripten's `webgpu/webgpu.h`. Device creation is asynchronous there and
//! stays on the JavaScript side; this module takes the resulting handles and hands them
//! to Skia.

use std::fmt;

use skia_bindings as sb;

use crate::{
    graphite::{BackendTexture, Context, ContextOptions},
    prelude::{self, NativeAccess, NativeDrop},
};

/// An opaque WebGPU handle, matching the `WGPU*` C types.
///
/// Under Emscripten these are the values JavaScript passes into wasm.
pub type Handle = *mut std::ffi::c_void;

pub type BackendContext = prelude::Handle<sb::skgpu_graphite_DawnBackendContext>;
unsafe_send_sync!(BackendContext);

impl NativeDrop for sb::skgpu_graphite_DawnBackendContext {
    fn drop(&mut self) {
        unsafe { sb::C_DawnBackendContext_destruct(self) }
    }
}

impl fmt::Debug for BackendContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackendContext").finish()
    }
}

impl BackendContext {
    /// Builds a backend context from WebGPU handles the caller already owns.
    ///
    /// Skia references the objects, so the caller stays responsible for releasing its
    /// own handles.
    ///
    /// # Safety
    ///
    /// `instance`, `device` and `queue` must be valid, non-null `WGPUInstance`,
    /// `WGPUDevice` and `WGPUQueue` handles belonging to the same device.
    pub unsafe fn new(instance: Handle, device: Handle, queue: Handle) -> Self {
        prelude::Handle::construct(|bc| unsafe {
            sb::C_DawnBackendContext_Construct(bc, instance as _, device as _, queue as _)
        })
    }
}

/// Create a new Graphite [`Context`] backed by WebGPU.
///
/// # Arguments
///
/// - `backend_context` - the instance, device and queue to render with.
/// - `options` - optional context configuration, defaults to
///   [`ContextOptions::default()`] if `None`.
///
/// # Returns
///
/// A new [`Context`], or `None` if creation failed.
pub fn make_context<'a>(
    backend_context: &BackendContext,
    options: impl Into<Option<&'a ContextOptions>>,
) -> Option<Context> {
    let default_options;
    let options_ptr = match options.into() {
        Some(opts) => opts.native() as *const _,
        None => {
            default_options = ContextOptions::default();
            default_options.native() as *const _
        }
    };

    Context::from_ptr(unsafe {
        sb::C_ContextFactory_MakeDawn(backend_context.native(), options_ptr)
    })
}

/// Wrap a `WGPUTexture` the caller owns as a Graphite [`BackendTexture`].
///
/// The usual source is `GPUCanvasContext.getCurrentTexture()`, whose texture is only
/// valid for the current frame — do not hold the result past the frame it came from.
///
/// # Safety
///
/// `texture` must be a valid, non-null `WGPUTexture`.
pub unsafe fn backend_texture(texture: Handle) -> BackendTexture {
    unsafe { BackendTexture::construct(|bt| sb::C_BackendTextures_MakeDawn(bt, texture as _)) }
}
