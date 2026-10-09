#!/usr/bin/env python3
"""Dev server for the OpenDoc developer site and WASM editor.

Serves this directory on http://localhost:8099 with **no-cache** headers, so a
rebuilt `pkg/` or an edited `src/` is picked up on a plain reload — the default
`python3 -m http.server` lets the browser heuristically cache the wasm/JS, which
silently serves a stale build (a real source of "I updated it but nothing
changed" confusion). Run it after `./build.sh`:

    ./serve.py            then open http://localhost:8099/
"""
import http.server
import sys
import gzip
import io
from functools import lru_cache
from pathlib import Path


@lru_cache(maxsize=2)
def compressed_wasm(path, modified, size):
    # Keyed by file metadata so a rebuilt engine never serves the old bytes.
    return gzip.compress(Path(path).read_bytes(), compresslevel=6, mtime=0)


def accepts_gzip(header):
    for item in header.lower().split(","):
        encoding, *parameters = item.strip().split(";")
        if encoding != "gzip":
            continue
        try:
            quality = next((float(p.strip()[2:]) for p in parameters
                            if p.strip().startswith("q=")), 1.0)
        except ValueError:
            return False
        return 0 < quality <= 1
    return False


class NoCacheHandler(http.server.SimpleHTTPRequestHandler):
    def send_head(self):
        path = Path(self.translate_path(self.path))
        gzip_allowed = accepts_gzip(self.headers.get("Accept-Encoding", ""))
        if path.suffix != ".wasm" or not path.is_file() or not gzip_allowed:
            return super().send_head()
        stat = path.stat()
        payload = compressed_wasm(str(path), stat.st_mtime_ns, stat.st_size)
        self.send_response(200)
        self.send_header("Content-Type", "application/wasm")
        self.send_header("Content-Encoding", "gzip")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        return io.BytesIO(payload)

    def end_headers(self):
        if Path(self.translate_path(self.path)).suffix == ".wasm":
            self.send_header("Vary", "Accept-Encoding")
        self.send_header("Cache-Control", "no-store, no-cache, must-revalidate, max-age=0")
        self.send_header("Pragma", "no-cache")
        self.send_header("Expires", "0")
        super().end_headers()


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8099
    print(f"Serving OpenDoc (no-cache) on http://localhost:{port}/")
    # ThreadingHTTPServer (not the single-threaded `http.server.test`): the
    # editor loads the wasm, JS, fonts, and fixtures concurrently, and a
    # single-threaded server stalls or refuses those parallel connections —
    # which flaked the Playwright e2e run (ERR_CONNECTION_REFUSED). One request
    # per thread keeps the dev server responsive under that concurrency.
    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), NoCacheHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        server.shutdown()
