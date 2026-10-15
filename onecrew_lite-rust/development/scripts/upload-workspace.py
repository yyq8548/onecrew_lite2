#!/usr/bin/env python3
"""Build immutable onecrew_lite workspace ZIPs; offline by default. Never selects a live release."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import sys
from urllib.parse import urlparse
import zipfile

TARGETS = {"aarch64-apple-darwin": "darwin-arm64", "x86_64-apple-darwin": "darwin-x64", "x86_64-pc-windows-msvc": "win32-x64"}
CHECKS = {"clean_install", "reinstall", "upgrade", "rollback", "codex_live", "library_live", "coexistence"}
def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value
transport = module("r2_transport", "upload-r2.py")
binary_manifest = module("binary_manifest", "binary-manifest.py")
def sha(data):
    return hashlib.sha256(data).hexdigest()
def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))
def regular(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError("regular package file required")
def verify_bundle(directory):
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("real bundle directory required")
    for file in ("manifest.json", "release-build.json"):
        regular(directory / file)
    manifest, provenance = read_json(directory / "manifest.json"), read_json(directory / "release-build.json")
    version, target = manifest.get("version", ""), manifest.get("target", "")
    if (manifest.get("product") != "onecrew_lite-workspace" or manifest.get("schema_version") != 1 or
        not re.fullmatch(r"\d+\.\d+\.\d+", version) or target not in TARGETS or
        provenance.get("schema_version") != 1 or provenance.get("target") != target or
        provenance.get("host") != TARGETS[target] or provenance.get("version") != version or
        provenance.get("plugin") != "embedded-in-cli" or not re.fullmatch(r"[a-f0-9]{40}", provenance.get("commit", "")) or
        provenance.get("manifest_sha256") != sha((directory / "manifest.json").read_bytes())):
        raise ValueError("invalid bundle provenance")
    files = {}
    for entry in manifest.get("files", []):
        name = entry.get("path", "")
        if (not name or "\\" in name or ":" in name or "\0" in name or name.startswith("/") or
            any(part in ("", ".", "..") for part in name.split("/")) or name in files or name in ("manifest.json", "release-build.json")):
            raise ValueError("unsafe or duplicate package path")
        file = directory / name
        for parent in file.parents:
            if parent == directory:
                break
            if parent.is_symlink():
                raise ValueError("symlink package parent")
        regular(file)
        data = file.read_bytes()
        if len(data) != entry.get("size") or sha(data) != entry.get("sha256"):
            raise ValueError("bundle integrity mismatch")
        files[name] = entry
    actual = set()
    for file in directory.rglob("*"):
        if file.is_symlink():
            raise ValueError("symlink package entry")
        if file.is_file():
            actual.add(file.relative_to(directory).as_posix())
    if actual != set(files) | {"manifest.json", "release-build.json"}:
        raise ValueError("unexpected or missing package file")
    binary = "onecrew_lite.exe" if target.endswith("msvc") else "onecrew_lite"
    cli, library = read_json(directory / "workbench-manifest.json"), read_json(directory / "onecrew_lite_library/manifest.json")
    lib_entry = next((f for f in library.get("files", []) if f["path"] == binary), None)
    if (cli.get("product") != "onecrew_lite-cli" or cli.get("target") != target or cli.get("version") != version or
        library.get("target") != TARGETS[target] or library.get("version") != manifest.get("library_version") or
        not {"discovery-contacts-v2", "recommendation-capture-v1"}.issubset(set(library.get("capabilities", []))) or not lib_entry):
        raise ValueError("incompatible component pair")
    for name, expected in ((binary, cli), ("onecrew_lite_library/" + binary, lib_entry)):
        if name not in files or any(files[name][key] != expected[key] for key in ("size", "sha256")):
            raise ValueError("component digest mismatch")
        binary_manifest.validate_architecture((directory / name).read_bytes(), target)
    required = {"install-core.ps1", "connect-codex.ps1", "windows-setup-step.ps1"} if target.endswith("msvc") else {"install-core.sh", "connect-codex.sh"}
    if not required.issubset(files):
        raise ValueError("missing core or client adapter")
    # Freeze non-self-listed metadata along with file fingerprints. Archive
    # creation must not silently accept another internally consistent build.
    for name in ("manifest.json", "release-build.json"):
        data = (directory / name).read_bytes()
        parsed = json.loads(data)
        if parsed != (manifest if name == "manifest.json" else provenance):
            raise ValueError("metadata changed during verification")
        if name == "manifest.json" and sha(data) != provenance["manifest_sha256"]:
            raise ValueError("manifest changed during verification")
        files[name] = {"size": len(data), "sha256": sha(data)}
    return manifest, provenance, files

def prepare(bundles, output, acceptance=None):
    validated = [(directory, *verify_bundle(directory)) for directory in bundles]
    if not validated or len({m["target"] for _, m, _, _ in validated}) != len(validated):
        raise ValueError("missing or duplicate platform")
    if len({(m["version"], m["library_version"], p["commit"]) for _, m, p, _ in validated}) != 1:
        raise ValueError("mixed release versions or source commits")
    output.mkdir()  # Never overwrite previously prepared artifacts.
    entries = []
    for directory, manifest, provenance, files in validated:
        target, version = manifest["target"], manifest["version"]
        accepted = (acceptance or {}).get(target)
        if acceptance is not None and (not accepted or accepted.get("commit") != provenance["commit"] or
            accepted.get("manifest_sha256") != provenance["manifest_sha256"] or accepted.get("native_host") != TARGETS[target] or
            not CHECKS.issubset({k for k, v in accepted.get("checks", {}).items() if v == "passed"}) or
            accepted.get("os_protection") != "unchanged" or not accepted.get("signing_evidence")):
            raise ValueError("native acceptance missing or belongs to another build")
        minimum = accepted.get("minimum_os") if accepted else ("10.0" if target.endswith("msvc") else "13.5")
        if not isinstance(minimum, str) or not re.fullmatch(r"\d+\.\d+(?:\.\d+)?", minimum):
            raise ValueError("minimum OS required")
        name = f"onecrew_lite-{version}-{target}.zip"
        destination = output / name
        with zipfile.ZipFile(destination, "x", compression=zipfile.ZIP_DEFLATED) as archive:
            for relative in sorted(set(files) | {"manifest.json", "release-build.json"}):
                file = directory / relative
                regular(file)
                data = file.read_bytes()
                if len(data) != files[relative]["size"] or sha(data) != files[relative]["sha256"]:
                    raise ValueError("bundle changed while archiving")
                info = zipfile.ZipInfo(relative, (2026, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.external_attr = (stat.S_IFREG | (0o755 if relative.endswith((".sh", ".command")) or relative.split('/')[-1] == "onecrew_lite" else 0o644)) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, data)
        # Provenance and the manifest are not self-listed; recheck after archive creation.
        verify_bundle(directory)
        data = destination.read_bytes()
        entries.append({"target": target, "version": version, "key": f"onecrew_lite/releases/{version}/{name}",
            "size": len(data), "sha256": sha(data), "minimum_os": minimum, "binary": destination, "content_type": "application/zip"})
    manifest = validated[0][1]
    catalog = {"schema_version": 1, "product": "onecrew_lite", "version": manifest["version"],
        "commit": validated[0][2]["commit"], "library_version": manifest["library_version"], "client_adapters": ["codex"],
        "packages": [{k: e[k] for k in ("target", "key", "size", "sha256", "minimum_os")} for e in sorted(entries, key=lambda e:e["target"])]}
    data = (json.dumps(catalog, indent=2) + "\n").encode()
    (output / "workspace-manifest.json").write_bytes(data)
    release = {"key": f"onecrew_lite/releases/{manifest['version']}/workspace-manifest.json", "size": len(data), "sha256": sha(data), "data": data, "content_type": "application/json"}
    return entries, release

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--acceptance", type=Path)
    parser.add_argument("--upload", action="store_true")
    parser.add_argument("--env-file", type=Path)
    parser.add_argument("--bucket", default="onecrew-releases")
    args = parser.parse_args()
    if args.upload and not args.acceptance:
        parser.error("--upload requires native acceptance for every selected platform")
    if not re.fullmatch(r"[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]", args.bucket):
        parser.error("invalid bucket")
    entries, release = prepare(args.bundle, args.output, read_json(args.acceptance) if args.acceptance else None)
    if args.upload:
        values = transport.r2_environment(args.env_file)
        endpoint = urlparse(values["R2_ENDPOINT_URL"])
        if (endpoint.scheme != "https" or not (endpoint.hostname or "").endswith(".r2.cloudflarestorage.com") or
            endpoint.username or endpoint.password or endpoint.port or endpoint.path not in ("", "/") or endpoint.query or endpoint.fragment or
            not values["R2_ACCESS_KEY_ID"] or not values["R2_SECRET_ACCESS_KEY"]):
            raise ValueError("official R2 endpoint and credentials required")
        import boto3
        from botocore.config import Config
        client = boto3.client("s3", endpoint_url=values["R2_ENDPOINT_URL"], aws_access_key_id=values["R2_ACCESS_KEY_ID"], aws_secret_access_key=values["R2_SECRET_ACCESS_KEY"], config=Config(connect_timeout=10, read_timeout=60, retries={"max_attempts": 2}))
        for entry in [*entries, release]:
            transport.publish(client, args.bucket, entry)
    print(json.dumps({"product":"onecrew_lite", "outcome":"uploaded_not_activated" if args.upload else "offline_candidate", "platforms":[e["target"] for e in entries]}))
if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("onecrew_lite workspace preparation failed: " + (str(error) if isinstance(error, ValueError) else type(error).__name__), file=sys.stderr)
        raise SystemExit(1)
