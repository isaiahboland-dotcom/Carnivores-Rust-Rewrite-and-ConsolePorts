"""Local review server for one audit: static files plus a writable reviews.json.

Binds to the loopback interface only. Human reviews are the single file this
server writes; capture results stay read-only.
"""
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path

LIMIT = 16 * 1024 * 1024


class Handler(SimpleHTTPRequestHandler):
    def do_PUT(self):
        length = int(self.headers.get('Content-Length') or 0)
        if self.path.split('?')[0] != '/reviews.json' or not 0 < length <= LIMIT:
            return self.send_error(403)
        try:
            saved = json.loads(self.rfile.read(length))
            if not isinstance(saved.get('reviews'), dict):
                raise ValueError('reviews must be an object')
        except (ValueError, AttributeError):
            return self.send_error(400)
        target = Path(self.directory) / 'reviews.json'
        temporary = target.with_suffix('.json.tmp')
        temporary.write_text(json.dumps(saved, indent=2) + '\n')
        temporary.replace(target)
        self.send_response(204)
        self.end_headers()

    def end_headers(self):
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()

    def log_message(self, *args):
        pass


def server(directory, port=0):
    return ThreadingHTTPServer(('127.0.0.1', port), partial(Handler, directory=str(directory)))


def serve(directory, port=0):
    httpd = server(directory, port)
    print(f'Reviewing {directory.name}: http://127.0.0.1:{httpd.server_address[1]}/report.html (Ctrl-C stops)', flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        httpd.server_close()
