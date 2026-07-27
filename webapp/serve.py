#!/usr/bin/env python3
"""Dev server for the OpenDoc WASM test harness.

Serves this directory on http://localhost:8099 with **no-cache** headers, so a
rebuilt `pkg/` or an edited `src/` is picked up on a plain reload — the default
`python3 -m http.server` lets the browser heuristically cache the wasm/JS, which
silently serves a stale build (a real source of "I updated it but nothing
changed" confusion). Run it after `./build.sh`:

    ./serve.py            then open http://localhost:8099/index.html
"""
import http.server
import sys


class NoCacheHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store, no-cache, must-revalidate, max-age=0")
        self.send_header("Pragma", "no-cache")
        self.send_header("Expires", "0")
        super().end_headers()


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8099
    print(f"Serving the WASM harness (no-cache) on http://localhost:{port}/index.html")
    http.server.test(HandlerClass=NoCacheHandler, port=port, bind="127.0.0.1")
