#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["boto3==1.40.76"]
# ///
"""Upload verified raw CLI artifacts to R2; default is an offline dry-run."""
import argparse
import hashlib
import importlib.util
import json
import os
import re
import shlex
import sys
from pathlib import Path
from urllib.parse import urlparse

CLI_TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc")


def r2_environment(path=None):
    names = ("R2_ENDPOINT_URL", "R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY")
    values = {name: os.environ.get(name, "") for name in names}
    if path is not None:
        if os.name != "nt" and path.stat().st_mode & 0o077:
            raise ValueError("R2 environment file must have private permissions (600)")
        for line in path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if line.startswith("export "):
                line = line[7:].lstrip()
            name, separator, raw = line.partition("=")
            name = name.strip()
            if name not in names:
                continue
            try:
                parts = shlex.split(raw, comments=True)
            except ValueError:
                raise ValueError("invalid R2 environment file encoding") from None
            if not separator or len(parts) != 1:
                raise ValueError("invalid R2 environment file assignment")
            values[name] = parts[0]
    return values


def release_manifest_entry(entries):
    """Publish the catalog last, only after every platform artifact is verified."""
    if {entry["target"] for entry in entries} != set(CLI_TARGETS) or len(entries) != 3:
        raise ValueError("a CLI release requires all three supported targets")
    versions = {entry["version"] for entry in entries}
    if len(versions) != 1:
        raise ValueError("all artifacts must use the same version")
    version = next(iter(versions))
    manifest = {"schema_version": 1, "product": "onecrew_lite-cli", "version": version,
                "assets": [{key: entry[key] for key in ("target", "key", "size", "sha256")}
                           for entry in sorted(entries, key=lambda item: item["target"])]}
    data = (json.dumps(manifest, indent=2) + "\n").encode("utf-8")
    return {"version": version, "key": f"onecrew_lite/releases/{version}/cli-manifest.json",
            "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
            "data": data, "content_type": "application/json"}


