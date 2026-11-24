"""Serve only the public design preview assets, never the repository."""
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit

class Handler(SimpleHTTPRequestHandler):
    def do_GET(self):
        path = urlsplit(self.path).path
        if path == '/':
            self.path = '/preview/index.html'
        elif path not in ('/preview/index.html', '/assets/nexus-wallpaper.png'):
            self.send_error(404)
            return
        super().do_GET()

    def do_HEAD(self):
        self.send_error(405)

if __name__ == '__main__':
    ThreadingHTTPServer(('0.0.0.0', 4173), Handler).serve_forever()
