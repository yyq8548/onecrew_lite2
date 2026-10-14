"""Offline release publication tests; no credentials or network are used."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("upload_r2", Path(__file__).with_name("upload-r2.py"))
upload = importlib.util.module_from_spec(spec)
spec.loader.exec_module(upload)


class FakeR2:
    def __init__(self, corrupt_at=None):
        self.objects = {}
        self.puts = []
        self.reads = []
        self.corrupt_at = corrupt_at

    def put_object(self, **request):
        assert request["IfNoneMatch"] == "*"
        self.puts.append(request["Key"])
        self.objects[request["Key"]] = request

    def get_object(self, **request):
        self.reads.append(request["Key"])
        obj = self.objects[request["Key"]]
        body = obj["Body"]
        if len(self.reads) == self.corrupt_at:
            body = bytes([body[0] ^ 1]) + body[1:]
        return {"ContentLength": len(body), "Metadata": obj["Metadata"], "Body": io.BytesIO(body)}


class ReleasePublicationTests(unittest.TestCase):
    def make_manifests(self, root):
        paths = []
        for target in upload.CLI_TARGETS:
            data = bytearray(96)
            windows = target.endswith("windows-msvc")
            if windows:
                data[:2] = b"MZ"
                data[60:64] = (64).to_bytes(4, "little")
                data[64:68] = b"PE\x00\x00"
                data[68:70] = b"\x64\x86"
            else:
                data[:4] = b"\xcf\xfa\xed\xfe"
                data[4:8] = (0x0100000C if target.startswith("aarch64") else 0x01000007).to_bytes(4, "little")
                data[12:16] = (2).to_bytes(4, "little")
            directory = root / target
            directory.mkdir()
            (directory / ("onecrew_lite.exe" if windows else "onecrew_lite")).write_bytes(data)
            path = directory / "manifest.json"
            path.write_text(json.dumps({"schema_version": 1, "product": "onecrew_lite-cli", "version": "0.5.21",
                                        "target": target, "size": len(data), "sha256": hashlib.sha256(data).hexdigest()}))
            paths.append(path)
        return paths

    def run_main(self, paths, output, fake, live=True):
        args = ["upload-r2.py", "--release-manifest", str(output)]
        for path in paths:
            args += ["--manifest", str(path)]
        if live:
            args += ["--upload"]
        modules = {"boto3": types.SimpleNamespace(client=lambda *a, **k: fake),
                   "botocore.config": types.SimpleNamespace(Config=lambda **k: None)}
        env = {"R2_ENDPOINT_URL": "https://example.r2.cloudflarestorage.com",
               "R2_ACCESS_KEY_ID": "offline-test", "R2_SECRET_ACCESS_KEY": "offline-test"}
        with patch.object(sys, "argv", args), patch.dict(sys.modules, modules), patch.dict(os.environ, env), contextlib.redirect_stdout(io.StringIO()):
            upload.main()

    def test_complete_catalog_is_published_after_all_three_readbacks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = FakeR2()
            self.run_main(self.make_manifests(root), root / "cli-manifest.json", fake)
            self.assertEqual(len(fake.puts), 4)
            self.assertEqual(fake.puts, fake.reads)
            self.assertEqual(fake.puts[-1], "onecrew_lite/releases/0.5.21/cli-manifest.json")
            catalog = json.loads(fake.objects[fake.puts[-1]]["Body"])
            self.assertEqual({a["target"] for a in catalog["assets"]}, set(upload.CLI_TARGETS))

    def test_corrupt_binary_readback_prevents_catalog_publication(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = FakeR2(corrupt_at=2)
            with self.assertRaisesRegex(ValueError, "content mismatch"):
                self.run_main(self.make_manifests(root), root / "cli-manifest.json", fake)
            self.assertEqual(len(fake.puts), 2)
            self.assertFalse(any(key.endswith("cli-manifest.json") for key in fake.puts))

    def test_missing_platform_fails_before_any_upload(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = FakeR2()
            with self.assertRaisesRegex(ValueError, "all three"):
                self.run_main(self.make_manifests(root)[:2], root / "cli-manifest.json", fake)
            self.assertEqual(fake.puts, [])

    def test_dry_run_writes_complete_local_catalog_without_network(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = FakeR2()
            output = root / "cli-manifest.json"
            self.run_main(self.make_manifests(root), output, fake, live=False)
            self.assertEqual(len(json.loads(output.read_text())["assets"]), 3)
            self.assertEqual(fake.puts, [])


class EnvironmentFileTests(unittest.TestCase):
    def test_environment_file_is_parsed_as_data_not_executed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root / ".env"
            marker = root / "must-not-exist"
            literal = f"$(touch {marker})"
            path.write_text(f"R2_SECRET_ACCESS_KEY='{literal}'\nR2_ACCESS_KEY_ID=test-key # comment\n")
            path.chmod(0o600)
            values = upload.r2_environment(path)
            self.assertEqual(values["R2_SECRET_ACCESS_KEY"], literal)
            self.assertEqual(values["R2_ACCESS_KEY_ID"], "test-key")
            self.assertFalse(marker.exists())

    def test_parse_error_never_contains_secret_value(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / ".env"
            path.write_text("R2_SECRET_ACCESS_KEY='private-test-value\n")
            path.chmod(0o600)
            with self.assertRaises(ValueError) as caught:
                upload.r2_environment(path)
            self.assertNotIn("private-test-value", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
