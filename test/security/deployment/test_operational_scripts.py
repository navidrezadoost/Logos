"""Offline safety tests for the P0.5 operational security scripts."""

import contextlib
import hashlib
import base64
import io
import json
import os
from pathlib import Path
import ssl
import socket
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import urllib.parse

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
RUNNER = HERE / "run_disposable_security_tests.sh"
sys.path.insert(0, str(HERE))
import probe_ws_origins as probe  # noqa: E402


class OperationalScriptSafetyTests(unittest.TestCase):
    def test_rejected_runner_inputs_block_before_schema_init_or_go_test(self):
        base = {
            "TEST_DATABASE_URL": "postgres://logos:PG_SECRET@127.0.0.1:5432/logos_p04_test?sslmode=disable",
            "TEST_REDIS_URL": "redis://:REDIS_SECRET@127.0.0.1:6379/0",
            "TEST_REDIS_DISPOSABLE": "true",
            "DEPLOYMENT_ENVIRONMENT": "nonproduction",
            "DEPLOYMENT_OWNER_AUTHORIZED": "true",
        }
        cases = [
            ("missing owner authorization", "DEPLOYMENT_OWNER_AUTHORIZED", None,
             "BLOCKED: DEPLOYMENT_OWNER_AUTHORIZED confirmation is required"),
            ("incorrect environment", "DEPLOYMENT_ENVIRONMENT", "production",
             "BLOCKED: DEPLOYMENT_ENVIRONMENT must be nonproduction"),
            ("remote PostgreSQL host", "TEST_DATABASE_URL",
             "postgres://logos:PG_SECRET@db.example.test:5432/logos_p04_test",
             "BLOCKED: PostgreSQL test endpoint host must be loopback-only"),
            ("remote Redis host", "TEST_REDIS_URL", "redis://redis.example.test:6379/0",
             "BLOCKED: Redis test endpoint host must be loopback-only"),
            ("unsupported PostgreSQL scheme", "TEST_DATABASE_URL",
             "https://logos:PG_SECRET@127.0.0.1:5432/logos_p04_test",
             "BLOCKED: TEST_DATABASE_URL scheme must be postgres or postgresql"),
            ("unsupported Redis scheme", "TEST_REDIS_URL", "https://127.0.0.1/0",
             "BLOCKED: TEST_REDIS_URL scheme must be redis or rediss"),
            ("invalid database name", "TEST_DATABASE_URL",
             "postgres://logos:PG_SECRET@127.0.0.1:5432/shared",
             "BLOCKED: PostgreSQL database name must begin with logos_p04"),
            ("invalid database path", "TEST_DATABASE_URL",
             "postgres://logos:PG_SECRET@127.0.0.1:5432/logos_p04_test/other",
             "BLOCKED: PostgreSQL database name must begin with logos_p04"),
            ("missing disposable Redis confirmation", "TEST_REDIS_DISPOSABLE", None,
             "BLOCKED: TEST_REDIS_DISPOSABLE confirmation is required"),
            ("false disposable Redis confirmation", "TEST_REDIS_DISPOSABLE", "false",
             "BLOCKED: TEST_REDIS_DISPOSABLE must be true"),
            ("malformed PostgreSQL port", "TEST_DATABASE_URL",
             "postgres://logos:PG_SECRET@127.0.0.1:notaport/logos_p04_test",
             "BLOCKED: TEST_DATABASE_URL port is malformed"),
        ]

        for label, key, value, expected in cases:
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary:
                marker = Path(temporary) / "unsafe-command-ran"
                wrappers = Path(temporary) / "bin"
                wrappers.mkdir()
                for command in ("bash", "go"):
                    wrapper = wrappers / command
                    wrapper.write_text(
                        "#!/bin/sh\nprintf invoked > \"$CALL_MARKER\"\nexit 93\n",
                        encoding="utf-8",
                    )
                    wrapper.chmod(0o755)

                env = {
                    "PATH": f"{wrappers}{os.pathsep}{os.environ['PATH']}",
                    "CALL_MARKER": str(marker),
                    **base,
                }
                if value is None:
                    env.pop(key)
                else:
                    env[key] = value

                result = subprocess.run(
                    ["/bin/bash", str(RUNNER)], cwd=ROOT, env=env,
                    capture_output=True, text=True, check=False,
                )

                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertEqual(result.stderr.strip(), expected)
                self.assertFalse(marker.exists(), "schema initialization or go test was invoked")
                self.assertNotIn("PG_SECRET", result.stdout + result.stderr)
                self.assertNotIn("REDIS_SECRET", result.stdout + result.stderr)

    def test_probe_rejects_malformed_endpoint_cookie_and_origin(self):
        valid = {
            "LOGOS_WS_URL": "wss://editor.example/ws/notifications?session-id=00000000-0000-4000-8000-000000000001",
            "AUTHORIZED_BROWSER_ORIGIN": "https://editor.example",
            "LOGOS_WS_COOKIE": "sid=COOKIE_SECRET",
            "DEPLOYMENT_ENVIRONMENT": "nonproduction",
            "DEPLOYMENT_OWNER_AUTHORIZED": "true",
        }
        invalid_values = [
            ("endpoint", "LOGOS_WS_URL", "https://editor.example/ws/notifications"),
            ("endpoint path", "LOGOS_WS_URL", "wss://editor.example/not-websocket"),
            ("cookie", "LOGOS_WS_COOKIE", "sid=COOKIE_SECRET; injected=1"),
            ("origin", "AUTHORIZED_BROWSER_ORIGIN", "null"),
            ("origin path", "AUTHORIZED_BROWSER_ORIGIN", "https://editor.example/private"),
            ("origin whitespace", "AUTHORIZED_BROWSER_ORIGIN", "https://editor example"),
            ("origin malformed port", "AUTHORIZED_BROWSER_ORIGIN", "https://editor.example:invalid"),
        ]
        for label, key, value in invalid_values:
            with self.subTest(label=label, value=value), mock.patch.dict(os.environ, valid, clear=True):
                os.environ[key] = value
                with self.assertRaises(ValueError):
                    probe.config_from_env("reverse-proxy")

    def test_invalid_probe_input_emits_sanitized_blocked_status(self):
        env = {
            "LOGOS_WS_URL": "wss://endpoint-secret.example/ws/notifications?session-id=00000000-0000-4000-8000-000000000001",
            "AUTHORIZED_BROWSER_ORIGIN": "https://origin-secret.example/path",
            "LOGOS_WS_COOKIE": "sid=COOKIE_SECRET",
            "DEPLOYMENT_ENVIRONMENT": "nonproduction",
            "DEPLOYMENT_OWNER_AUTHORIZED": "true",
        }
        stdout = io.StringIO()
        with mock.patch.dict(os.environ, env, clear=True), \
             mock.patch.object(sys, "argv", ["probe_ws_origins.py", "--mode", "reverse-proxy"]), \
             contextlib.redirect_stdout(stdout):
            self.assertEqual(probe.main(), 2)
        output = stdout.getvalue()
        self.assertEqual(json.loads(output)["status"], "BLOCKED")
        for secret in ("endpoint-secret", "origin-secret", "COOKIE_SECRET", "session-id"):
            self.assertNotIn(secret, output)

    def test_probe_requires_owner_authorization_and_nonproduction_classification(self):
        valid = {
            "LOGOS_WS_URL": "wss://editor.example/ws/notifications?session-id=00000000-0000-4000-8000-000000000001",
            "AUTHORIZED_BROWSER_ORIGIN": "https://editor.example",
            "LOGOS_WS_COOKIE": "sid=COOKIE_SECRET",
            "DEPLOYMENT_ENVIRONMENT": "nonproduction",
            "DEPLOYMENT_OWNER_AUTHORIZED": "true",
        }
        for key, value in (("DEPLOYMENT_OWNER_AUTHORIZED", "false"),
                           ("DEPLOYMENT_ENVIRONMENT", "production")):
            with self.subTest(key=key), mock.patch.dict(os.environ, valid, clear=True):
                os.environ[key] = value
                with self.assertRaises(ValueError):
                    probe.config_from_env("reverse-proxy")

    def test_origin_cases_have_exact_authorized_and_denied_values(self):
        origin = urllib.parse.urlsplit("https://editor.example:8443")
        self.assertEqual(probe.cases_for(origin), [
            ("authorized_origin", "https://editor.example:8443", 101),
            ("missing_origin", None, 403),
            ("malformed_origin", "not-an-origin", 403),
            ("opaque_origin", "null", 403),
            ("unauthorized_origin", "https://origin-probe-denied.invalid", 403),
            ("scheme_mismatch", "http://editor.example:8443", 403),
            ("port_mismatch", "https://editor.example:8444", 403),
        ])

    def test_upgrade_response_validation_checks_status_headers_and_accept_key(self):
        key = "dGhlIHNhbXBsZSBub25jZQ=="
        accept = base64.b64encode(hashlib.sha1(
            (key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode("ascii")
        ).digest()).decode("ascii")
        headers = {
            "upgrade": "websocket",
            "connection": "keep-alive, Upgrade",
            "sec-websocket-accept": accept,
        }
        self.assertEqual(probe.validate_upgrade_response(b"HTTP/1.1 101 Switching Protocols\r\n", headers, key), (101, True))
        self.assertEqual(probe.validate_upgrade_response(b"HTTP/1.1 403 Forbidden\r\n", headers, key), (403, False))
        self.assertEqual(probe.validate_upgrade_response(b"not HTTP\r\n", headers, key), (None, False))
        bad_headers = {**headers, "sec-websocket-accept": "wrong"}
        self.assertEqual(probe.validate_upgrade_response(b"HTTP/1.1 101 Switching Protocols\r\n", bad_headers, key), (101, False))

    def test_dns_tcp_tls_and_http_upgrade_failures_are_safe(self):
        failures = [
            socket.gaierror("DNS_SECRET"),
            ConnectionRefusedError("TCP_SECRET"),
            ssl.SSLError("TLS_SECRET"),
            (502, False),
        ]
        for failure in failures:
            with self.subTest(failure=type(failure).__name__):
                stdout = io.StringIO()
                network_mock = (mock.patch.object(probe, "perform_upgrade", return_value=failure)
                                if isinstance(failure, tuple)
                                else mock.patch.object(probe, "perform_upgrade", side_effect=failure))
                with mock.patch.object(probe, "config_from_env", return_value=(
                        urllib.parse.urlsplit("wss://editor.example/ws/notifications?session-id=00000000-0000-4000-8000-000000000001"),
                        urllib.parse.urlsplit("https://editor.example"),
                        "sid=COOKIE_SECRET")), \
                     network_mock, \
                     mock.patch.object(sys, "argv", ["probe_ws_origins.py", "--mode", "reverse-proxy"]), \
                     contextlib.redirect_stdout(stdout):
                    result = probe.main()
                self.assertEqual(result, 1)
                output = stdout.getvalue()
                report = json.loads(output)
                self.assertEqual(report["status"], "FAILED")
                self.assertNotIn("DNS_SECRET", output)
                self.assertNotIn("TCP_SECRET", output)
                self.assertNotIn("TLS_SECRET", output)
                self.assertNotIn("COOKIE_SECRET", output)
                self.assertEqual(len(report["results"]), 7)

    def test_failed_probe_report_is_secret_free_and_private(self):
        with tempfile.TemporaryDirectory() as temporary:
            report_path = Path(temporary) / "evidence.json"
            stdout = io.StringIO()
            with mock.patch.object(probe, "config_from_env", return_value=(
                    urllib.parse.urlsplit("wss://editor.example/ws/notifications?session-id=00000000-0000-4000-8000-000000000001"),
                    urllib.parse.urlsplit("https://editor.example"),
                    "sid=COOKIE_SECRET")), \
                 mock.patch.object(probe, "perform_upgrade", side_effect=OSError("NETWORK_SECRET")), \
                 mock.patch.object(sys, "argv", ["probe_ws_origins.py", "--mode", "direct", "--output", str(report_path)]), \
                 contextlib.redirect_stdout(stdout):
                self.assertEqual(probe.main(), 1)

            self.assertEqual(report_path.stat().st_mode & 0o777, 0o600)
            for output in (stdout.getvalue(), report_path.read_text(encoding="utf-8")):
                for secret in ("COOKIE_SECRET", "NETWORK_SECRET", "editor.example", "session-id"):
                    self.assertNotIn(secret, output)


if __name__ == "__main__":
    unittest.main(verbosity=2)
