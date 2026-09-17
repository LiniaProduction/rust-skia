// WebGPU handles for the worker example. JavaScript creates the device, configures the
// canvas context and imports both through emdawnwebgpu's WebGPU.importJs* helpers;
// this side only needs what Skia takes on input and the per-frame surface texture.
#include <cstdio>
#include <webgpu/webgpu_cpp.h>

extern "C" {

WGPUInstance spike_create_instance() { return wgpuCreateInstance(nullptr); }

WGPUQueue spike_device_queue(WGPUDevice device) { return wgpuDeviceGetQueue(device); }

WGPUTexture spike_current_texture(WGPUSurface surface) {
    wgpu::SurfaceTexture st;
    wgpu::Surface(surface).GetCurrentTexture(&st);
    if (st.status != wgpu::SurfaceGetCurrentTextureStatus::SuccessOptimal &&
        st.status != wgpu::SurfaceGetCurrentTextureStatus::SuccessSuboptimal) {
        std::printf("[spike] GetCurrentTexture status %d\n", static_cast<int>(st.status));
        return nullptr;
    }
    return st.texture.MoveToCHandle();
}

void spike_texture_release(WGPUTexture texture) {
    if (texture) wgpuTextureRelease(texture);
}

void spike_instance_process_events(WGPUInstance instance) { wgpuInstanceProcessEvents(instance); }

}
