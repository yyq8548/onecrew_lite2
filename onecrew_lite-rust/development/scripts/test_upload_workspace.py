"""Offline whole-package publication boundaries, without credentials or execution."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile
from test_upload_r2 import FakeR2
import importlib.util

spec = importlib.util.spec_from_file_location("workspace_upload", Path(__file__).with_name("upload-workspace.py"))
upload = importlib.util.module_from_spec(spec)
spec.loader.exec_module(upload)


def fixture(root):
    root.mkdir()
    target = "aarch64-apple-darwin"
    binary = bytearray(96)
    binary[:4] = b"\xcf\xfa\xed\xfe"
    binary[4:8] = (0x0100000C).to_bytes(4, "little")
    binary[12:16] = (2).to_bytes(4, "little")
    digest = {"size": len(binary), "sha256": upload.sha(binary)}
    cli = {"schema_version": 1, "product": "onecrew_lite-cli", "version": "0.7.1", "target": target, **digest}
    library = {"version": "0.1.4", "target": "darwin-arm64", "capabilities": ["discovery-contacts-v2", "recommendation-capture-v1"], "files": [{"path": "onecrew_lite", **digest}]}
    contents = {"onecrew_lite": binary, "onecrew_lite_library/onecrew_lite": binary,
                "workbench-manifest.json": json.dumps(cli).encode(),
                "onecrew_lite_library/manifest.json": json.dumps(library).encode(),
                "install-core.sh": b"fixture", "connect-codex.sh": b"fixture"}
    files = []
    for name, data in contents.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        files.append({"path": name, "size": len(data), "sha256": upload.sha(data)})
    manifest = {"schema_version": 1, "product": "onecrew_lite-workspace", "version": "0.7.1", "target": target, "library_version": "0.1.4", "files": files}
    data = json.dumps(manifest).encode()
    (root / "manifest.json").write_bytes(data)
    provenance = {"schema_version": 1, "target": target, "host": "darwin-arm64", "version": "0.7.1", "plugin": "embedded-in-cli", "commit": "a" * 40, "manifest_sha256": upload.sha(data)}
    (root / "release-build.json").write_text(json.dumps(provenance))
    acceptance = {target: {"commit": provenance["commit"], "manifest_sha256": provenance["manifest_sha256"], "native_host": "darwin-arm64", "checks": {k: "passed" for k in upload.CHECKS}, "os_protection": "unchanged", "signing_evidence": "fixture-only", "minimum_os": "13.5"}}
    return acceptance


class WorkspaceTests(unittest.TestCase):
    def test_whole_zip_and_catalog_readbacks_precede_activation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            acceptance = fixture(root / "bundle")
            entries, release = upload.prepare([root / "bundle"], root / "output", acceptance)
            with zipfile.ZipFile(entries[0]["binary"]) as archive:
                self.assertIn("onecrew_lite_library/onecrew_lite", archive.namelist())
                self.assertEqual(upload.sha(archive.read("manifest.json")), acceptance["aarch64-apple-darwin"]["manifest_sha256"])
            fake = FakeR2()
            for entry in [*entries, release]:
                upload.transport.publish(fake, "onecrew-releases", entry)
            self.assertEqual(fake.puts, fake.reads)
            self.assertTrue(fake.puts[-1].endswith("/workspace-manifest.json"))
            corrupt = FakeR2(corrupt_at=1)
            with self.assertRaises(ValueError):
                for entry in [*entries, release]:
                    upload.transport.publish(corrupt, "onecrew-releases", entry)
            self.assertEqual(len(corrupt.puts), 1)

    def test_missing_or_wrong_build_acceptance_blocks_release(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            acceptance = fixture(root / "bundle")
            acceptance["aarch64-apple-darwin"]["commit"] = "b" * 40
            with self.assertRaisesRegex(ValueError, "native acceptance"):
                upload.prepare([root / "bundle"], root / "output", acceptance)

    def test_extra_file_and_corruption_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "bundle"
            fixture(root)
            (root / "secret.env").write_text("fixture")
            with self.assertRaisesRegex(ValueError, "unexpected"):
                upload.verify_bundle(root)
            (root / "secret.env").unlink()
            (root / "onecrew_lite").write_text("corrupt")
            with self.assertRaisesRegex(ValueError, "integrity"):
                upload.verify_bundle(root)

    def test_metadata_changes_between_validation_and_archiving_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fixture(root / "bundle")
            original = upload.verify_bundle
            def changed(directory):
                result = original(directory)
                path = directory / "release-build.json"
                value = json.loads(path.read_text())
                value["commit"] = "b" * 40
                path.write_text(json.dumps(value))
                return result
            with patch.object(upload, "verify_bundle", changed), self.assertRaisesRegex(ValueError, "changed while archiving"):
                upload.prepare([root / "bundle"], root / "output")


if __name__ == "__main__":
    unittest.main()