def load_artifacts(manifests):
    module_path = Path(__file__).with_name("binary-manifest.py")
    spec = importlib.util.spec_from_file_location("binary_manifest", module_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    entries, targets, versions = [], set(), set()
    for path in manifests:
        value = json.loads(path.read_text(encoding="utf-8"))
        target, version = value["target"], value["version"]
        if value.get("schema_version") != 1 or value.get("product") != "onecrew_lite-cli":
            raise ValueError("unsupported artifact manifest")
        if target not in module.TARGETS or not re.fullmatch(r"\d+\.\d+\.\d+", version):
            raise ValueError("invalid artifact target/version")
        if target in targets:
            raise ValueError("duplicate target manifest")
        targets.add(target)
        versions.add(version)
        suffix = ".exe" if target.endswith("windows-msvc") else ""
        binary = path.parent / ("onecrew_lite" + suffix)
        if binary.is_symlink() or not binary.is_file():
            raise ValueError("artifact must be a regular non-symlink file")
        data = binary.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if len(data) != value["size"] or digest != value["sha256"]:
            raise ValueError(f"artifact hash/size mismatch: {target}")
        module.validate_architecture(data, target)
        key = f"onecrew_lite/releases/{version}/onecrew_lite-{version}-{target}{suffix}"
        entries.append({"target": target, "version": version, "key": key,
                        "size": len(data), "sha256": digest, "binary": binary})
    if len(versions) != 1:
        raise ValueError("all artifacts must use the same version")
    return entries


def verify_remote(client, bucket, entry):
    response = client.get_object(Bucket=bucket, Key=entry["key"])
    body = response["Body"]
    try:
        if response["ContentLength"] != entry["size"] or response.get("Metadata", {}).get("sha256") != entry["sha256"]:
            raise ValueError("R2 object metadata/length mismatch")
        digest, size = hashlib.sha256(), 0
        for chunk in iter(lambda: body.read(1024 * 1024), b""):
            size += len(chunk)
            if size > entry["size"]:
                raise ValueError("R2 object exceeds expected size")
            digest.update(chunk)
        if size != entry["size"] or digest.hexdigest() != entry["sha256"]:
            raise ValueError("R2 object content mismatch")
    finally:
        body.close()


def publish(client, bucket, entry):
    # Read each bounded CLI artifact into the request body and revalidate it at
    # the upload boundary, so concurrent local rebuilds cannot change the PUT.
    data = entry["data"] if "data" in entry else entry["binary"].read_bytes()
    if len(data) != entry["size"] or hashlib.sha256(data).hexdigest() != entry["sha256"]:
        raise ValueError("artifact changed after preflight")
    outcome = "uploaded"
    try:
        client.put_object(Bucket=bucket, Key=entry["key"], Body=data,
                          ContentLength=len(data), ContentType=entry.get("content_type", "application/octet-stream"),
                          Metadata={"sha256": entry["sha256"]}, IfNoneMatch="*")
    except Exception as error:
        code = getattr(error, "response", {}).get("Error", {}).get("Code")
        if code not in ("PreconditionFailed", "ConditionalRequestConflict", "412", "409"):
            raise
        outcome = "already_present"
    verify_remote(client, bucket, entry)
    return outcome


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, action="append",
                        help="build manifest.json; repeat for the other targets")
    parser.add_argument("--release", action="store_true", help="use all three targets from the repository's current release version")
    parser.add_argument("--env-file", type=Path, help="private R2 environment file; values are never printed")
    parser.add_argument("--bucket", default="onecrew-releases")
    parser.add_argument("--upload", action="store_true", help="perform conditional PUT and verified GET")
    parser.add_argument("--release-manifest", type=Path,
                        help="require all three targets and write/publish their CLI manifest after verified binaries")
    args = parser.parse_args()
    if args.release:
        if args.manifest or args.release_manifest:
            parser.error("--release cannot be combined with explicit manifest paths")
        root = Path(__file__).resolve().parents[2]
        version = json.loads((root / "development/release/version.json").read_text())["cliPluginBase"]
        if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d+", version):
            raise ValueError("invalid repository release version")
        directory = root / "dist/onecrew_lite" / version
        args.manifest = [directory / target / "manifest.json" for target in CLI_TARGETS]
        args.release_manifest = directory / "cli-manifest.json"
    if not args.manifest:
        parser.error("use --release or --manifest")
    if not re.fullmatch(r"[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]", args.bucket):
        parser.error("invalid R2 bucket")
    entries = load_artifacts(args.manifest)
    release = release_manifest_entry(entries) if args.release_manifest else None
    if release:
        args.release_manifest.parent.mkdir(parents=True, exist_ok=True)
        args.release_manifest.write_bytes(release["data"])
    client = None
    if args.upload:
        environment = r2_environment(args.env_file)
        endpoint = environment["R2_ENDPOINT_URL"]
        url = urlparse(endpoint)
        if url.scheme != "https" or not (url.hostname or "").endswith(".r2.cloudflarestorage.com") or url.username or url.password or url.port or url.path not in ("", "/") or url.query or url.fragment:
            raise ValueError("R2_ENDPOINT_URL must be an official HTTPS R2 S3 endpoint")
        key, secret = environment["R2_ACCESS_KEY_ID"], environment["R2_SECRET_ACCESS_KEY"]
        if not key or not secret:
            raise ValueError("R2_ACCESS_KEY_ID and R2_SECRET_ACCESS_KEY are required")
        import boto3
        from botocore.config import Config
        client = boto3.client("s3", endpoint_url=endpoint, region_name="auto",
                              aws_access_key_id=key, aws_secret_access_key=secret,
                              config=Config(signature_version="s3v4", connect_timeout=15, read_timeout=120,
                                            retries={"max_attempts": 2, "mode": "standard"}))
    for entry in entries:
        result = {k: v for k, v in entry.items() if k != "binary"}
        result.update(bucket=args.bucket, outcome=publish(client, args.bucket, entry) if client else "dry_run")
        print(json.dumps(result, ensure_ascii=False), flush=True)
    if release:
        result = {k: v for k, v in release.items() if k not in ("data", "content_type")}
        result.update(bucket=args.bucket, outcome=publish(client, args.bucket, release) if client else "dry_run")
        print(json.dumps(result, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # SDK exception text can contain request details; expose only its code.
        code = getattr(error, "response", {}).get("Error", {}).get("Code")
        message = str(error) if isinstance(error, (ValueError, FileNotFoundError, ModuleNotFoundError)) else code or type(error).__name__
        print(f"upload-r2: {message}", file=sys.stderr)
        raise SystemExit(1)
