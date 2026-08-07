// Ship everything the page prints back to the dev server.
//
// Safari has no remote console we can read from a script, and the failure under
// investigation only happens there. Rather than reading a screenshot of the web
// inspector, the page posts its console -- including the wasm module's stdout and
// stderr, which emscripten routes through console.log/console.error -- to /log,
// where serve.py appends it to a file.
//
// This must run before the module script, so the hooks are in place before wasm
// starts printing; index.html loads it as a classic script for exactly that reason.
// Posting is best effort: against a plain static server the POST fails and the page
// behaves as if this file were not here.
(function () {
  const pending = [];
  const started = performance.now();

  function record(level, args) {
    const text = args
      .map((a) => {
        if (typeof a === "string") return a;
        if (a instanceof Error) return `${a.message}\n${a.stack || ""}`;
        try {
          return JSON.stringify(a);
        } catch {
          return String(a);
        }
      })
      .join(" ");
    pending.push(`[${((performance.now() - started) / 1000).toFixed(3)}s ${level}] ${text}`);
  }

  for (const level of ["log", "info", "warn", "error"]) {
    const original = console[level].bind(console);
    console[level] = (...args) => {
      record(level, args);
      original(...args);
    };
  }

  window.addEventListener("error", (e) => record("onerror", [e.message, e.filename + ":" + e.lineno]));
  window.addEventListener("unhandledrejection", (e) => record("rejection", [String(e.reason)]));

  function flush(beacon) {
    if (!pending.length) return;
    const body = pending.splice(0).join("\n") + "\n";
    if (beacon && navigator.sendBeacon) {
      navigator.sendBeacon("/log", body);
    } else {
      fetch("/log", { method: "POST", body }).catch(() => {});
    }
  }

  setInterval(() => flush(false), 500);
  window.addEventListener("beforeunload", () => flush(true));
  window.__flushLog = flush;
})();
