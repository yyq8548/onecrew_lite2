from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("agent-context.py")
SPEC = importlib.util.spec_from_file_location("agent_platform_agent_context", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
AGENT_CONTEXT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AGENT_CONTEXT)


class AgentContextTests(unittest.TestCase):
    def test_uv_runtime_lookup_is_not_a_bare_python_tool_invocation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            guide = root / "guide.md"
            guide.write_text('export RUNNER="$(uv python find 3.11)"\nuv run --python=3.11 --script tool.py\n', encoding="utf-8")
            with patch.object(AGENT_CONTEXT, "ROOT", root):
                errors: list[str] = []
                AGENT_CONTEXT.validate_project_python_invocations(errors)
                self.assertEqual(errors, [])
                guide.write_text('uv python find 3.11 && python3 tool.py\n', encoding="utf-8")
                AGENT_CONTEXT.validate_project_python_invocations(errors)
                self.assertEqual(errors, ["guide.md: invoke project Python tooling through uv run"])

    def test_current_repository_passes(self) -> None:
        errors, stats = AGENT_CONTEXT.audit()

        self.assertEqual(errors, [])
        self.assertLessEqual(stats["discoverableSkills"], 40)
        self.assertGreater(stats["developmentSkills"], 0)

    def test_parses_folded_description_and_boolean(self) -> None:
        errors: list[str] = []
        fields = AGENT_CONTEXT.parse_frontmatter(
            "---\nname: sample\ndescription: >\n  First line.\n  Second line.\ndisable-model-invocation: true\n---\nBody\n",
            "sample/SKILL.md",
            errors,
        )

        self.assertEqual(errors, [])
        self.assertEqual(fields["description"], "First line. Second line.")
        self.assertIs(fields["disable-model-invocation"], True)

    def test_budget_config_matches_schema(self) -> None:
        config_path = Path(__file__).with_name("agent-context.config.json")
        config = json.loads(config_path.read_text(encoding="utf-8"))
        errors: list[str] = []

        self.assertTrue(AGENT_CONTEXT.validate_config(config, errors))
        self.assertEqual(errors, [])

    def test_frontmatter_only_skill_is_rejected(self) -> None:
        errors: list[str] = []
        AGENT_CONTEXT.parse_frontmatter("---\nname: empty\ndescription: Has no guidance.\n---\n", "empty/SKILL.md", errors)
        self.assertIn("empty/SKILL.md: skill body is empty", errors)

    def test_concise_references_do_not_require_a_template(self) -> None:
        source = "# Locale keys\nEdit both catalogs and preserve interpolation names.\n"
        errors: list[str] = []
        for kind in ("pattern", "workflow", "map"):
            AGENT_CONTEXT.validate_reference_source(source, "locale.md", kind, errors)
        self.assertEqual(errors, [])

    def test_empty_oversized_and_unknown_references_are_rejected(self) -> None:
        errors: list[str] = []
        AGENT_CONTEXT.validate_reference_source("  \n", "empty.md", "map", errors)
        AGENT_CONTEXT.validate_reference_source(
            "detail\n" * 301, "large.md", "workflow", errors
        )
        AGENT_CONTEXT.validate_reference_source("text", "unknown.md", "other", errors)
        self.assertIn("empty.md: project reference is empty", errors)
        self.assertIn(
            "large.md: project reference exceeds 300 lines; split the topic",
            errors,
        )
        self.assertIn(
            "unknown.md: unsupported project reference kind: other", errors
        )

    def test_portable_python_exemption_is_exact_and_keeps_tooling_guarded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "support.py").write_text("#!/usr/bin/env python3\n", encoding="utf-8")
            (root / "tool.py").write_text("#!/usr/bin/env python3\n", encoding="utf-8")
            (root / "guide.md").write_text("python3 support.py --apply\n", encoding="utf-8")
            with patch.object(AGENT_CONTEXT, "ROOT", root):
                errors: list[str] = []
                AGENT_CONTEXT.validate_project_python_invocations(
                    errors, {"support.py": "Standalone user script"}
                )
            self.assertEqual(len(errors), 2)
            self.assertTrue(any("tool.py" in error for error in errors))
            self.assertTrue(any("guide.md" in error for error in errors))

    def test_portable_python_declarations_reject_wildcards_and_missing_reason(self) -> None:
        config = json.loads(AGENT_CONTEXT.CONFIG_PATH.read_text(encoding="utf-8"))
        for declaration in ({"**/*.py": "too broad"}, {"script.py": ""}, {"../script.py": "outside"}):
            errors: list[str] = []
            config["portablePythonFiles"] = declaration
            self.assertFalse(AGENT_CONTEXT.validate_config(config, errors))
        with tempfile.TemporaryDirectory() as directory, patch.object(AGENT_CONTEXT, "ROOT", Path(directory)):
            errors = []
            AGENT_CONTEXT.validate_project_python_invocations(errors, {"missing.py": "Standalone"})
            self.assertEqual(errors, ["Portable Python declaration has no file: missing.py"])

    def test_reference_must_be_linked_exactly_once(self) -> None:
        errors: list[str] = []
        AGENT_CONTEXT.validate_reference_links(
            "[first](references/rule.md) and [again](references/rule.md)",
            {"references/rule.md"},
            "sample",
            errors,
        )

        self.assertEqual(
            errors,
            ["sample: SKILL.md must link every project reference exactly once"],
        )

    def test_detects_direct_python_commands_but_not_uv(self) -> None:
        self.assertIsNotNone(
            AGENT_CONTEXT.PYTHON_COMMAND_PATTERN.search(
                "PYTHONDONTWRITEBYTECODE=1 python3 -m unittest"
            )
        )
        self.assertIsNone(
            AGENT_CONTEXT.PYTHON_COMMAND_PATTERN.search("uv run -m unittest")
        )

    def test_project_skill_manifest_covers_both_catalogs(self) -> None:
        errors: list[str] = []
        manifest = AGENT_CONTEXT.load_project_skill_ownership(errors)

        self.assertEqual(errors, [])
        self.assertIn("onecrew-creator", manifest["skills"])
        self.assertEqual(
            set(manifest["developmentSkills"]),
            {
                "onecrew-guidance-drift",
                "onecrew-new-skill",
            },
        )
        self.assertTrue(
            all(
                entry["invocation"] == "manual"
                for entry in manifest["developmentSkills"].values()
            )
        )

    def test_review_and_verify_do_not_form_an_automatic_return_route(self) -> None:
        errors: list[str] = []
        manifest = AGENT_CONTEXT.load_project_skill_ownership(errors)
        self.assertEqual(errors, [])
        review_targets = {
            relationship["to"]
            for relationship in manifest["skills"]["onecrew-review"]["relationships"]
        }
        verify_targets = {
            relationship["to"]
            for relationship in manifest["skills"]["onecrew-verify"]["relationships"]
        }

        self.assertIn("onecrew-verify", review_targets)
        self.assertNotIn("onecrew-review", verify_targets)

    def test_skill_mirrors_require_relative_symlinks_to_canonical_source(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            canonical = root / ".skillshare" / "skills"
            canonical.mkdir(parents=True)
            for parent in (root / ".agents", root / ".claude"):
                parent.mkdir()
                (parent / "skills").symlink_to(Path("../.skillshare/skills"))

            errors: list[str] = []
            AGENT_CONTEXT.validate_skill_mirrors(root, canonical, errors)
            self.assertEqual(errors, [])

            (root / ".claude" / "skills").unlink()
            (root / ".claude" / "skills").mkdir()
            AGENT_CONTEXT.validate_skill_mirrors(root, canonical, errors)
            self.assertIn(
                ".claude/skills: expected relative symlink to .skillshare/skills",
                errors,
            )


if __name__ == "__main__":
    unittest.main()
