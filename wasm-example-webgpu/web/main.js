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
  // Without this, WebGPU validation errors are simply swallowed: the frame comes out
  // wrong and nothing is reported. Not having it is why the Safari failure looked
  // silent for so long.
  device.addEventListener("uncapturederror", (event) => {
    const text = `WebGPU error: ${event.error.message}`;
    console.error(text);
    const box = document.getElementById("diag");
    box.textContent = text + "\n\n" + box.textContent;
    status.textContent = "WebGPU validation error — see below";
    status.classList.add("bad");
  });

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
  // ?only=0,8 draws just those tiles -- for testing combinations rather than prefixes.
  const only = params.get("only");
  const mask = only ? only.split(",").reduce((m, n) => m | (1 << Number(n)), 0) : 0;
  // ?blend=N restricts the blend tile to one mode: 0 SrcOver, 1 Plus, 2 Screen,
  // 3 Overlay, 4 Darken, 5 SoftLight.
  const blend = params.get("blend");
  const tiles = params.get("tiles");
  let mode = params.has("minimal") ? 1 : (tiles !== null ? 100 + Number(tiles) : 0);
  let tile = 0;
  if (cycling) {
    setInterval(() => { tile = (tile + 1) % 12; }, 1500);
  }

  if (blend !== null) {
    module._set_blend_filter(Number(blend));
  }

  let frames = 0;
  let lastReport = performance.now();
  const start = performance.now();

  // ?scope=1 brackets every frame in a validation error scope. `uncapturederror` is
  // not a reliable count: it says an error happened, not how many frames it happened
  // in, and a browser is free to report the same one once. A scope per frame answers
  // "every frame or just the first?", which is the difference between a broken
  // command buffer and a one-off at startup.
  const scoped = params.has("scope");
  const scopeCounts = new Map();
  let scopeFrames = 0;

  // ?trace=N names the failing WebGPU call for the first N frames; see trace-webgpu.js.
  const traceFrames = params.has("trace") ? Number(params.get("trace")) || 1 : 0;
  let frameNo = 0;

  function frame() {
    const now = performance.now();
    frameNo += 1;
    if (traceFrames) {
      window.__trace.on = frameNo <= traceFrames;
    }
    if (scoped) {
      device.pushErrorScope("validation");
    }
    if (only) {
      module._render_mask(state, (now - start) / 1000, mask);
    } else {
      module._render_mode(state, (now - start) / 1000, cycling ? 10 + tile : mode);
    }
    if (scoped) {
      const n = ++scopeFrames;
      device.popErrorScope().then((error) => {
        const key = error ? error.message : "(no error)";
        const seen = (scopeCounts.get(key) || 0) + 1;
        scopeCounts.set(key, seen);
        // Every frame would otherwise print the same line 60 times a second; the
        // first few and then a periodic tally is enough to tell the two cases apart.
        if (seen <= 3 || n % 120 === 0) {
          console.log(`[scope] frame ${n}: ${key} (x${seen})`);
        }
      });
    }

    frames += 1;
    // Report an average over a full second. The previous version printed whatever had
    // accumulated by the first tick, which right after load is one frame -- and "1 fps"
    // sent us chasing a performance problem that did not exist.
    if (now - lastReport >= 1000 && frames > 0) {
      status.textContent = cycling
        ? `tile ${tile} — ${Math.round((frames * 1000) / (now - lastReport))} fps`
        : `${Math.round((frames * 1000) / (now - lastReport))} fps — Graphite / WebGPU` + (blend !== null ? ` — blend ${blend}` : "") + (only ? ` — tiles ${only}` : tiles !== null ? ` — first ${tiles} tiles` : "");
      frames = 0;
      lastReport = now;
    }
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);

  // ?pixels=1 reports what actually ended up on the canvas. "Did it go black?" should
  // be answered by reading the pixels, not by looking at a screenshot -- and a mean
  // brightness of zero is exactly the symptom a dropped command buffer produces.
  if (params.has("pixels")) {
    setTimeout(() => {
      const copy = document.createElement("canvas");
      copy.width = canvas.width;
      copy.height = canvas.height;
      const ctx = copy.getContext("2d");
      ctx.drawImage(canvas, 0, 0);
      const { data } = ctx.getImageData(0, 0, copy.width, copy.height);
      let sum = 0;
      let nonBlack = 0;
      for (let i = 0; i < data.length; i += 4) {
        const v = data[i] + data[i + 1] + data[i + 2];
        sum += v;
        if (v > 12) nonBlack += 1;
      }
      const pixels = data.length / 4;
      // The dominant colours say more than a brightness average: a tile that draws the
      // wrong thing and a tile that draws nothing can average out the same, but the
      // checkerboard's two colours either appear in this list or they do not.
      const histogram = new Map();
      for (let i = 0; i < data.length; i += 4) {
        const key = ((data[i] >> 4) << 8 | (data[i + 1] >> 4) << 4 | (data[i + 2] >> 4))
          .toString(16).padStart(3, "0");
        histogram.set(key, (histogram.get(key) || 0) + 1);
      }
      const top = [...histogram.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, 6)
        .map(([k, n]) => `${k}:${((100 * n) / pixels).toFixed(1)}%`)
        .join(" ");
      console.log(`[pixels] mean=${(sum / pixels / 3).toFixed(1)} ` +
                  `nonBlack=${((100 * nonBlack) / pixels).toFixed(1)}% top=${top}`);
    }, 2000);
  }
}

main().catch((error) => {
  status.textContent = String(error);
  status.classList.add("bad");
  console.error(error);
});
