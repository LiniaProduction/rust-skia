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

  // Report what this browser actually offers. Skia targets Dawn, so a WebGPU that
  // is merely present is not enough — the diagnosis for "nothing renders" starts
  // with the difference between this output in Chrome and in Safari.
  const diag = {
    preferredCanvasFormat: navigator.gpu.getPreferredCanvasFormat(),
    adapter: adapter.info ? { vendor: adapter.info.vendor, architecture: adapter.info.architecture } : "n/a",
    features: [...adapter.features].sort(),
    limits: Object.fromEntries(
      ["maxStorageBuffersInVertexStage", "maxStorageBuffersInFragmentStage",
       "maxStorageBuffersPerShaderStage", "maxBindGroups", "maxColorAttachments",
       "maxTextureDimension2D", "maxVertexBuffers"]
        .map((k) => [k, adapter.limits[k]]),
    ),
  };
  console.log("[webgpu] capabilities", diag);
  document.getElementById("diag").textContent = JSON.stringify(diag, null, 2);

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

  // ?minimal=1 draws a single rectangle instead of the scene.
  // ?minimal=1 -> one rectangle; ?cycle=1 -> one tile at a time, so a backend that
  // silently drops draws reveals which tile it cannot handle.
  const params = new URLSearchParams(location.search);
  const cycling = params.has("cycle");
  const tiles = params.get("tiles");
  let mode = params.has("minimal") ? 1 : (tiles !== null ? 100 + Number(tiles) : 0);
  let tile = 0;
  if (cycling) {
    setInterval(() => { tile = (tile + 1) % 12; }, 1500);
  }

  let frames = 0;
  let lastReport = performance.now();
  const start = performance.now();

  function frame() {
    const now = performance.now();
    module._render_mode(state, (now - start) / 1000, cycling ? 10 + tile : mode);

    frames += 1;
    // Report an average over a full second. The previous version printed whatever had
    // accumulated by the first tick, which right after load is one frame -- and "1 fps"
    // sent us chasing a performance problem that did not exist.
    if (now - lastReport >= 1000 && frames > 0) {
      status.textContent = cycling
        ? `tile ${tile} — ${Math.round((frames * 1000) / (now - lastReport))} fps`
        : `${Math.round((frames * 1000) / (now - lastReport))} fps — Graphite / WebGPU` + (tiles !== null ? ` — first ${tiles} tiles` : "");
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
