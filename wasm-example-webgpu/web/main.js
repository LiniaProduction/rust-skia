// Device creation is asynchronous and wasm is not, so JavaScript does it first and
// hands the finished device to the module through `preinitializedWebGPUDevice`.
// Everything after that — surface, swapchain texture, drawing — happens inside wasm.

import createModule from "./renderer.js";

const status = document.getElementById("status");
const canvas = document.getElementById("canvas");

function fail(message) {
  status.textContent = message;
  status.classList.add("bad");
  throw new Error(message);
}

async function main() {
  if (!navigator.gpu) {
    fail("This browser has no WebGPU (navigator.gpu is undefined).");
  }

  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) {
    fail("No WebGPU adapter available.");
  }

  const device = await adapter.requestDevice();
  device.lost.then((info) => {
    status.textContent = `device lost: ${info.reason} — ${info.message}`;
    status.classList.add("bad");
  });

  status.textContent = "loading wasm…";

  const module = await createModule({
    // Read by emdawnwebgpu during startup; emscripten_webgpu_get_device() returns it.
    preinitializedWebGPUDevice: device,
  });

  // The canvas is configured on the wasm side, which needs it to exist under the
  // selector the Rust code passes ("#canvas").
  const state = module._init(canvas.width, canvas.height);
  if (!state) {
    fail("Skia failed to initialise — see the console for details.");
  }

  let frames = 0;
  let lastReport = performance.now();
  const start = performance.now();

  function frame() {
    const now = performance.now();
    module._render(state, (now - start) / 1000);

    frames += 1;
    if (now - lastReport >= 1000) {
      status.textContent = `${frames} fps — Graphite / WebGPU`;
      frames = 0;
      lastReport = now;
    }
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main().catch((error) => {
  status.textContent = String(error);
  status.classList.add("bad");
  console.error(error);
});
