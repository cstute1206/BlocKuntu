#!/usr/bin/env python3
"""Deterministic HTTP fixture for BlocKuntu browser acceptance tests."""

from __future__ import annotations

import argparse
import json
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlsplit


class FixtureServer(ThreadingHTTPServer):
    allow_reuse_address = True
    daemon_threads = True


class FixtureHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "BlocKuntuTestSite/1"

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        self._respond(include_body=True)

    def do_HEAD(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        self._respond(include_body=False)

    def _respond(self, *, include_body: bool) -> None:
        parsed = urlsplit(self.path)
        route = parsed.path

        if route == "/healthz":
            self._send(
                HTTPStatus.OK,
                "application/json; charset=utf-8",
                json.dumps({"status": "ok"}, sort_keys=True).encode(),
                include_body,
                route,
            )
            return

        known_routes = {
            "/",
            "/free",
            "/exact/blocked",
            "/exact/blocked/child",
            "/prefix/blocked",
            "/prefix/blocked/child",
            "/prefix/free",
            "/path/blocked",
            "/path/blocked/child",
            "/path/free",
            "/outside",
            "/allowed",
            "/long-running",
            "/spa",
            "/spa/target",
        }
        if route not in known_routes and route != "/contains":
            self._send(
                HTTPStatus.NOT_FOUND,
                "text/plain; charset=utf-8",
                b"not found\n",
                include_body,
                route,
            )
            return

        query = parse_qs(parsed.query, keep_blank_values=True)
        marker = query.get("marker", [""])[0]
        body = self._page(route, marker).encode()
        self._send(
            HTTPStatus.OK,
            "text/html; charset=utf-8",
            body,
            include_body,
            route,
        )

    def _page(self, route: str, marker: str) -> str:
        host = self.headers.get("Host", "")
        marker_text = marker.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        spa_controls = ""
        if route == "/spa":
            spa_controls = """
              <button id="spa-target" type="button">Navigate with pushState</button>
              <script>
                document.querySelector('#spa-target').addEventListener('click', () => {
                  history.pushState({}, '', '/spa/target');
                  document.querySelector('#route').textContent = '/spa/target';
                });
              </script>
            """
        timer = ""
        if route == "/long-running":
            timer = """
              <p>Elapsed seconds: <output id="elapsed">0</output></p>
              <script>
                const started = Date.now();
                setInterval(() => {
                  document.querySelector('#elapsed').textContent =
                    String(Math.floor((Date.now() - started) / 1000));
                }, 1000);
              </script>
            """
        return f"""<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>BlocKuntu test route {route}</title>
  </head>
  <body>
    <main>
      <h1>BlocKuntu deterministic test site</h1>
      <dl>
        <dt>Host</dt><dd id="host">{host}</dd>
        <dt>Route</dt><dd id="route">{route}</dd>
        <dt>Marker</dt><dd id="marker">{marker_text}</dd>
      </dl>
      <a id="free-link" href="/free">Free route</a>
      {spa_controls}
      {timer}
    </main>
  </body>
</html>
"""

    def _send(
        self,
        status: HTTPStatus,
        content_type: str,
        body: bytes,
        include_body: bool,
        route: str,
    ) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-BlocKuntu-Test-Route", route)
        self.end_headers()
        if include_body:
            self.wfile.write(body)

    def log_message(self, format_string: str, *args: object) -> None:
        message = {
            "client": self.client_address[0],
            "host": self.headers.get("Host", ""),
            "method": self.command,
            "path": self.path,
            "message": format_string % args,
        }
        print(json.dumps(message, sort_keys=True), flush=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=18080)
    parser.add_argument("--ready-file", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    server = FixtureServer((args.bind, args.port), FixtureHandler)
    host, port = server.server_address[:2]
    ready = {"bind": host, "port": port, "status": "ready"}
    if args.ready_file is not None:
        args.ready_file.parent.mkdir(parents=True, exist_ok=True)
        args.ready_file.write_text(json.dumps(ready, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(ready, sort_keys=True), flush=True)
    try:
        server.serve_forever(poll_interval=0.2)
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
