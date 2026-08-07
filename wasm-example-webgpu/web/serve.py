#!/usr/bin/env python3
"""Serve this directory and collect what the page prints.

    ./serve.py [port] [logfile]
    (default: port 8090, log to ./page.log)

`python3 -m http.server` is enough to load the page, but not to debug it in Safari,
which offers no console a script can read. collect-log.js posts the page's console --
including the wasm module's stdout and stderr -- to /log, and this server appends it to
a file, so a failing run can be read with `tail -f` instead of a screenshot of the web
inspector. That is how the Safari-only failures in this example were found.
"""
import http.server
import os
import socketserver
import sys

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8090
LOGFILE = sys.argv[2] if len(sys.argv) > 2 else "page.log"
DIRECTORY = os.path.dirname(os.path.abspath(__file__))


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=DIRECTORY, **kwargs)

    def do_POST(self):
        if self.path != "/log":
            self.send_error(404)
            return
        length = int(self.headers.get("Content-Length", 0))
        with open(LOGFILE, "ab") as f:
            f.write(self.rfile.read(length))
        self.send_response(204)
        self.end_headers()

    def end_headers(self):
        # A stale wasm behind a fixed filename is a long detour; never cache.
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


socketserver.TCPServer.allow_reuse_address = True
print(f"serving {DIRECTORY} on http://127.0.0.1:{PORT}, page log -> {LOGFILE}")
with socketserver.TCPServer(("127.0.0.1", PORT), Handler) as httpd:
    httpd.serve_forever()
