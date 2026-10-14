"""Run a real locally built OneCrew binary against isolated update/plugin fixtures.

macOS: python3 development/installers/test_update_e2e.py --binary /absolute/debug/onecrew
No release endpoint, real service, user credential, or real Codex plugin is used.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile


def sha(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def tree_hash(path):
    return {
        str(file.relative_to(path)): sha(file)
        for file in sorted(path.rglob("*")) if file.is_file()
    }


def receipt(state):
    return {
        "state": state,
        "verification": "cached",
        "installed_version": "0.5.21+codex." + "a" * 64 if state == "installed" else None,
        "enabled": True if state == "installed" else None,
        "last_verified_at": 1,
        "diagnostic_code": None,
    }


def run_case(binary, version, state, current=False, failure=None):
    with tempfile.TemporaryDirectory(prefix="onecrew-update-e2e-") as directory:
        root = Path(directory).resolve()
        home = root / "home"
        codex_home = home / ".codex"
        codex_home.mkdir(parents=True)
        cache_key = hashlib.sha256(os.fsencode(codex_home.resolve())).hexdigest()
        cache = home / ".local/share/onecrew/codex-plugin" / cache_key / "onecrew"
        cache.mkdir(parents=True)
        receipt_path = cache / "status.json"
        receipt_path.write_text(json.dumps(receipt(state)), encoding="utf-8")
        receipt_path.chmod(0o600)
        plugin = codex_home / "plugins/cache/fixture-plugin/unchanged"
        plugin.mkdir(parents=True)
        (plugin / "plugin.json").write_text('{"fixture":"do not change"}', encoding="utf-8")
        (plugin / "mcp.txt").write_text("unchanged plugin bytes", encoding="utf-8")
        fake_bin = root / "fake-bin"
        fake_bin.mkdir()
        calls = root / "codex-calls.log"
        calls.write_bytes(b"")
        fake_codex = fake_bin / "codex"
        fake_codex.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$CODEX_CALL_LOG"\nexit 79\n')
        fake_codex.chmod(0o755)
        data = root / "isolated-data"
        data.mkdir()
        # Opaque sentinels prove bootstrap does not open, recreate, or migrate data.
        for name in ("agent-platform.db", "vault.key", "crm.db"):
            (data / name).write_bytes(("preserve-" + name).encode())
        if failure == "service":
            endpoint = data / ".onecrew-service.json"
            endpoint.write_bytes(b"invalid endpoint fixture")
            endpoint.chmod(0o600)
        installed = root / "installation/onecrew"
        installed.parent.mkdir()
        if current:
            shutil.copy2(binary, installed)
        else:
            installed.write_bytes(b"existing-binary-must-be-preserved-on-failure")
        before_binary = sha(installed)
        before_plugin = tree_hash(codex_home)
        before_receipts = tree_hash(cache)
        before_data = {name: sha(data / name) for name in ("agent-platform.db", "vault.key", "crm.db")}
        env = dict(os.environ, HOME=str(home), CODEX_HOME=str(codex_home),
                   PATH=f"{fake_bin}:/usr/bin:/bin:/usr/sbin:/sbin", CODEX_CALL_LOG=str(calls),
                   HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1")
        expected_hash = "0" * 64 if failure == "hash" else sha(binary)
        command = [str(binary), "__bootstrap-update", "--install-path", str(installed),
                   "--expected-version", version, "--expected-sha256", expected_hash,
                   "--expected-size", str(binary.stat().st_size), "--data-dir", str(data), "--json"]
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=90)
        assert calls.read_bytes() == b"", f"Codex was invoked: {calls.read_text()}"
        assert tree_hash(codex_home) == before_plugin, "plugin fixture changed"
        assert tree_hash(cache) == before_receipts, "receipt bytes changed"
        assert {name: sha(data / name) for name in before_data} == before_data, "login/data changed"
        if failure:
            assert result.returncode != 0, result.stdout
            assert sha(installed) == before_binary, "failed update replaced old binary"
            return {"case": failure, "preserved": True, "codex_calls": 0}
        assert result.returncode == 0, result.stderr
        output = json.loads(result.stdout)
        assert output["action"] == ("already_current" if current else "updated"), output
        assert sha(installed) == sha(binary), "new binary was not installed"
        expected = {"installed": "plugin_update_required", "unknown": "unknown", "not_installed": "optional"}[state]
        guidance = output["plugin"]
        assert guidance["compatibility"] == expected, output
        if state == "installed":
            assert guidance["next_action"]["args"][:2] == ["plugin", "install"]
        elif state == "unknown":
            assert guidance["next_action"]["args"][:3] == ["plugin", "status", "--refresh"]
        else:
            assert guidance["next_action"] is None
            assert guidance["optional_action"]["args"][:2] == ["plugin", "install"]
        assert str(installed) == output["installed_path"]
        return {"case": output["action"], "plugin": expected, "codex_calls": 0, "plugin_and_receipt_unchanged": True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("This native E2E requires macOS. Windows swap logic is covered separately by Rust unit tests.")
    binary = args.binary.resolve(strict=True)
    reported = subprocess.check_output([str(binary), "--version"], text=True).strip()
    assert reported.startswith("onecrew "), reported
    version = reported.removeprefix("onecrew ")
    results = [run_case(binary, version, state, current) for state in ("installed", "unknown", "not_installed") for current in (False, True)]
    results.extend(run_case(binary, version, "installed", failure=failure) for failure in ("hash", "service"))
    print(json.dumps({"status": "passed", "cases": results}, indent=2))


if __name__ == "__main__":
    main()
