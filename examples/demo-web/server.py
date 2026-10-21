#!/usr/bin/env python3
"""Minimal web server that refuses to be embedded, used to demonstrate level 1."""

import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PAGE = """<!doctype html><html lang="en"><meta charset="utf-8"><title>Demo Web</title>
<style>body{font:14px system-ui;padding:20px} code{background:#8882;padding:2px 4px}</style>
<h2>Web interface of a tool</h2>
<p>This server sends <code>X-Frame-Options: DENY</code> and
<code>frame-ancestors 'none'</code>, yet it is shown inside tuxly.</p>
<p>Result of <code>fetch('/api/time')</code>: <b id="t">…</b></p>
<script>
fetch('/api/time').then(r => r.json()).then(d => document.getElementById('t').textContent = d.time)
  .catch(e => document.getElementById('t').textContent = 'Error: ' + e);
</script></html>"""


class Handler(BaseHTTPRequestHandler):
    def send(self, status, body, ctype):
        data = body.encode()
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("X-Frame-Options", "DENY")
        self.send_header("Content-Security-Policy", "frame-ancestors 'none'")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/api/time":
            self.send(200, json.dumps({"time": time.strftime("%H:%M:%S")}), "application/json")
        elif self.path == "/":
            self.send(200, PAGE, "text/html; charset=utf-8")
        else:
            self.send(404, "not found", "text/plain")

    def log_message(self, *args):
        pass


ThreadingHTTPServer(("127.0.0.1", 47811), Handler).serve_forever()
