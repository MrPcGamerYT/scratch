"""Serve the design preview (not the emulator) from the repository root."""
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

class Handler(SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/':
            self.path = '/preview/index.html'
        super().do_GET()

if __name__ == '__main__':
    ThreadingHTTPServer(('0.0.0.0', 4173), Handler).serve_forever()
