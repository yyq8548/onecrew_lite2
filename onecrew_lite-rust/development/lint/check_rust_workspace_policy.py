#!/usr/bin/env -S uv run --script

from __future__ import annotations

import copy
import os
import re
import tempfile
import tomllib
import unittest
from collections.abc import Iterator
from pathlib import Path


REPO_ROOT = Path(__file__).parents[2]
ROOT_MANIFEST = REPO_ROOT / "Cargo.toml"
TOOLCHAIN_MANIFEST = REPO_ROOT / "rust-toolchain.toml"
MCP_ADAPTER_MANIFEST = REPO_ROOT / "crates/mcp-adapter/Cargo.toml"
# ADR 0017 / upper-layer realignment D7: the adapter is a thin inbound
# projection over the application owners, so any other workspace crate
# (platform, host composition or contract crate) is a boundary violation.
MCP_ADAPTER_INTERNAL_ALLOWLIST = frozenset({
    "onecrew-creator-runtime",
    "onecrew-local-runtime",
    "onecrew-schema-manifest",
})
DEPENDENCY_SECTIONS = ("dependencies", "dev-dependencies", "build-dependencies")
EXPECTED_WORKSPACE_LINTS = {
    "rust": {
        "unsafe_op_in_unsafe_fn": "deny",
        "unreachable_pub": "deny",
        "unused_must_use": "deny",
    },
    "rustdoc": {
        "broken_intra_doc_links": "deny",
        "invalid_html_tags": "deny",
        "missing_crate_level_docs": "deny",
        "private_intra_doc_links": "deny",
    },
    "clippy": {
        "allow_attributes_without_reason": "deny",
        "dbg_macro": "deny",
        "float_cmp": "deny",
        "let_underscore_future": "deny",
        "let_underscore_must_use": "deny",
        "too_many_arguments": "deny",
        "undocumented_unsafe_blocks": "deny",
    },
}


def read_toml(path: Path) -> dict[str, object]:
    with path.open("rb") as source:
        return tomllib.load(source)


def workspace_member_manifests() -> list[Path]:
    root = read_toml(ROOT_MANIFEST)
    members = root["workspace"]["members"]
    manifests: list[Path] = []
    for member in members:
        matches = sorted(REPO_ROOT.glob(member))
        if not matches:
            raise AssertionError(f"workspace member pattern matched nothing: {member}")
        manifests.extend(path / "Cargo.toml" for path in matches)
    return manifests


def dependency_declarations(manifest: dict, workspace: dict) -> Iterator[tuple[str, object, bool]]:
    """Yield (key, declaration, inherited) for renamed, target-specific and development
    dependencies; a `workspace = true` declaration resolves to its workspace entry."""
    scopes = [manifest, *manifest.get("target", {}).values()]
    for scope in scopes:
        for section in DEPENDENCY_SECTIONS:
            for key, value in scope.get(section, {}).items():
                inherited = isinstance(value, dict) and bool(value.get("workspace"))
                if inherited:
                    value = workspace.get(key, value)
                yield key, value, inherited


def declared_package(key: str, value: object) -> str:
    return value.get("package", key) if isinstance(value, dict) else key


def dependency_names(manifest: dict, workspace: dict) -> set[str]:
    """Include renamed, target-specific and development dependency declarations."""
    return {declared_package(key, value) for key, value, _ in dependency_declarations(manifest, workspace)}


def path_dependency_packages(manifest: dict, manifest_dir: Path, workspace: dict) -> set[str]:
    """Real package names of every path dependency, member or not.

    A direct path resolves against the declaring manifest's directory and a
    workspace entry's path against the workspace root. The target manifest's
    `package.name` wins; an unreadable target falls back to the declared
    `package` or the dependency key.
    """
    names: set[str] = set()
    for key, value, inherited in dependency_declarations(manifest, workspace):
        if not (isinstance(value, dict) and "path" in value):
            continue
        base = ROOT_MANIFEST.parent if inherited else manifest_dir
        try:
            name = read_toml(base / value["path"] / "Cargo.toml")["package"]["name"]
        except (OSError, tomllib.TOMLDecodeError, KeyError, TypeError):
            name = None
        names.add(name if isinstance(name, str) else declared_package(key, value))
    return names


