const params = new URLSearchParams(location.search);
const tag = params.get("tag") || "page";
const out = document.getElementById("out");
const pending = [];
function log(text) {
  const line = `[${tag}] ${text}`;
  pending.push(line);
  out.textContent += line + "\n";
}
setInterval(() => {
  if (pending.length) fetch("/log", { method: "POST", body: pending.splice(0).join("\n") + "\n" });
}, 250);

log(`main ua=${navigator.userAgent} crossOriginIsolated=${self.crossOriginIsolated}`);
const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
worker.onerror = (e) => log(`worker onerror ${e.message} ${e.filename}:${e.lineno}`);
worker.onmessage = (e) => {
  if (e.data.log !== undefined) log(`worker ${e.data.log}`);
  if (e.data.done) setTimeout(readPixels, 300);
};
const canvases = ["c0", "c1"].map((id) => document.getElementById(id));
const offscreen = canvases.map((c) => c.transferControlToOffscreen());
worker.postMessage({ canvases: offscreen, search: location.search }, offscreen);

function readPixels() {
  for (const c of canvases) {
    const copy = document.createElement("canvas");
    copy.width = c.width;
    copy.height = c.height;
    const ctx = copy.getContext("2d");
    ctx.drawImage(c, 0, 0);
    const { data } = ctx.getImageData(0, 0, copy.width, copy.height);
    const hist = new Map();
    let nonBlack = 0;
    for (let i = 0; i < data.length; i += 4) {
      const key = ((data[i] >> 4) << 8 | (data[i + 1] >> 4) << 4 | (data[i + 2] >> 4)).toString(16).padStart(3, "0");
      hist.set(key, (hist.get(key) || 0) + 1);
      if (data[i] + data[i + 1] + data[i + 2] > 12) nonBlack++;
    }
    const n = data.length / 4;
    const top = [...hist.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6)
      .map(([k, v]) => `${k}:${((100 * v) / n).toFixed(1)}%`).join(" ");
    log(`pixels ${c.id} ${copy.width}x${copy.height} nonBlack=${((100 * nonBlack) / n).toFixed(1)}% distinct=${hist.size} top=${top}`);
  }
  log("END");
}
