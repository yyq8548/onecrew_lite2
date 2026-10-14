#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Write local raw-binary identity evidence without publishing an artifact."""
import argparse
import hashlib
import json
import os
import tempfile
from pathlib import Path

TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc", "x86_64-pc-windows-gnu")


def validate_architecture(data, target):
    if target.endswith("apple-darwin"):
        cpu = 0x0100000C if target.startswith("aarch64") else 0x01000007
        valid = len(data) >= 32 and data[:4] == b"\xcf\xfa\xed\xfe" and int.from_bytes(data[4:8], "little") == cpu and int.from_bytes(data[12:16], "little") == 2
    else:
        offset = int.from_bytes(data[60:64], "little") if len(data) >= 64 else len(data)
        valid = data[:2] == b"MZ" and offset + 24 <= len(data) and data[offset:offset + 4] == b"PE\x00\x00" and data[offset + 4:offset + 6] == b"\x64\x86"
    if not valid:
        raise ValueError(f"binary does not match {target}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    version = json.loads((root / "development/release/version.json").read_text())["cliPluginBase"]
    if not args.binary.is_file() or args.binary.is_symlink():
        parser.error("binary must be a regular non-symlink file")
    data = args.binary.read_bytes()
    if not data:
        parser.error("binary must not be empty")
    try:
        validate_architecture(data, args.target)
    except ValueError as error:
        parser.error(str(error))
    manifest = {"schema_version": 1, "product": "onecrew_lite-cli", "version": version,
                "target": args.target, "sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
    # Local build outputs may be rebuilt. R2 publication has its own immutable
    # conditional-write boundary; publish the local manifest atomically.
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=args.output.parent, delete=False) as output:
        json.dump(manifest, output, indent=2)
        output.write("\n")
        pending = output.name
    try:
        os.replace(pending, args.output)
    finally:
        if os.path.exists(pending):
            os.unlink(pending)


if __name__ == "__main__":
    main()
