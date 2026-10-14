#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Exercise the real CLI against a local synthetic login service; no browser or Access calls."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading


def check(binary: Path, expected_documents: int):
    with tempfile.TemporaryDirectory(prefix="onecrew-login-output-") as directory:
        root = Path(directory)
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(0.2)
        token = "synthetic-local-test-token"
        endpoint = root / ".onecrew-service.json"
        endpoint.write_text(json.dumps(dict(protocol=1, version="0.5.19",
            binary_id=hashlib.sha256(binary.read_bytes()).hexdigest(), pid=os.getpid(),
            port=listener.getsockname()[1], web_port=1, token=token)))
        endpoint.chmod(0o600)
        commands = []
        done = threading.Event()

        def serve():
            while not done.is_set():
                try:
                    stream, _ = listener.accept()
                except socket.timeout:
                    continue
                with stream, stream.makefile("rb") as reader:
                    request = json.loads(reader.readline())
                    assert request["token"] == token
                    command = request["command"]
                    commands.append(command)
                    if command == "status":
                        active = "login" in commands
                        result = {"running": True, "version": "0.5.19", "plugin": {"state": "unknown"},
                            "login": {"status": {"state": "active" if active else "pairing_required"}}}
                    elif command == "login":
                        result = {"authorize_url": "https://example.invalid/synthetic-login",
                            "projection": {"browser_opened": False, "status": {"state": "pairing_pending"}}}
                    elif command == "login-status":
                        result = {"status": {"state": "active" if commands.count(command) > 1 else "pairing_pending"}}
                    else:
                        assert command in ("ping", "browser-opened")
                        result = {}
                    stream.sendall((json.dumps({"ok": True, "result": result}) + "\n").encode())

        thread = threading.Thread(target=serve, daemon=True)
        thread.start()
        # Only this child process sees the fake browser opener.
        opener = root / "open"
        opener.write_text("#!/bin/sh\nexit 0\n")
        opener.chmod(0o755)
        try:
            result = subprocess.run([str(binary), "login", "--json", "--data-dir", str(root)],
                env={**os.environ, "PATH": str(root) + os.pathsep + os.environ.get("PATH", "")},
                capture_output=True, text=True, timeout=15)
        finally:
            done.set()
            thread.join(timeout=2)
            listener.close()
        assert result.returncode == 0, result.stderr
        decoder, remaining, documents = json.JSONDecoder(), result.stdout.lstrip(), []
        while remaining:
            value, end = decoder.raw_decode(remaining)
            documents.append(value)
            remaining = remaining[end:].lstrip()
        assert len(documents) == expected_documents, f"expected {expected_documents} JSON result(s), got {len(documents)}"
        assert documents[-1]["logged_in"] is True
        assert commands.count("login") == 1
        assert "browser-opened" in commands
        if expected_documents == 1:
            assert "authorize_url" not in result.stdout
            assert "synthetic-login" not in result.stdout + result.stderr
            assert "pairing_pending" not in result.stdout
        print(json.dumps({"json_documents": len(documents), "logged_in": True, "login_requests": commands.count("login")}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--expect-documents", type=int, default=1)
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("this native browser-opener regression runs on macOS only")
    check(args.binary.resolve(), args.expect_documents)
