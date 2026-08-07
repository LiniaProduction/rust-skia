// Ganesh/WebGL half of the comparison. Emscripten creates the WebGL context on the
// canvas it is given, so unlike the WebGPU page there is nothing asynchronous to do
// before starting the module.

import createModule from "./renderer.js";

const status = document.getElementById("status");
const canvas = document.getElementById("canvas");

async function main() {
  status.textContent = "loading wasm…";

  const module = await createModule({ canvas });

  // Passing the canvas is not enough: the context has to be created here and
  // registered with Emscripten, otherwise Skia finds no GL to talk to.
  const context = canvas.getContext("webgl2", { antialias: true, depth: true, stencil: true });
  if (!context) {
    throw new Error("This browser has no WebGL2 context.");
  }
  const handle = module.GL.registerContext(context, { majorVersion: 2 });
  module.GL.makeContextCurrent(handle);

  const state = module._init(canvas.width, canvas.height);
  if (!state) {
    throw new Error("Skia failed to initialise — see the console for details.");
  }

  let frames = 0;
  let lastReport = performance.now();
  const start = performance.now();

  function frame() {
    const now = performance.now();
    module._render(state, (now - start) / 1000);

    frames += 1;
    if (now - lastReport >= 1000) {
      status.textContent = `${frames} fps — Ganesh / WebGL`;
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
