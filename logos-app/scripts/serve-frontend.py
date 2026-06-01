#!/usr/bin/env python3
"""Serve the React Logos app (logos-app/build) on port 8888 with /api and /ws proxy."""

from __future__ import annotations

import http.server
import os
import select
import socket
import socketserver
import sys
import urllib.error
import urllib.request
from pathlib import Path

PORT = 8888
HOST = "127.0.0.1"
BACKEND_HOST = "127.0.0.1"
BACKEND_PORT = 8080
BACKEND = f"http://{BACKEND_HOST}:{BACKEND_PORT}"
BUILD = Path(__file__).resolve().parent.parent / "build"
DIST = Path(__file__).resolve().parent.parent / "dist"


def spa_root() -> Path:
    """Prefer build/; fall back to dist/ after sync-spa-to-dist."""
    for directory in (BUILD, DIST):
        index = directory / "index.html"
        if index.is_file() and 'id="root"' in index.read_text(encoding="utf-8"):
            return directory
    return BUILD


SPA_ROOT = BUILD


def relay_sockets(src: socket.socket, dst: socket.socket) -> None:
    """Bidirectionally relay bytes between two connected sockets."""
    sockets = [src, dst]
    try:
        while True:
            readable, _, exceptional = select.select(sockets, [], sockets, 60)
            if exceptional:
                break
            if not readable:
                continue
            for sock in readable:
                data = sock.recv(65536)
                if not data:
                    return
                other = dst if sock is src else src
                other.sendall(data)
    except OSError:
        pass
    finally:
        for sock in (src, dst):
            try:
                sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            try:
                sock.close()
            except OSError:
                pass


def proxy_websocket(client: socket.socket, path: str, headers) -> None:
    """Forward a WebSocket upgrade request to the Go backend and relay frames."""
    backend = socket.create_connection((BACKEND_HOST, BACKEND_PORT), timeout=30)

    lines = [f"GET {path} HTTP/1.1"]
    for key, value in headers.items():
        if key.lower() == "host":
            continue
        lines.append(f"{key}: {value}")
    lines.append(f"Host: {BACKEND_HOST}:{BACKEND_PORT}")
    lines.append("Connection: Upgrade")
    request = "\r\n".join(lines) + "\r\n\r\n"
    backend.sendall(request.encode("latin-1"))

    response = b""
    while b"\r\n\r\n" not in response:
        chunk = backend.recv(4096)
        if not chunk:
            backend.close()
            client.close()
            return
        response += chunk

    client.sendall(response)
    if b" 101 " not in response.split(b"\r\n", 1)[0]:
        backend.close()
        client.close()
        return

    relay_sockets(client, backend)


class FrontendHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(SPA_ROOT), **kwargs)

    def end_headers(self) -> None:
        path_only = self.path.split("?", 1)[0]
        logos_override = (
            "logos=" in self.path
            or "/logos-" in path_only
            or path_only.endswith("/logos-theme.css")
        )
        if logos_override:
            self.send_header("Cache-Control", "no-cache")
        elif "version=" in self.path or path_only.startswith(("/js/", "/css/", "/fonts/")):
            self.send_header("Cache-Control", "public, max-age=31536000, immutable")
        super().end_headers()

    def log_message(self, fmt: str, *args) -> None:
        sys.stdout.write("%s - %s\n" % (self.address_string(), fmt % args))

    def _is_websocket(self) -> bool:
        return self.headers.get("Upgrade", "").lower() == "websocket"

    def _proxy_websocket(self) -> None:
        proxy_websocket(self.connection, self.path, self.headers)

    def _proxy_backend(self) -> None:
        url = BACKEND + self.path
        body = None
        if self.command in ("POST", "PUT", "PATCH", "DELETE"):
            length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(length) if length else None

        req = urllib.request.Request(
            url,
            data=body,
            method=self.command,
        )
        for key, value in self.headers.items():
            if key.lower() in ("host", "connection", "content-length"):
                continue
            req.add_header(key, value)

        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                self.send_response(resp.status)
                for key, value in resp.headers.items():
                    if key.lower() in ("transfer-encoding", "connection"):
                        continue
                    self.send_header(key, value)
                self.end_headers()
                self.wfile.write(resp.read())
        except urllib.error.HTTPError as err:
            self.send_response(err.code)
            for key, value in err.headers.items():
                if key.lower() in ("transfer-encoding", "connection"):
                    continue
                self.send_header(key, value)
            self.end_headers()
            self.wfile.write(err.read())
        except Exception as exc:  # noqa: BLE001
            body = f'{{"type":"error","hint":"{exc}"}}'.encode()
            self.send_response(502)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    def _proxy_api(self) -> None:
        self._proxy_backend()

    def _penpot_shell_cookie(self) -> bool:
        """Set by workspace.html when the Penpot editor is active in this tab."""
        return "logos-penpot-shell=1" in self.headers.get("Cookie", "")

    def _spa_path(self) -> str:
        """Return index.html for client-side routes (React Router)."""
        path_only = self.path.split("?", 1)[0]
        if path_only in ("", "/"):
            # Reload of http://host:8888/#/workspace?… only requests GET / (hash is not sent).
            # Serve the Penpot shell directly when this tab previously opened the editor.
            if self._penpot_shell_cookie():
                return "/workspace.html"
            return self.path
        if "." in Path(path_only).name:
            return self.path
        return "/index.html"

    def _proxy_backend_assets(self) -> bool:
        """Backend storage URLs only — not Vite's /assets/*.js bundles."""
        path_only = self.path.split("?", 1)[0]
        return path_only.startswith("/assets/by-id/")

    def do_GET(self) -> None:
        if self.path.startswith("/ws/") and self._is_websocket():
            self._proxy_websocket()
            return
        if self.path.startswith("/api/") or self._proxy_backend_assets():
            self._proxy_backend()
            return
        self.path = self._spa_path()
        super().do_GET()

    def do_HEAD(self) -> None:
        if self.path.startswith("/api/") or self._proxy_backend_assets():
            self._proxy_backend()
            return
        self.path = self._spa_path()
        super().do_HEAD()

    def do_OPTIONS(self) -> None:
        if self.path.startswith("/api/"):
            self._proxy_api()
            return
        self.send_response(204)
        self.end_headers()

    def do_POST(self) -> None:
        if self.path.startswith("/api/"):
            self._proxy_api()
            return
        self.send_response(405)
        self.end_headers()

    def do_PUT(self) -> None:
        if self.path.startswith("/api/"):
            self._proxy_api()
            return
        self.send_response(405)
        self.end_headers()

    def do_PATCH(self) -> None:
        if self.path.startswith("/api/"):
            self._proxy_api()
            return
        self.send_response(405)
        self.end_headers()

    def do_DELETE(self) -> None:
        if self.path.startswith("/api/"):
            self._proxy_api()
            return
        self.send_response(405)
        self.end_headers()


if __name__ == "__main__":
    SPA_ROOT = spa_root()
    if not SPA_ROOT.is_dir() or not (SPA_ROOT / "index.html").is_file():
        sys.stderr.write(
            "Missing React build. Run from logos-app:\n"
            "  npm run build:spa\n"
            "Or use the dev server instead:\n"
            "  npm run dev\n"
        )
        sys.exit(1)

    os.chdir(SPA_ROOT)
    socketserver.ThreadingTCPServer.allow_reuse_address = True
    try:
        httpd = socketserver.ThreadingTCPServer((HOST, PORT), FrontendHandler)
    except OSError as err:
        if err.errno == 98:  # EADDRINUSE
            sys.stderr.write(
                f"Port {PORT} is already in use.\n"
                f"Stop the other server first, e.g.:\n"
                f"  kill $(ss -tlnp | grep ':{PORT}' | grep -oP 'pid=\\K[0-9]+')\n"
                f"Or:  pkill -f serve-frontend.py\n"
            )
        else:
            sys.stderr.write(f"Failed to bind {HOST}:{PORT}: {err}\n")
        sys.exit(1)

    with httpd:
        print(f"Logos app:  http://{HOST}:{PORT}/  (from {SPA_ROOT.name}/)")
        print(f"API proxy:  {BACKEND}")
        print(f"WS proxy:   ws://{BACKEND_HOST}:{BACKEND_PORT}/ws/")
        httpd.serve_forever()
