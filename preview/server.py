#!/usr/bin/env python3
"""Minimal preview server exposing only the UI preview and wallpaper asset."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent.parent
FILES = {
    "/": (ROOT / "preview" / "index.html", "text/html; charset=utf-8"),
    "/preview/index.html": (ROOT / "preview" / "index.html", "text/html; charset=utf-8"),
    "/assets/nexus-wallpaper.png": (ROOT / "assets" / "nexus-wallpaper.png", "image/png"),
}


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if urlsplit(self.path).path == "/preview-revision":
            import hashlib
            digest = hashlib.sha256()
            for path in sorted({entry[0] for entry in FILES.values()}):
                if path.is_file():
                    digest.update(path.read_bytes())
            body = digest.hexdigest().encode("ascii")
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        entry = FILES.get(urlsplit(self.path).path)
        if not entry or not entry[0].is_file():
            self.send_error(404)
            return
        body = entry[0].read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", entry[1])
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Project Nexus UI preview server")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=4173)
    args = parser.parse_args()
    ThreadingHTTPServer((args.host, args.port), Handler).serve_forever()
