// Name the WebGPU call that fails, instead of the frame that fails.
//
// An error scope around a whole frame says only "something in these few hundred
// calls was invalid". This wraps the WebGPU methods themselves, so each traced call
// gets its own scope and reports for itself. The call order it prints is worth as
// much as the error: it shows what Skia actually asked the browser to do, without
// having to infer it from the C++.
//
// Off unless the page turns it on (?trace=N traces the first N frames), because a
// scope per call is far too slow to leave running.
(function () {
  const state = { on: false, device: null, seq: 0, depth: 0 };
  window.__trace = state;

  function log(text) {
    console.log(`[trace] ${text}`);
  }

  // WebKit defers a bad beginRenderPass: the pass comes back invalid, the encoder is
  // quietly marked invalid with it, and the only error raised is at finish(). So the
  // descriptor has to be printed to find which pass the browser refused. A view does
  // not expose its texture, so textures are tagged as they are created and the tag is
  // carried onto their views.
  function tagTextures() {
    const createTexture = GPUDevice.prototype.createTexture;
    GPUDevice.prototype.createTexture = function (desc) {
      const texture = createTexture.call(this, desc);
      texture.__tag = `${desc.format} ${desc.size.width ?? desc.size[0]}x${desc.size.height ?? desc.size[1]}` +
                      ` samples=${desc.sampleCount || 1} usage=0x${(desc.usage || 0).toString(16)}` +
                      (desc.label ? ` "${desc.label}"` : "");
      return texture;
    };
    const getCurrentTexture = GPUCanvasContext.prototype.getCurrentTexture;
    GPUCanvasContext.prototype.getCurrentTexture = function () {
      const texture = getCurrentTexture.call(this);
      texture.__tag = "SWAPCHAIN";
      return texture;
    };
    const createView = GPUTexture.prototype.createView;
    GPUTexture.prototype.createView = function (desc) {
      const view = createView.call(this, desc);
      view.__tag = this.__tag || "(untagged)";
      return view;
    };
  }

  function describeRenderPass(desc) {
    const color = (desc.colorAttachments || []).filter(Boolean).map((a) =>
      `      color: ${a.view && a.view.__tag} load=${a.loadOp} store=${a.storeOp}` +
      (a.resolveTarget ? ` resolve->[${a.resolveTarget.__tag}]` : " resolve=none"));
    const ds = desc.depthStencilAttachment;
    const dsLine = ds
      ? [`      depth: ${ds.view && ds.view.__tag} depthLoad=${ds.depthLoadOp} depthStore=${ds.depthStoreOp}` +
         ` stencilLoad=${ds.stencilLoadOp} stencilStore=${ds.stencilStoreOp}` +
         ` readOnlyDepth=${ds.depthReadOnly} readOnlyStencil=${ds.stencilReadOnly}`]
      : [];
    return [...color, ...dsLine].join("\n");
  }

  // Methods worth naming. Per-draw calls inside a pass are left alone: there are
  // thousands per frame, and an invalid draw shows up as an invalid pass anyway.
  const TARGETS = [
    ["GPUDevice", ["createCommandEncoder", "createRenderPipeline", "createTexture", "createBindGroup"]],
    ["GPUCommandEncoder", ["beginRenderPass", "beginComputePass", "copyTextureToTexture",
                           "copyBufferToTexture", "copyTextureToBuffer", "copyBufferToBuffer",
                           "clearBuffer", "finish"]],
    ["GPURenderPassEncoder", ["end", "setViewport", "setScissorRect"]],
    ["GPUComputePassEncoder", ["end"]],
    ["GPUQueue", ["submit", "writeBuffer", "writeTexture"]],
  ];

  for (const [typeName, methods] of TARGETS) {
    const type = window[typeName];
    if (!type) continue;
    for (const method of methods) {
      const original = type.prototype[method];
      if (typeof original !== "function") continue;
      type.prototype[method] = function (...args) {
        if (!state.on || !state.device || state.probing) {
          return original.apply(this, args);
        }
        const id = ++state.seq;
        const indent = "  ".repeat(state.depth);
        // Viewport and scissor are the calls whose arguments have to be compared
        // against the attachment size, so they print theirs.
        const detail = method === "setViewport" || method === "setScissorRect"
          ? ` (${args.join(", ")})`
          : "";
        log(`${indent}#${id} ${typeName}.${method}${detail}`);
        // WebKit defers these failures to finish(), so each is replayed alone to see
        // whether it is the one that poisons the encoder.
        if (method === "beginRenderPass") {
          log(describeRenderPass(args[0] || {}));
          probe(id, "PASS DESCRIPTOR", (e) => e.beginRenderPass(args[0]).end());
        } else if (method.startsWith("copy")) {
          probe(id, method.toUpperCase(), (e) => e[method](...args));
        }
        state.device.pushErrorScope("validation");
        let result;
        try {
          result = original.apply(this, args);
        } finally {
          state.device.popErrorScope().then((error) => {
            if (error) {
              log(`#${id} ${typeName}.${method} FAILED: ${error.message}`);
            }
          });
        }
        return result;
      };
    }
  }

  // Replay one descriptor on an encoder of its own and finish it. The real encoder
  // records many passes, so "this encoder is invalid" does not say which of them the
  // browser refused; a throwaway encoder holding a single pass does.
  function probe(id, what, record) {
    // The probe's own calls go through the same wrappers, which would probe them in
    // turn; trace nothing while probing.
    if (state.probing) return;
    state.probing = true;
    const device = state.device;
    device.pushErrorScope("validation");
    const encoder = device.createCommandEncoder({ label: `probe-${id}` });
    try {
      record(encoder);
      encoder.finish();
    } catch (e) {
      log(`#${id} probe threw: ${e.message}`);
    } finally {
      state.probing = false;
    }
    device.popErrorScope().then((error) => {
      if (error) log(`#${id} ${what} REJECTED: ${error.message}`);
    });
  }

  tagTextures();

  // The device is needed for the scopes; take it as it is handed out.
  const requestDevice = GPUAdapter.prototype.requestDevice;
  GPUAdapter.prototype.requestDevice = async function (...args) {
    const device = await requestDevice.apply(this, args);
    state.device = device;
    return device;
  };
})();
