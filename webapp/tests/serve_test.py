import functools
import gzip
import http.client
import importlib.util
from pathlib import Path
import tempfile
import threading
import unittest
from http.server import ThreadingHTTPServer

spec = importlib.util.spec_from_file_location("serve", Path(__file__).parents[1] / "serve.py")
serve = importlib.util.module_from_spec(spec)
spec.loader.exec_module(serve)


class ServerTest(unittest.TestCase):
    def test_wasm_negotiation_head_and_rebuild(self):
        with tempfile.TemporaryDirectory() as root:
            asset = Path(root) / "engine.wasm"
            original = b"\x00asm" + b"engine" * 10000
            asset.write_bytes(original)
            handler = functools.partial(serve.NoCacheHandler, directory=root)
            server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                def request(encoding, method="GET"):
                    connection = http.client.HTTPConnection(*server.server_address)
                    connection.request(method, "/engine.wasm", headers={"Accept-Encoding": encoding})
                    response = connection.getresponse()
                    result = response.status, dict(response.getheaders()), response.read()
                    connection.close()
                    return result

                status, headers, body = request("gzip")
                self.assertEqual(status, 200)
                self.assertEqual(headers["Content-Type"], "application/wasm")
                self.assertEqual(headers["Vary"], "Accept-Encoding")
                self.assertIn("no-store", headers["Cache-Control"])
                self.assertEqual(gzip.decompress(body), original)
                self.assertLess(len(body), len(original))
                self.assertEqual(request("gzip", "HEAD")[2], b"")
                for encoding in ("identity", "gzip;q=0", "gzip;q=invalid"):
                    _, headers, body = request(encoding)
                    self.assertNotIn("Content-Encoding", headers)
                    self.assertEqual(body, original)
                rebuilt = b"\x00asm" + b"changed" * 10000
                asset.write_bytes(rebuilt)
                self.assertEqual(gzip.decompress(request("gzip")[2]), rebuilt)
            finally:
                server.shutdown()
                server.server_close()
                thread.join()
