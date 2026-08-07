// WebGPU setup for the example.
//
// Written in C++ on purpose. These descriptors are chained structs whose layout must
// match the headers exactly; declaring them by hand in Rust would turn a header
// revision into silent memory corruption. Here the layouts come from
// <webgpu/webgpu_cpp.h> by construction, and Rust only ever sees opaque handles.
//
// The surface is created inside wasm rather than handed in from JavaScript because
// emdawnwebgpu can import a JS device or buffer but not a JS texture — so the
// per-frame swapchain texture has to be obtained on this side.

// Note: deliberately NOT <emscripten/html5_webgpu.h>. That header belongs to
// Emscripten's legacy built-in binding and refers to WGPUSwapChain, a type
// emdawnwebgpu no longer has. `emscripten_webgpu_get_device` is declared by
// emdawnwebgpu's own <webgpu/webgpu.h>, which webgpu_cpp.h pulls in.
#include <cstdio>
#include <webgpu/webgpu_cpp.h>

namespace {
// The canvas format that browsers present. Kept in one place because Skia must be
// told the matching SkColorType.
constexpr wgpu::TextureFormat kSurfaceFormat = wgpu::TextureFormat::BGRA8Unorm;
}  // namespace

extern "C" {

WGPUInstance demo_wgpu_create_instance() {
    // A null descriptor is the browser instance; there is nothing to configure.
    return wgpuCreateInstance(nullptr);
}

// The device is created asynchronously by JavaScript and handed over through
// Module.preinitializedWebGPUDevice before the module starts.
WGPUDevice demo_wgpu_device() { return emscripten_webgpu_get_device(); }

WGPUQueue demo_wgpu_queue(WGPUDevice device) {
    return wgpuDeviceGetQueue(device);
}

WGPUSurface demo_wgpu_create_surface(WGPUInstance instance, const char* selector) {
    wgpu::EmscriptenSurfaceSourceCanvasHTMLSelector canvasSource;
    canvasSource.selector = selector;

    wgpu::SurfaceDescriptor descriptor;
    descriptor.nextInChain = &canvasSource;

    wgpu::Instance wrapped(instance);
    wgpu::Surface surface = wrapped.CreateSurface(&descriptor);
    if (!surface) {
        std::printf("[webgpu] CreateSurface failed for selector '%s'\n", selector);
        return nullptr;
    }
    // Hand ownership to the caller; the wrapper must not release on scope exit.
    return surface.MoveToCHandle();
}

void demo_wgpu_configure(WGPUSurface surface, WGPUDevice device, int width, int height) {
    wgpu::SurfaceConfiguration config;
    config.device = wgpu::Device(device);
    config.format = kSurfaceFormat;
    config.usage = wgpu::TextureUsage::RenderAttachment | wgpu::TextureUsage::TextureBinding |
                   wgpu::TextureUsage::CopySrc | wgpu::TextureUsage::CopyDst;
    config.width = static_cast<uint32_t>(width);
    config.height = static_cast<uint32_t>(height);
    config.alphaMode = wgpu::CompositeAlphaMode::Opaque;

    wgpu::Surface(surface).Configure(&config);
}

// Returns the texture for this frame, or null if the surface is not usable right now
// (resized, lost, or otherwise not ready). The caller must release it after drawing.
WGPUTexture demo_wgpu_current_texture(WGPUSurface surface) {
    wgpu::SurfaceTexture surfaceTexture;
    wgpu::Surface(surface).GetCurrentTexture(&surfaceTexture);

    if (surfaceTexture.status != wgpu::SurfaceGetCurrentTextureStatus::SuccessOptimal &&
        surfaceTexture.status != wgpu::SurfaceGetCurrentTextureStatus::SuccessSuboptimal) {
        std::printf("[webgpu] GetCurrentTexture status %d\n",
                    static_cast<int>(surfaceTexture.status));
        return nullptr;
    }
    return surfaceTexture.texture.MoveToCHandle();
}

void demo_wgpu_release_texture(WGPUTexture texture) {
    if (texture) {
        wgpuTextureRelease(texture);
    }
}

}  // extern "C"
