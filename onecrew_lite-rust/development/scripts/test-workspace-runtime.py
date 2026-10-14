"""Native local-package acceptance with fresh profiles and no Node in PATH.

Usage: python3 test-workspace-runtime.py /absolute/workspace-package-directory
Does not use the user's data or credentials and does not contact providers.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request


if sys.platform != "darwin":
    raise SystemExit("This acceptance script isolates macOS profiles only; Windows requires its native lane.")
package = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix="onecrew-workspace-native-") as temporary:
    home = Path(temporary)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("ONECREW_") and key not in ("HOME", "PORT", "XDG_DATA_HOME", "XDG_CACHE_HOME")}
    # Test-child environment only; the user's HOME is never reassigned.
    env.update(HOME=str(home), PATH="/usr/bin:/bin",
               XDG_DATA_HOME=str(home / "data"), XDG_CACHE_HOME=str(home / "cache"))
    primary = package / "onecrew"
    library = package / "library" / primary.name

    def run(binary, *arguments):
        return subprocess.run([str(binary), *arguments], cwd=home, env=env,
                              capture_output=True, text=True, check=True, timeout=70)

    try:
        first = json.loads(run(primary, "workspace", "--no-browser").stdout)
        assert set(first) == {"workbench_url", "library_url"}, first
        assert first["library_url"].endswith("/library"), first
        for address in first.values():
            assert address.startswith("http://127.0.0.1:"), address
            with urllib.request.urlopen(address, timeout=5) as response:
                assert response.status == 200
                assert b"<html" in response.read().lower()
        # No account/table is manufactured to make the landing page appear.
        status = json.loads(run(primary, "status").stdout)
        assert status["login"]["status"]["state"] != "active", "fresh profile must remain unsigned"
        try:
            urllib.request.urlopen(first["library_url"].removesuffix("/library") + "/api/v1/dynamic-tables", timeout=5)
            raise AssertionError("fresh Library must require its existing sign-in")
        except urllib.error.HTTPError as error:
            assert error.code == 401
        pid = status["pid"]
        second = json.loads(run(primary, "workspace", "--no-browser").stdout)
        assert first == second, "repeat launch must preserve actual ports"
        assert json.loads(run(primary, "status").stdout)["pid"] == pid
        print("PASS: two real native services, fresh unsigned profiles, no Node in PATH, HTML pages, idempotent relaunch")
    finally:
        # Each service's own graceful stop is the only shutdown mechanism.
        failures = []
        for binary in (primary, library):
            result = subprocess.run([str(binary), "stop"], cwd=home, env=env,
                                    capture_output=True, text=True, timeout=35)
            if result.returncode:
                failures.append(binary.name)
        if failures:
            raise RuntimeError(f"Native test services did not stop: {failures}")
