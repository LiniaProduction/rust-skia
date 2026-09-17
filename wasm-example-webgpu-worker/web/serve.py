import http.server, os, socketserver, sys
PORT = int(sys.argv[1]); LOGFILE = sys.argv[2]; DIRECTORY = sys.argv[3] if len(sys.argv) > 3 else os.getcwd()
class H(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, '.wasm': 'application/wasm', '.js': 'text/javascript', '.mjs': 'text/javascript'}
    def __init__(s, *a, **k): super().__init__(*a, directory=DIRECTORY, **k)
    def do_POST(s):
        n = int(s.headers.get("Content-Length", 0))
        with open(LOGFILE, "ab") as f: f.write(s.rfile.read(n))
        s.send_response(204); s.end_headers()
    def end_headers(s):
        s.send_header("Cache-Control", "no-store")
        s.send_header("Cross-Origin-Opener-Policy", "same-origin")
        s.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        super().end_headers()
    def log_message(s, *a): pass
socketserver.TCPServer.allow_reuse_address = True
with socketserver.ThreadingTCPServer(("127.0.0.1", PORT), H) as h: h.serve_forever()