def workspace_package_names() -> set[str]:
    return {read_toml(path)["package"]["name"] for path in workspace_member_manifests()}


def disallowed_internal_dependencies(
    manifest: dict, manifest_dir: Path, workspace: dict, internal_names: set[str], allowlist: frozenset[str],
) -> set[str]:
    """Internal means a workspace member package or any path dependency, matched by real package name."""
    internal = dependency_names(manifest, workspace) & internal_names
    internal |= path_dependency_packages(manifest, manifest_dir, workspace)
    return internal - allowlist


def with_dependency(manifest: dict, section: tuple[str, ...], key: str, declaration: object) -> dict:
    fixture = copy.deepcopy(manifest)
    scope = fixture
    for part in section:
        scope = scope.setdefault(part, {})
    scope[key] = declaration
    return fixture


class RustWorkspacePolicyTests(unittest.TestCase):
    def test_provider_and_creator_dependencies_point_to_their_owners(self) -> None:
        workspace = read_toml(ROOT_MANIFEST)["workspace"].get("dependencies", {})
        manifests = [(path, read_toml(path)) for path in workspace_member_manifests()]
        provider_names = {
            manifest["package"]["name"]
            for path, manifest in manifests
            if path.parent.name.endswith(("-transport", "-client", "-platform"))
        }
        application_names = {
            "onecrew", "onecrew-local-service", "onecrew-mcp-adapter",
            "onecrew-local-runtime", "onecrew-creator-runtime", "onecrew-sessions",
        }
        for path, manifest in manifests:
            name = manifest["package"]["name"]
            names = dependency_names(manifest, workspace)
            with self.subTest(manifest=str(path.relative_to(REPO_ROOT))):
                self.assertNotIn("onecrew-creator-platform-adapter", names)
                if name in provider_names:
                    self.assertFalse(names & application_names)
                if name == "onecrew-creator-runtime":
                    self.assertFalse(names & (application_names | provider_names))
                if name == "onecrew-sessions":
                    self.assertFalse(names & (application_names - {"onecrew-sessions"}))

    def test_runtime_cannot_depend_on_mcp_execution_or_search_compilers(self) -> None:
        root = read_toml(ROOT_MANIFEST)
        runtime = read_toml(REPO_ROOT / "crates/platform-runtime/Cargo.toml")
        names = dependency_names(runtime, root["workspace"].get("dependencies", {}))
        self.assertFalse(names & {"onecrew-mcp-adapter", "rmcp", "tantivy", "jsonschema"})

    def test_dependency_boundary_includes_aliases_and_target_development_edges(self) -> None:
        fixture = {
            "dependencies": {"protocol": {"workspace": True}},
            "target": {"cfg(test)": {"dev-dependencies": {"search": {"package": "tantivy"}}}},
        }
        self.assertEqual(dependency_names(fixture, {"protocol": {"package": "rmcp"}}), {"rmcp", "tantivy"})

    def test_mcp_adapter_internal_dependencies_stay_on_allowlist(self) -> None:
        workspace = read_toml(ROOT_MANIFEST)["workspace"].get("dependencies", {})
        internal_names = workspace_package_names()
        self.assertLessEqual(MCP_ADAPTER_INTERNAL_ALLOWLIST, internal_names)
        self.assertEqual(
            disallowed_internal_dependencies(
                read_toml(MCP_ADAPTER_MANIFEST), MCP_ADAPTER_MANIFEST.parent, workspace, internal_names,
                MCP_ADAPTER_INTERNAL_ALLOWLIST,
            ),
            set(),
            f"mcp-adapter internal dependencies must stay within {sorted(MCP_ADAPTER_INTERNAL_ALLOWLIST)}",
        )

    def test_mcp_adapter_allowlist_rejects_aliases_targets_and_platform_crates(self) -> None:
        workspace = read_toml(ROOT_MANIFEST)["workspace"].get("dependencies", {})
        internal_names = workspace_package_names()
        adapter = read_toml(MCP_ADAPTER_MANIFEST)
        platform_crates = sorted(
            (path.parent.name, read_toml(path)["package"]["name"])
            for path in workspace_member_manifests()
            if path.parent.name.endswith("-platform")
        )
        self.assertTrue(platform_crates)
        # (label, section path, dependency key, declaration, workspace overlay, rejected package)
        cases = [
            ("package alias", ("dependencies",), "host",
             {"package": "onecrew-local-service", "path": "../local-service"}, {}, "onecrew-local-service"),
            ("workspace package alias", ("dev-dependencies",), "vault",
             {"workspace": True}, {"vault": {"package": "onecrew-local-vault", "path": "crates/local-vault"}},
             "onecrew-local-vault"),
            ("target dependency", ("target", "cfg(unix)", "dependencies"), "onecrew-dm-contracts",
             {"path": "../dm-contracts"}, {}, "onecrew-dm-contracts"),
            ("build dependency", ("build-dependencies",), "onecrew-local-vault",
             {"path": "../local-vault"}, {}, "onecrew-local-vault"),
            *(
                (f"platform crate {package}", ("dependencies",), package, {"path": f"../{directory}"}, {}, package)
                for directory, package in platform_crates
            ),
        ]
        for label, section, key, declaration, overlay, rejected in cases:
            fixture = with_dependency(adapter, section, key, declaration)
            with self.subTest(case=label):
                self.assertIn(rejected, internal_names)
                self.assertEqual(
                    disallowed_internal_dependencies(
                        fixture, MCP_ADAPTER_MANIFEST.parent, {**workspace, **overlay}, internal_names,
                        MCP_ADAPTER_INTERNAL_ALLOWLIST,
                    ),
                    {rejected},
                )

    def test_mcp_adapter_allowlist_rejects_path_dependencies_outside_members(self) -> None:
        workspace = read_toml(ROOT_MANIFEST)["workspace"].get("dependencies", {})
        internal_names = workspace_package_names()
        adapter = read_toml(MCP_ADAPTER_MANIFEST)
        with tempfile.TemporaryDirectory() as directory:
            crate = Path(directory) / "unlisted"
            crate.mkdir()
            (crate / "Cargo.toml").write_text('[package]\nname = "onecrew-unlisted-crate"\n')
            missing = Path(directory) / "missing"
            # Relative paths prove each declaration resolves against its own base
            # directory: a wrong base leaves the target unreadable, and the
            # fallback name then differs from the target's package name.
            # (label, section path, dependency key, declaration, workspace overlay, rejected package)
            cases = [
                ("direct path", ("dependencies",), "unlisted",
                 {"path": os.path.relpath(crate, MCP_ADAPTER_MANIFEST.parent)}, {}, "onecrew-unlisted-crate"),
                ("workspace path", ("dev-dependencies",), "unlisted", {"workspace": True},
                 {"unlisted": {"path": os.path.relpath(crate, ROOT_MANIFEST.parent)}}, "onecrew-unlisted-crate"),
                ("unreadable target, package alias", ("target", "cfg(unix)", "dependencies"), "gone",
                 {"package": "onecrew-gone-crate", "path": os.path.relpath(missing, MCP_ADAPTER_MANIFEST.parent)},
                 {}, "onecrew-gone-crate"),
                ("unreadable target, key", ("build-dependencies",), "onecrew-gone-key",
                 {"path": str(missing)}, {}, "onecrew-gone-key"),
            ]
            for label, section, key, declaration, overlay, rejected in cases:
                fixture = with_dependency(adapter, section, key, declaration)
                with self.subTest(case=label):
                    self.assertNotIn(rejected, internal_names)
                    self.assertEqual(
                        disallowed_internal_dependencies(
                            fixture, MCP_ADAPTER_MANIFEST.parent, {**workspace, **overlay}, internal_names,
                            MCP_ADAPTER_INTERNAL_ALLOWLIST,
                        ),
                        {rejected},
                    )

    def test_mcp_adapter_allowlist_rejects_excluded_workspace_crates(self) -> None:
        root = read_toml(ROOT_MANIFEST)["workspace"]
        workspace = root.get("dependencies", {})
        internal_names = workspace_package_names()
        adapter = read_toml(MCP_ADAPTER_MANIFEST)
        excluded = "libs/chat-xdk/crates/core"
        self.assertTrue(any(Path(excluded).is_relative_to(entry) for entry in root["exclude"]))
        package = read_toml(REPO_ROOT / excluded / "Cargo.toml")["package"]["name"]
        self.assertNotIn(package, internal_names)
        cases = [
            ("direct path", "xdk", {"path": os.path.relpath(REPO_ROOT / excluded, MCP_ADAPTER_MANIFEST.parent)},
             {}),
            ("workspace path", "xdk", {"workspace": True}, {"xdk": {"path": excluded}}),
        ]
        for label, key, declaration, overlay in cases:
            fixture = with_dependency(adapter, ("dependencies",), key, declaration)
            with self.subTest(case=label):
                self.assertEqual(
                    disallowed_internal_dependencies(
                        fixture, MCP_ADAPTER_MANIFEST.parent, {**workspace, **overlay}, internal_names,
                        MCP_ADAPTER_INTERNAL_ALLOWLIST,
                    ),
                    {package},
                )

    def test_catalog_imports_remain_neutral(self) -> None:
        catalog = REPO_ROOT / "crates/platform-runtime/src/catalog"
        allowed_roots = {"std", "serde", "serde_json", "schemars", "self", "super"}
        allowed_roots.update(path.stem for path in catalog.glob("*.rs") if path.name != "mod.rs")
        forbidden = re.compile(r"\b(?:LocalPlatform|PlatformSessions|CredentialServices|RegistryBuilder|OperationRegistry|BoxFuture|JoinHandle)\b")
        # This is an import boundary check, not proof of workflow semantics;
        # catalog unit tests and review verify the actual read-only API.
        for path in sorted(catalog.rglob("*.rs")):
            source = path.read_text()
            imports = re.findall(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+([^;]+);", source)
            for imported in imports:
                with self.subTest(path=str(path.relative_to(REPO_ROOT)), imported=imported):
                    self.assertIn(imported.split("::", 1)[0].strip(), allowed_roots)
                    self.assertIsNone(forbidden.search(imported))

    def test_toolchain_matches_workspace_rust_version(self) -> None:
        root = read_toml(ROOT_MANIFEST)
        workspace_package = root["workspace"]["package"]
        toolchain = read_toml(TOOLCHAIN_MANIFEST)["toolchain"]

        self.assertEqual(toolchain["channel"], workspace_package["rust-version"])
        self.assertRegex(toolchain["channel"], r"^\d+\.\d+\.\d+$")
        self.assertEqual(toolchain["profile"], "minimal")
        self.assertTrue({"clippy", "rustfmt"}.issubset(toolchain["components"]))
        self.assertTrue(toolchain["targets"])
        self.assertEqual(len(toolchain["targets"]), len(set(toolchain["targets"])))

    def test_every_root_member_inherits_internal_package_policy(self) -> None:
        root = read_toml(ROOT_MANIFEST)
        workspace_package = root["workspace"]["package"]
        self.assertIs(workspace_package["publish"], False)

        for manifest_path in workspace_member_manifests():
            package = read_toml(manifest_path)["package"]
            relative = manifest_path.relative_to(REPO_ROOT)
            with self.subTest(manifest=str(relative)):
                self.assertEqual(package["rust-version"], {"workspace": True})
                self.assertEqual(package["publish"], {"workspace": True})
                self.assertEqual(package["edition"], "2024")

    def test_every_root_member_inherits_workspace_lints(self) -> None:
        root = read_toml(ROOT_MANIFEST)
        self.assertEqual(root["workspace"]["lints"], EXPECTED_WORKSPACE_LINTS)

        for manifest_path in workspace_member_manifests():
            relative = manifest_path.relative_to(REPO_ROOT)
            with self.subTest(manifest=str(relative)):
                self.assertEqual(
                    read_toml(manifest_path).get("lints"),
                    {"workspace": True},
                )


if __name__ == "__main__":
    unittest.main()
