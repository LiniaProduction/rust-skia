import createModule from "./wasm_example_webgpu_worker.js";

const log = (text) => postMessage({ log: text });
self.addEventListener("error", (e) => log(`error ${e.message}`));
self.addEventListener("unhandledrejection", (e) => log(`rejection ${e.reason}`));

onmessage = async ({ data }) => {
  try {
    await run(data.canvases, new URLSearchParams(data.search));
  } catch (err) {
    log(`EXC ${err}\n${err.stack}`);
    postMessage({ done: 1 });
  }
};

function makeBitmap(size, hue) {
  const c = new OffscreenCanvas(size, size);
  const g = c.getContext("2d");
  for (let y = 0; y < 8; y++)
    for (let x = 0; x < 8; x++) {
      g.fillStyle = (x + y) % 2 ? `hsl(${hue},80%,55%)` : "#f0f0f0";
      g.fillRect((x * size) / 8, (y * size) / 8, size / 8, size / 8);
    }
  g.fillStyle = "rgba(0,0,0,0.5)";
  g.beginPath();
  g.arc(size / 2, size / 2, size / 4, 0, Math.PI * 2);
  g.fill();
  return c;
}

async function run(canvases, params) {
  const frames = Number(params.get("frames") || 300);
  const mode = Number(params.get("mode") ?? -1);
  const offscreen = Number(params.get("offscreen") || 0);
  const interleave = params.has("interleave");
  const bigSize = Number(params.get("big") || 4096);

  log(`crossOriginIsolated=${self.crossOriginIsolated} gpu=${!!navigator.gpu}`);
  const adapter = await navigator.gpu.requestAdapter();
  const features = params.has("features") ? [...adapter.features] : params.has("feat") ? params.get("feat").split(",").filter((f) => adapter.features.has(f)) : [];
  log("adapter ok");
  const device = await adapter.requestDevice({ requiredFeatures: features });
  log("device ok");
  let uncaptured = 0;
  device.addEventListener("uncapturederror", (e) => {
    if (++uncaptured <= 5) log(`uncapturederror ${e.error.message}`);
  });
  device.lost.then((info) => log(`device.lost reason=${info.reason} message=${info.message}`));

  const t0 = performance.now();
  const module = await createModule({
    print: (s) => log(`stdout ${s}`),
    printErr: (s) => log(`stderr ${s}`),
  });
  log(`module ready ${(performance.now() - t0).toFixed(0)}ms WebGPU=${typeof module.WebGPU}`);
  const WebGPU = module.WebGPU;

  const devicePtr = WebGPU.importJsDevice(device);
  const rc = module._gpu_init(devicePtr);
  if (rc !== 0) throw new Error(`gpu_init ${rc}`);

  const format = navigator.gpu.getPreferredCanvasFormat();
  // ?gl=1 puts both canvases on the WebGL fallback, ?both=1 puts canvas 1 there while
  // canvas 0 stays on WebGPU -- the question is whether one wasm can drive both.
  const glOnly = params.has("gl");
  const both = params.has("both");
  const glCanvases = glOnly ? [0, 1] : both ? [1] : [];
  const glApps = new Map();
  for (const i of glCanvases) {
    const canvas = canvases[i];
    const gl = canvas.getContext("webgl2", { antialias: true, depth: false, stencil: true });
    const handle = module.GL.registerContext(gl, { majorVersion: 2 });
    module.GL.makeContextCurrent(handle);
    const index = module._gl_app_create(canvas.width, canvas.height);
    if (index < 0) throw new Error(`gl_app_create ${i}`);
    glApps.set(i, { canvas, gl, handle, index });
    log(`gl app ${i} index=${index} contextLost=${gl.isContextLost()}`);
  }
  const apps = canvases.map((canvas, i) => {
    if (glApps.has(i)) return { canvas, gl: glApps.get(i) };
    const ctx = canvas.getContext("webgpu");
    ctx.configure({
      device,
      format: "bgra8unorm",
      alphaMode: "premultiplied",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC | GPUTextureUsage.COPY_DST,
    });
    const surfacePtr = WebGPU.importJsSurface(ctx);
    const app = module._app_create(i, surfacePtr, canvas.width, canvas.height, offscreen);
    if (!app) throw new Error(`app_create ${i}`);
    return { canvas, ctx, app };
  });
  if (!glOnly && !both) module._scene_upload_image(apps[0].app);
  log(`preferredFormat=${format} apps=${apps.length}`);

  const imported = [];
  for (const [i, size] of [[0, 256], [1, bigSize]]) {
    const bitmap = await createImageBitmap(makeBitmap(size, i * 140), { premultiplyAlpha: "premultiply" });
    const ta = performance.now();
    const texture = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    device.queue.copyExternalImageToTexture({ source: bitmap }, { texture, premultipliedAlpha: true }, [bitmap.width, bitmap.height]);
    const texturePtr = WebGPU.importJsTexture(texture);
    const index = module._image_wrap_texture(apps[0].app, texturePtr, 1);
    const tb = performance.now();
    await device.queue.onSubmittedWorkDone();
    log(`import texture ${size}x${size}: js ${(tb - ta).toFixed(2)}ms, gpu done +${(performance.now() - tb).toFixed(1)}ms, index=${index}`);
    imported.push(texture);
    bitmap.close();
  }

  {
    const size = bigSize;
    const c = makeBitmap(size, 60);
    const ta = performance.now();
    const pixels = c.getContext("2d").getImageData(0, 0, size, size).data;
    const tb = performance.now();
    const ptr = module._malloc(pixels.length);
    module.HEAPU8.set(pixels, ptr);
    const tc = performance.now();
    const index = module._image_upload_pixels(apps[1].app, ptr, size, size);
    const td = performance.now();
    module._app_frame(apps[1].app, 0, 1, 3);
    await device.queue.onSubmittedWorkDone();
    module._free(ptr);
    log(`upload pixels ${size}x${size}: getImageData ${(tb - ta).toFixed(1)}ms, heap copy ${(tc - tb).toFixed(1)}ms, wasm ${(td - tc).toFixed(1)}ms, submit+gpu ${(performance.now() - td).toFixed(1)}ms, index=${index}`);
  }

  if (params.has("budget")) {
    const [count, size, mb] = params.get("budget").split(",").map(Number);
    device.pushErrorScope("out-of-memory");
    device.pushErrorScope("validation");
    const idle = async () => {
      await device.queue.onSubmittedWorkDone();
      await new Promise((r) => setTimeout(r, 50));
    };
    const step = (n, c = count, sz = size) => module._budget_step(apps[0].app, n, c, sz, mb);
    step(0); step(1); await idle(); step(4); step(7);
    step(2); step(4); step(3); step(4); await idle(); step(4); step(7);
    step(5); step(6);
    step(1, count * 2); await idle(); step(4);
    step(2); step(3); await idle(); step(4); step(5);
    const v = await device.popErrorScope();
    const o = await device.popErrorScope();
    log(`budget probe scopes: validation=${v?.message ?? "ok"} oom=${o?.message ?? "ok"}`);
  }

  const scope = new Map();
  let frame = 0;
  const start = performance.now();
  const frameMs = [];
  const channel = new MessageChannel();
  const tick = () => {
    frame++;
    const time = (performance.now() - start) / 1000;
    device.pushErrorScope("validation");
    const f0 = performance.now();
    const modeFor = (i) => (params.has("shared") ? (i === 0 ? 0 : 3) : mode >= 0 ? mode : i === 0 ? 0 : 2);
    const drawApp = (a, i, phase) => {
      if (a.gl) {
        module.GL.makeContextCurrent(a.gl.handle);
        module._gl_app_frame(a.gl.index, time, a.canvas.width, a.canvas.height);
        return;
      }
      module._app_frame(a.app, time, modeFor(i), phase);
    };
    if (interleave) {
      apps.forEach((a, i) => drawApp(a, i, 1));
      if (apps[0].app) module._app_frame(apps[0].app, time, modeFor(0), 2);
    } else {
      apps.forEach((a, i) => drawApp(a, i, 3));
    }
    frameMs.push(performance.now() - f0);
    device.popErrorScope().then((e) => {
      const key = e ? e.message.slice(0, 300) : "ok";
      scope.set(key, (scope.get(key) || 0) + 1);
    });
    if (frame === Math.floor(frames / 2) && params.has("resize")) {
      apps[1].canvas.width = 320;
      apps[1].canvas.height = 480;
      module._app_resize(apps[1].app, 320, 480);
      log("resized app 1 to 320x480");
    }
    if (params.has("lose") && frame === frames - 20) {
      log("destroying device");
      device.destroy();
    }
    if (frame < frames) {
      setTimeout(tick, 16);
    } else {
      finish();
    }
  };
  const finish = async () => {
    await device.queue.onSubmittedWorkDone().catch(() => {});
    await new Promise((r) => setTimeout(r, 200));
    frameMs.sort((a, b) => a - b);
    const p = (q) => frameMs[Math.floor(q * (frameMs.length - 1))].toFixed(2);
    log(`pthread_sum=${module._pthread_sum()} frames=${frame} cpu ms p50=${p(0.5)} p95=${p(0.95)} max=${p(1)} uncaptured=${uncaptured}`);
    log(`scope ${JSON.stringify([...scope.entries()])}`);
    postMessage({ done: 1 });
  };
  tick();
}
