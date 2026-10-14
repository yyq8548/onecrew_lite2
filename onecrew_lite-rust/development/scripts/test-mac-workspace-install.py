"""Native customer install/relaunch/plugin/MCP acceptance in an empty profile.

Usage: python3 test-mac-workspace-install.py PACKAGE_DIR /absolute/test/node
Node is used by the development MCP test client only, never by the package.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


if sys.platform != "darwin":
    raise SystemExit("This lane requires a native Apple Silicon Mac.")
package = Path(sys.argv[1]).resolve()
node = Path(sys.argv[2]).resolve()
sdk_test = Path(__file__).with_name("test-workspace-mcp.mjs")
manifest = json.loads((package / "manifest.json").read_text())
assert manifest["target"] == "aarch64-apple-darwin"
expected = {entry["path"]: entry["sha256"] for entry in manifest["files"]}

with tempfile.TemporaryDirectory(prefix="onecrew-mac-install-") as temporary:
    home = Path(temporary).resolve()
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("ONECREW_") and key not in
           ("HOME", "CODEX_HOME", "PORT", "XDG_DATA_HOME", "XDG_CACHE_HOME")}
    # These paths belong only to test children; real user configuration is untouched.
    env.update(HOME=str(home), PATH="/usr/bin:/bin",
               XDG_DATA_HOME=str(home / "data"), XDG_CACHE_HOME=str(home / "cache"))
    primary = home / ".local/bin/onecrew"
    library = home / ".local/bin/library/onecrew"
    legacy_cli = home / ".local/bin/opencli"
    legacy_cli.parent.mkdir(parents=True)
    legacy_bytes = b'#!/bin/sh\ncase "$1" in --version) echo 1.8.6 ;; list) echo "[]" ;; *) exit 88 ;; esac\n'
    legacy_cli.write_bytes(legacy_bytes)
    legacy_cli.chmod(0o755)
    codex_config = home / ".codex/config.toml"
    codex_config.parent.mkdir()
    codex_config.write_text('model = "fixture-model"\n'
                           '[plugins."onecrew-local@onecrew-cli"]\nenabled = true\n'
                           '[plugins."onecrew-local@reacher-local"]\nenabled = false\n'
                           '[plugins."unrelated@fixture"]\nenabled = false\n')

    def run(*arguments, timeout=90):
        result = subprocess.run([str(arg) for arg in arguments], cwd=home, env=env,
                                capture_output=True, text=True, timeout=timeout)
        if result.returncode:
            raise RuntimeError(result.stderr[-1800:])
        return result.stdout

    def launch(entry="安装并打开 OneCrew.command", timeout=90):
        output = run("/bin/sh", package / entry, "--no-browser", timeout=timeout)
        return json.JSONDecoder().raw_decode(output[output.index("{"):])[0]

    try:
        first = launch("首次安装 OneCrew.command", timeout=420)
        assert "onecrew-local@" not in codex_config.read_text()
        assert "unrelated@fixture" in codex_config.read_text()
        assert "fixture-model" in codex_config.read_text()
        managed = json.loads(run(primary, "opencli", "repair", "--json"))
        assert managed["installed"] is True and managed["version"] == "1.8.8"
        assert str(home / ".local/share/onecrew/opencli/managed-v1/") in managed["command_path"]
        assert legacy_cli.read_bytes() == legacy_bytes
        for relative, installed in (("onecrew", primary), ("library/onecrew", library)):
            assert hashlib.sha256(installed.read_bytes()).hexdigest() == expected[relative]
        status = json.loads(run(primary, "status"))
        assert status["login"]["status"]["state"] != "active"
        pid = status["pid"]
        sentinel = home / "Library/Application Support/customer-data-kept.txt"
        sentinel.parent.mkdir(parents=True, exist_ok=True)
        sentinel.write_text("customer data is not part of program installation")
        assert launch() == first
        assert json.loads(run(primary, "status"))["pid"] == pid
        assert sentinel.read_text() == "customer data is not part of program installation"
        plugin = json.loads(run(primary, "plugin", "status", "--refresh"))
        assert plugin["state"] == "installed" and plugin["enabled"] is True
        assert plugin["verification"] == "fresh"
        cached_binary = home / ".codex/plugins/cache/onecrew-cli/onecrew" / plugin["installed_version"] / "bin/onecrew"
        assert cached_binary.is_file()
        assert hashlib.sha256(cached_binary.read_bytes()).hexdigest() == expected["onecrew"]
        print(run(node, sdk_test, cached_binary, timeout=150))
        assert json.loads(run(primary, "status"))["pid"] == pid
        print("PASS: installed workbench/Library, managed OpenCLI over old CLI, retired plugin identities removed, idempotent relaunch, preserved data, fresh Codex plugin and cached-binary MCP workspace/card protocol")
    finally:
        for binary in (primary, library):
            if binary.is_file():
                run(binary, "stop", timeout=35)
