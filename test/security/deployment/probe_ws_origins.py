#!/usr/bin/env python3
"""Non-production authenticated WebSocket Origin policy probe.

The report contains case names and HTTP status codes only. It never prints the
target URL, Origin strings, cookie, headers, or transport exception text.
"""

import argparse
import base64
import hashlib
import json
import os
import re
import socket
import ssl
import sys
import urllib.parse
import uuid
from pathlib import Path


def origin_parts(value):
    if not value or value != value.strip() or any(ord(char) <= 0x20 or ord(char) == 0x7f for char in value):
        raise ValueError("invalid authorized origin")
    parsed = urllib.parse.urlsplit(value)
    if (parsed.scheme != "https" or not parsed.hostname or parsed.username or
            parsed.password or parsed.path not in ("", "/") or parsed.query or parsed.fragment):
        raise ValueError("invalid authorized origin")
    try:
        parsed.port
    except ValueError as error:
        raise ValueError("invalid authorized origin") from error
    return parsed


def config_from_env(mode):
    ws_value = os.environ.get("LOGOS_WS_URL", "")
    ws = urllib.parse.urlsplit(ws_value)
    if (ws.scheme != "wss" or not ws.hostname or ws.username or ws.password or
            ws.fragment or ws.path != "/ws/notifications"):
        raise ValueError("invalid WebSocket endpoint")
    params = urllib.parse.parse_qs(ws.query, strict_parsing=True)
    if set(params) != {"session-id"} or len(params["session-id"]) != 1:
        raise ValueError("endpoint must contain only one session-id query parameter")
    uuid.UUID(params["session-id"][0])

    origin_value = os.environ.get("AUTHORIZED_BROWSER_ORIGIN", "")
    origin = origin_parts(origin_value)
    cookie = os.environ.get("LOGOS_WS_COOKIE", "")
    if not cookie or "\r" in cookie or "\n" in cookie or ";" in cookie:
        raise ValueError("invalid session cookie input")
    if not re.fullmatch(r"[!#$%&'*+.^_`|~0-9A-Za-z-]+=[^\s;]+", cookie):
        raise ValueError("invalid session cookie input")
    if os.environ.get("DEPLOYMENT_ENVIRONMENT") != "nonproduction":
        raise ValueError("nonproduction environment confirmation is required")
    if os.environ.get("DEPLOYMENT_OWNER_AUTHORIZED") != "true":
        raise ValueError("deployment-owner authorization is required")

    if mode == "direct":
        ws_port = ws.port or 443
        origin_port = origin.port or 443
        if ws.hostname.lower().rstrip(".") != origin.hostname.lower().rstrip(".") or ws_port != origin_port:
            raise ValueError("direct HTTPS mode requires the WebSocket host and allowed Origin to match")
    return ws, origin, cookie


def perform_upgrade(ws, cookie, origin_header):
    port = ws.port or 443
    host = ws.hostname
    if ":" in host and not host.startswith("["):
        host = f"[{host}]"
    host_header = host if port == 443 else f"{host}:{port}"
    target = urllib.parse.urlunsplit(("", "", ws.path, ws.query, ""))
    key = base64.b64encode(os.urandom(16)).decode("ascii")
    request = [
        f"GET {target} HTTP/1.1",
        f"Host: {host_header}",
        "Upgrade: websocket",
        "Connection: Upgrade",
        f"Sec-WebSocket-Key: {key}",
        "Sec-WebSocket-Version: 13",
        f"Cookie: {cookie}",
    ]
    if origin_header is not None:
        if "\r" in origin_header or "\n" in origin_header:
            raise ValueError("invalid generated Origin")
        request.append(f"Origin: {origin_header}")
    raw = ("\r\n".join(request) + "\r\n\r\n").encode("ascii")

    with socket.create_connection((ws.hostname, port), timeout=8) as conn:
        conn.settimeout(8)
        with ssl.create_default_context().wrap_socket(conn, server_hostname=ws.hostname) as secure:
            secure.sendall(raw)
            stream = secure.makefile("rb")
            status_line = stream.readline(4096)
            line = status_line
            headers = {}
            while line and line not in (b"\r\n", b"\n"):
                header_line = stream.readline(8192)
                if not header_line:
                    break
                name, separator, value = header_line.partition(b":")
                if separator:
                    headers[name.decode("ascii", "ignore").strip().lower()] = value.decode("ascii", "ignore").strip()
                line = header_line

    return validate_upgrade_response(status_line, headers, key)


def validate_upgrade_response(status_line, headers, key):
    match = re.match(rb"HTTP/1\.[01] ([0-9]{3})(?:\s|\r|\n)", status_line)
    status = int(match.group(1)) if match else None
    expected_accept = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode("ascii")).digest()).decode("ascii")
    connection_tokens = {token.strip().lower() for token in headers.get("connection", "").split(",")}
    valid_upgrade = (status == 101 and headers.get("upgrade", "").lower() == "websocket" and
                     "upgrade" in connection_tokens and headers.get("sec-websocket-accept") == expected_accept)
    return status, valid_upgrade


def cases_for(origin):
    scheme = origin.scheme
    hostname = origin.hostname
    port = origin.port
    authority = origin.netloc
    other_port = (port + 1) if port else 444
    if other_port == 443:
        other_port = 444
    wrong_port_authority = f"{hostname}:{other_port}"
    return [
        ("authorized_origin", f"{scheme}://{authority}", 101),
        ("missing_origin", None, 403),
        ("malformed_origin", "not-an-origin", 403),
        ("opaque_origin", "null", 403),
        ("unauthorized_origin", f"{scheme}://origin-probe-denied.invalid", 403),
        ("scheme_mismatch", f"{'http' if scheme == 'https' else 'https'}://{authority}", 403),
        ("port_mismatch", f"{scheme}://{wrong_port_authority}", 403),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("direct", "reverse-proxy"), required=True)
    parser.add_argument("--output", help="optional sanitized JSON evidence path")
    args = parser.parse_args()
    try:
        ws, origin, cookie = config_from_env(args.mode)
    except Exception:
        print(json.dumps({"status": "BLOCKED", "reason": "required non-production endpoint, owner authorization, or credential input is invalid"}))
        return 2

    results = []
    for name, origin_header, expected in cases_for(origin):
        try:
            observed, valid_upgrade = perform_upgrade(ws, cookie, origin_header)
        except Exception:
            observed, valid_upgrade = None, False
        results.append({"case": name, "expected_status": expected,
                        "observed_status": observed,
                        "valid_websocket_upgrade": valid_upgrade if expected == 101 else None,
                        "passed": observed == expected and (expected != 101 or valid_upgrade)})
    report = {"mode": args.mode,
              "status": "PASSED" if all(row["passed"] for row in results) else "FAILED",
              "results": results}
    serialized = json.dumps(report, indent=2) + "\n"
    if args.output:
        output_path = Path(args.output)
        descriptor = os.open(output_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        try:
            os.fchmod(descriptor, 0o600)
            with os.fdopen(descriptor, "w", encoding="utf-8") as evidence_file:
                descriptor = -1
                evidence_file.write(serialized)
        finally:
            if descriptor >= 0:
                os.close(descriptor)
    print(serialized, end="")
    return 0 if report["status"] == "PASSED" else 1


if __name__ == "__main__":
    sys.exit(main())
