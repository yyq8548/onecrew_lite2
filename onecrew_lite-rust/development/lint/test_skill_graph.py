from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


MODULE_PATH = Path(__file__).with_name("agent-context.py")
SPEC = importlib.util.spec_from_file_location("agent_platform_agent_context_graph", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
AGENT_CONTEXT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AGENT_CONTEXT)


def graph_entry(name: str, positive: str, negative: str) -> dict[str, object]:
    return {
        "invocation": "implicit",
        "ownerConcern": f"{name}-owner",
        "what": f"Own {name}.",
        "concerns": [f"{name}-concern"],
        "positiveCases": [positive],
        "negativeCases": [negative],
        "pathScopes": {
            "owns": [f"{name}/**"],
            "appliesTo": [],
            "governs": [],
            "observes": [],
        },
        "references": {},
        "requiredReferences": [],
        "verificationOwner": "onecrew-beta",
        "effects": {
            "mutation": False,
            "externalEffect": False,
            "review": False,
            "package": False,
        },
        "relationships": [],
        "stopCondition": f"{name}_complete",
    }


def routing_case(
    case_id: str,
    owner: str,
    rejected: str,
    stop: str,
    *,
    cross_cutting: list[str] | None = None,
) -> dict[str, object]:
    return {
        "id": case_id,
        "request": f"Route {case_id}.",
        "owner": owner,
        "crossCutting": cross_cutting or [],
        "reject": [rejected],
        "references": [],
        "verificationOwner": "onecrew-beta",
        "stopCondition": stop,
    }


class SkillGraphTests(unittest.TestCase):
    def setUp(self) -> None:
        self.manifest = {
            "schemaVersion": 2,
            "skills": {
                "onecrew-alpha": graph_entry(
                    "alpha", "alpha-positive", "beta-positive"
                ),
                "onecrew-beta": graph_entry(
                    "beta", "beta-positive", "alpha-positive"
                ),
            },
            "developmentSkills": {},
        }
        self.routing = {
            "schemaVersion": 1,
            "cases": [
                routing_case(
                    "alpha-positive",
                    "onecrew-alpha",
                    "onecrew-beta",
                    "alpha_complete",
                ),
                routing_case(
                    "beta-positive",
                    "onecrew-beta",
                    "onecrew-alpha",
                    "beta_complete",
                ),
            ],
        }
        self.sources = {
            "onecrew-alpha": "## Related Skills\n\n## Workflow\n",
            "onecrew-beta": "## Related Skills\n\n## Workflow\n",
        }

    def validate(
        self,
        manifest: dict[str, object] | None = None,
        routing: dict[str, object] | None = None,
        sources: dict[str, str] | None = None,
    ) -> list[str]:
        errors: list[str] = []
        AGENT_CONTEXT.validate_skill_graph_data(
            manifest or self.manifest,
            routing or self.routing,
            sources or self.sources,
            skills_directory=".skillshare/skills",
            development_directory="development/skills",
            max_overlap_ratio=0.5,
            errors=errors,
        )
        return errors

    def test_valid_graph_passes(self) -> None:
        self.assertEqual(self.validate(), [])

    def test_missing_relationship_target_fails(self) -> None:
        manifest = copy.deepcopy(self.manifest)
        manifest["skills"]["onecrew-alpha"]["relationships"] = [
            {"to": "onecrew-missing", "kind": "pair", "when": "Needed."}
        ]
        sources = dict(self.sources)
        sources["onecrew-alpha"] = (
            "## Related Skills\n- onecrew-missing — needed.\n## Workflow\n"
        )

        errors = self.validate(manifest=manifest, sources=sources)

        self.assertTrue(any("target does not exist" in error for error in errors))

    def test_duplicate_relationship_fails(self) -> None:
        manifest = copy.deepcopy(self.manifest)
        relationship = {
            "to": "onecrew-beta",
            "kind": "pair",
            "when": "Needed.",
        }
        manifest["skills"]["onecrew-alpha"]["relationships"] = [
            relationship,
            dict(relationship),
        ]
        sources = dict(self.sources)
        sources["onecrew-alpha"] = (
            "## Related Skills\n- onecrew-beta — needed.\n## Workflow\n"
        )

        errors = self.validate(manifest=manifest, sources=sources)

        self.assertTrue(any("duplicate relationship" in error for error in errors))

    def test_positive_negative_case_overlap_fails(self) -> None:
        routing = copy.deepcopy(self.routing)
        routing["cases"][0]["crossCutting"] = ["onecrew-beta"]
        routing["cases"][0]["reject"] = []
        routing["cases"][1]["crossCutting"] = ["onecrew-alpha"]
        routing["cases"][1]["reject"] = []

        errors = self.validate(routing=routing)

        self.assertTrue(any("positive routing overlap" in error for error in errors))

    def test_declared_owner_pairing_is_not_duplicate_ownership(self) -> None:
        manifest = copy.deepcopy(self.manifest)
        routing = copy.deepcopy(self.routing)
        alpha = manifest["skills"]["onecrew-alpha"]
        alpha["relationships"] = [
            {"to": "onecrew-beta", "kind": "pair", "when": "The same UI change needs locale behavior."}
        ]
        for index in range(3):
            case_id = f"paired-{index}"
            case = routing_case(case_id, "onecrew-alpha", "", "alpha_complete", cross_cutting=["onecrew-beta"])
            case["reject"] = []
            routing["cases"].append(case)
            alpha["positiveCases"].append(case_id)
        sources = dict(self.sources)
        sources["onecrew-alpha"] = "## Related Skills\n- onecrew-beta — locale rendering.\n"
        self.assertEqual(self.validate(manifest, routing, sources), [])

        alpha["relationships"][0]["kind"] = "handoff"
        errors = self.validate(manifest, routing, sources)
        self.assertTrue(any("positive routing overlap" in error for error in errors))

    def test_unknown_reference_and_missing_required_reference_fail(self) -> None:
        manifest = copy.deepcopy(self.manifest)
        alpha = manifest["skills"]["onecrew-alpha"]
        alpha["references"] = {"references/owner.md": {"kind": "map"}}
        alpha["requiredReferences"] = ["references/owner.md"]
        routing = copy.deepcopy(self.routing)
        routing["cases"][0]["references"] = [
            ".skillshare/skills/onecrew-alpha/references/other.md"
        ]

        errors = self.validate(manifest=manifest, routing=routing)

        self.assertTrue(any("references must belong" in error for error in errors))
        self.assertTrue(any("missing owner required references" in error for error in errors))

    def test_rust_architecture_routing_contract(self) -> None:
        repo_root = MODULE_PATH.parents[2]
        manifest = json.loads(
            (repo_root / "development/lint/project-skill-ownership.json").read_text()
        )
        routing = json.loads(
            (repo_root / "development/lint/project-skill-routing-cases.json").read_text()
        )
        rust = manifest["skills"]["onecrew-rust-architecture"]
        cases = {case["id"]: case for case in routing["cases"]}
        module_reference = (
            ".skillshare/skills/onecrew-rust-architecture/"
            "references/rust-crate-module-and-api.md"
        )
        structure_reference = (
            ".skillshare/skills/onecrew-rust-architecture/"
            "references/structure-only-rust.md"
        )
        visibility_reference = (
            ".skillshare/skills/onecrew-rust-architecture/"
            "references/rust-visibility-and-reexports.md"
        )
        source = (
            repo_root / ".skillshare/skills/onecrew-rust-architecture/SKILL.md"
        ).read_text()
        visibility_source = (
            repo_root
            / ".skillshare/skills/onecrew-rust-architecture/"
            "references/rust-visibility-and-reexports.md"
        ).read_text()

        self.assertIn("rust-test-organization", rust["concerns"])
        self.assertIn("rust-restricted-visibility", rust["concerns"])
        self.assertIn("crates/**/tests/**", rust["pathScopes"]["appliesTo"])
        self.assertIn(
            "crates/local-service/tests/**", rust["pathScopes"]["appliesTo"]
        )
        self.assertEqual(
            cases["rust-module-api-boundary"]["references"],
            [module_reference, visibility_reference],
        )
        for case_id in [
            "rust-structure-only-refactor",
            "rust-test-organization",
            "rust-test-visibility-boundary",
        ]:
            self.assertEqual(
                cases[case_id]["references"],
                [module_reference, visibility_reference, structure_reference],
            )
        self.assertEqual(
            cases["rust-internal-visibility-paths"]["references"],
            [module_reference, visibility_reference, structure_reference],
        )
        self.assertIn(
            "onecrew-architecture-placement",
            cases["rust-internal-visibility-paths"]["reject"],
        )
        self.assertIn("❌ BAD", source)
        self.assertIn("✅ GOOD", source)
        self.assertIn("const _: Option<CommandInput> = None;", visibility_source)
        self.assertIn("composition root", visibility_source)
        self.assertIn(
            "onecrew-rust-architecture",
            cases["ordinary-product-implementation"]["reject"],
        )
        self.assertEqual(
            cases["rust-cohesive-module-no-split"]["owner"],
            "onecrew-rust-architecture",
        )

    def test_rust_quality_packet_and_routing_contract(self) -> None:
        repo_root = MODULE_PATH.parents[2]
        manifest = json.loads(
            (repo_root / "development/lint/project-skill-ownership.json").read_text()
        )
        routing = json.loads(
            (repo_root / "development/lint/project-skill-routing-cases.json").read_text()
        )
        quality = manifest["skills"]["onecrew-rust-quality"]
        architecture = manifest["skills"]["onecrew-rust-architecture"]
        cases = {case["id"]: case for case in routing["cases"]}
        quality_root = ".skillshare/skills/onecrew-rust-quality"
        references = {
            "error": f"{quality_root}/references/error-panic-and-boundaries.md",
            "unsafe": f"{quality_root}/references/unsafe-proof-and-safe-abstractions.md",
            "api": f"{quality_root}/references/api-rustdoc-and-traits.md",
            "cargo": f"{quality_root}/references/cargo-lints-features-and-msrv.md",
        }
        source = (repo_root / quality_root / "SKILL.md").read_text()
        async_source = (
            repo_root
            / ".skillshare/skills/onecrew-rust-architecture/"
            "references/rust-async-supervision.md"
        ).read_text()
        native_source = (
            repo_root
            / ".skillshare/skills/onecrew-native-cli/"
            "references/platform-boundaries.md"
        ).read_text()
        readiness_source = (
            repo_root / quality_root / "scripts/check-readiness.mjs"
        ).read_text()
        error_source = (repo_root / references["error"]).read_text()
        unsafe_source = (repo_root / references["unsafe"]).read_text()
        module_source = (
            repo_root
            / ".skillshare/skills/onecrew-rust-architecture/"
            "references/rust-crate-module-and-api.md"
        ).read_text()

        self.assertEqual(quality["ownerConcern"], "rust-quality")
        self.assertEqual(
            set(quality["references"]),
            {
                "references/error-panic-and-boundaries.md",
                "references/unsafe-proof-and-safe-abstractions.md",
                "references/api-rustdoc-and-traits.md",
                "references/cargo-lints-features-and-msrv.md",
            },
        )
        self.assertEqual(
            quality["featurePacket"]["caseTypes"],
            ["golden", "negative", "verification"],
        )
        self.assertEqual(quality["featurePacket"]["passThreshold"], 90)
        self.assertEqual(
            cases["rust-error-boundary-quality"]["references"],
            [references["error"]],
        )
        self.assertEqual(
            cases["rust-panic-context-quality"]["references"],
            [references["error"]],
        )
        self.assertEqual(
            cases["rust-unsafe-proof-quality"]["references"],
            [references["unsafe"]],
        )
        self.assertEqual(
            cases["rust-api-rustdoc-trait-quality"]["references"],
            [references["api"]],
        )
        for case_id in ["rust-cargo-quality-policy", "rust-lint-suppression-quality"]:
            self.assertEqual(cases[case_id]["references"], [references["cargo"]])
        self.assertEqual(
            cases["rust-unsafe-proof-quality"]["crossCutting"],
            ["onecrew-native-cli"],
        )
        architecture_only_cases = {
            "rust-module-api-boundary",
            "rust-internal-visibility-paths",
            "rust-async-supervisor",
            "rust-structure-only-refactor",
            "rust-test-organization",
            "rust-cohesive-module-no-split",
            "rust-test-visibility-boundary",
        }
        native_only_cases = {
            "native-platform-mechanics",
            "cli-distribution",
            "windows-browser-security",
            "windows-cross-build",
        }
        verification_only_cases = {
            "final-diff-skill-gate",
            "targeted-owner-check",
            "test-execution-request",
        }
        explicit_negative_cases = (
            architecture_only_cases
            | native_only_cases
            | verification_only_cases
            | {
                "ordinary-product-implementation",
                "x-chat-crypto-change",
                "mechanism-comment-sweep",
            }
        )
        self.assertTrue(explicit_negative_cases.issubset(quality["negativeCases"]))
        for negative_case in explicit_negative_cases:
            self.assertIn("onecrew-rust-quality", cases[negative_case]["reject"])
        self.assertIn(
            "onecrew-rust-quality",
            {relationship["to"] for relationship in architecture["relationships"]},
        )
        self.assertGreaterEqual(source.count("// ❌ BAD"), 5)
        self.assertGreaterEqual(source.count("// ✅ GOOD"), 5)
        self.assertIn("private cohesive", module_source)
        self.assertIn(
            "const _: Option<CommandInput> = None;",
            (
                repo_root
                / ".skillshare/skills/onecrew-rust-architecture/"
                "references/rust-visibility-and-reexports.md"
            ).read_text(),
        )
        self.assertNotIn("HTTP async command", async_source)
        self.assertIn("supervisor panic edge", async_source)
        self.assertIn("onecrew-rust-quality", native_source)
        self.assertNotIn("### Narrow unsafe with an explicit SAFETY comment", native_source)
        self.assertIn("product_domain_worktree_dirty", readiness_source)
        self.assertIn("Existing Rust debt and code quality are not graded", readiness_source)
        self.assertNotIn("Never assert success with `unwrap`/`expect`", error_source)
        self.assertIn("proved unrecoverable invariant", error_source)
        self.assertIn("slice/fn.from_raw_parts.html", unsafe_source)
        self.assertIn("BufferTooLarge", unsafe_source)
        self.assertIn("one allocation", unsafe_source)
        for relative in quality["references"]:
            reference_source = (repo_root / quality_root / relative).read_text()
            for volatile_project_term in [
                "OneCrew",
                "Campaign",
                "HTTP",
                "MCP",
                "OneCrew Native",
                "X Chat",
                "Verify",
                "apps/",
                "crates/",
            ]:
                self.assertNotIn(volatile_project_term, reference_source)

    def test_cross_boundary_contract_codegen_routing_contract(self) -> None:
        repo_root = MODULE_PATH.parents[2]
        manifest = json.loads(
            (repo_root / "development/lint/project-skill-ownership.json").read_text()
        )
        routing = json.loads(
            (repo_root / "development/lint/project-skill-routing-cases.json").read_text()
        )
        peer = manifest["skills"]["onecrew-peer-adapters"]
        cases = {case["id"]: case for case in routing["cases"]}
        case = cases["cross-boundary-contract-codegen"]
        reference = (
            ".skillshare/skills/onecrew-peer-adapters/"
            "references/cross-boundary-contract-codegen.md"
        )

        self.assertIn("adapter-contract-codegen", peer["concerns"])
        self.assertIn("crates/schema-manifest/**", peer["pathScopes"]["owns"])
        self.assertIn("development/schema-codegen/**", peer["pathScopes"]["owns"])
        self.assertIn(
            "apps/web/src/lib/generated/creator-sync-contracts/**",
            peer["pathScopes"]["governs"],
        )
        self.assertIn(
            "apps/mcp-ui/src/lib/generated/contracts/**",
            peer["pathScopes"]["governs"],
        )
        self.assertEqual(case["owner"], "onecrew-peer-adapters")
        self.assertEqual(
            case["crossCutting"],
            ["onecrew-mcp-app-ui", "onecrew-web-ui"],
        )
        self.assertEqual(case["references"], [reference])
        self.assertIn("onecrew-native-cli", case["reject"])
        self.assertIn("onecrew-local-data-evolution", case["reject"])
        self.assertIn("deterministic", case["request"])
        self.assertIn("non-mutating drift", case["request"])

    def test_upstream_design_skills_preserve_explicit_composition(self) -> None:
        repo_root = MODULE_PATH.parents[2]
        manifest = json.loads(
            (repo_root / "development/lint/project-skill-ownership.json").read_text()
        )
        routing = json.loads(
            (repo_root / "development/lint/project-skill-routing-cases.json").read_text()
        )
        cases = {case["id"]: case for case in routing["cases"]}
        wrapper = manifest["skills"]["grill-with-docs"]
        self.assertEqual(wrapper["invocation"], "explicit")
        self.assertEqual(
            {(edge["to"], edge["kind"]) for edge in wrapper["relationships"]},
            {("grilling", "pair"), ("domain-modeling", "pair")},
        )
        self.assertEqual(
            cases["explicit-design-interview-with-docs"]["crossCutting"],
            ["grilling", "domain-modeling"],
        )
        resumed = cases["resume-authorized-provider-implementation"]
        self.assertEqual(resumed["owner"], "onecrew-capability")
        self.assertEqual(resumed["crossCutting"], [])
        self.assertEqual(
            set(resumed["reject"]), set(AGENT_CONTEXT.UPSTREAM_SKILL_PATHS)
        )
        explicit = {"grill-with-docs", "to-spec", "to-tickets", "implement", "setup-matt-pocock-skills"}
        for name in AGENT_CONTEXT.UPSTREAM_SKILL_PATHS:
            contract = manifest["skills"][name]
            self.assertTrue(AGENT_CONTEXT.approved_upstream_skill(name, contract))
            self.assertEqual(contract["invocation"], "explicit" if name in explicit else "implicit")

    def validate_import(self, name: str, upstream: object) -> list[str]:
        contract = graph_entry(name, "import-positive", "import-negative")
        if upstream is not None:
            contract["upstream"] = upstream
        errors: list[str] = []
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            skill = root / "skills" / name
            skill.mkdir(parents=True)
            (skill / "SKILL.md").write_text(
                f"---\nname: {name}\ndescription: Imported skill.\n---\n\nBody.\n"
            )
            with patch.object(AGENT_CONTEXT, "ROOT", root):
                AGENT_CONTEXT.validate_project_skill_contract(
                    root / "skills", {name: contract}, {name: "implicit"}, errors
                )
        return errors

    def test_only_pinned_approved_imports_bypass_project_prefix(self) -> None:
        expected_names = {
            "grill-with-docs", "grilling", "domain-modeling", "to-spec", "to-tickets",
            "implement", "code-review", "tdd", "codebase-design", "setup-matt-pocock-skills",
        }
        self.assertEqual(set(AGENT_CONTEXT.UPSTREAM_SKILL_PATHS), expected_names)
        self.assertEqual(AGENT_CONTEXT.UPSTREAM_SKILL_PATHS["grilling"], "skills/productivity/grilling")
        for name, path in AGENT_CONTEXT.UPSTREAM_SKILL_PATHS.items():
            upstream = {
                "repository": "https://github.com/mattpocock/skills",
                "revision": "3cca18b368ae95cdbdebbff572ccafa662551015",
                "path": path,
            }
            with self.subTest(name=name, provenance="valid"):
                self.assertEqual(self.validate_import(name, upstream), [])
            invalid_values = [
                None,
                {},
                {**upstream, "repository": "https://github.com/other/skills"},
                {**upstream, "path": "skills/engineering/other"},
                {**upstream, "path": path + "/../other"},
                {**upstream, "revision": "main"},
                {**upstream, "revision": "g" * 40},
                {**upstream, "revision": "a" * 39},
                {**upstream, "revision": 123},
            ]
            for invalid in invalid_values:
                with self.subTest(name=name, provenance=invalid):
                    errors = self.validate_import(name, invalid)
                    self.assertTrue(any("must use the onecrew- prefix" in e for e in errors))
            with self.subTest(name="unapproved", provenance=upstream):
                errors = self.validate_import("unapproved", upstream)
                self.assertTrue(any("must use the onecrew- prefix" in e for e in errors))

    def test_upstream_root_templates_keep_reference_guards(self) -> None:
        contract = {
            "upstream": {
                "repository": "https://github.com/mattpocock/skills",
                "revision": "3cca18b368ae95cdbdebbff572ccafa662551015",
                "path": "skills/engineering/domain-modeling",
            },
            "references": {"CONTEXT-FORMAT.md": {"kind": "pattern"}, "ADR-FORMAT.md": {"kind": "pattern"}},
        }
        roots = AGENT_CONTEXT.upstream_root_references("domain-modeling", contract)
        self.assertEqual(roots, {"CONTEXT-FORMAT.md", "ADR-FORMAT.md"})
        self.assertEqual(AGENT_CONTEXT.upstream_root_references("domain-modeling", {}), set())
        self.assertEqual(AGENT_CONTEXT.upstream_root_references("unapproved", contract), set())
        references = {name: {"kind": "pattern"} for name in roots}
        links = "\n".join(f"[{name}](./{name})" for name in sorted(roots))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in roots:
                (root / name).write_text("# Template\n\nMeaningful template content.\n")

            def validate(source: str = links) -> list[str]:
                errors: list[str] = []
                with patch.object(AGENT_CONTEXT, "ROOT", root):
                    AGENT_CONTEXT.validate_skill_references(
                        root, references, "domain-modeling", source, errors,
                        root_references=roots,
                    )
                return errors

            self.assertEqual(validate(), [])
            duplicate = links + "\n[again](CONTEXT-FORMAT.md)"
            self.assertTrue(any("exactly once" in e for e in validate(duplicate)))
            self.assertTrue(any("exactly once" in e for e in validate("")))
            (root / "CONTEXT-FORMAT.md").write_text("")
            self.assertTrue(any("reference is empty" in e for e in validate()))
            (root / "CONTEXT-FORMAT.md").unlink()
            self.assertTrue(any("references do not exist" in e for e in validate()))

    def test_approved_layout_uses_declared_root_inventory_without_blanket_exemption(self) -> None:
        repo_root = MODULE_PATH.parents[2]
        manifest = json.loads((repo_root / "development/lint/project-skill-ownership.json").read_text())
        layouts = {
            "domain-modeling": {"CONTEXT-FORMAT.md", "ADR-FORMAT.md"},
            "tdd": {"tests.md", "mocking.md"},
            "codebase-design": {"DEEPENING.md", "DESIGN-IT-TWICE.md"},
            "setup-matt-pocock-skills": {
                "domain.md", "issue-tracker-github.md", "issue-tracker-gitlab.md",
                "issue-tracker-local.md", "triage-labels.md",
            },
        }
        for name, expected in layouts.items():
            contract = manifest["skills"][name]
            with self.subTest(skill=name):
                self.assertEqual(AGENT_CONTEXT.upstream_root_references(name, contract), expected)
                without_refs = {**contract, "references": {}}
                self.assertEqual(AGENT_CONTEXT.upstream_root_references(name, without_refs), set())
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    template = next(iter(expected))
                    (root / "SKILL.md").write_text(f"[Template]({template})\n")
                    (root / template).write_text("[Back](SKILL.md)\n")
                    with patch.object(AGENT_CONTEXT, "ROOT", root):
                        errors: list[str] = []
                        AGENT_CONTEXT.validate_markdown_links(root, errors, root_templates=expected)
                        self.assertEqual(errors, [])
                        (root / "unlisted.md").write_text("# Unlisted root file\n")
                        (root / template).write_text("[Undeclared](unlisted.md)\n")
                        AGENT_CONTEXT.validate_markdown_links(root, errors, root_templates=expected)
                        self.assertTrue(any("undeclared root reference" in e for e in errors))

    def test_imported_edges_require_original_evidence_and_approved_targets(self) -> None:
        root = MODULE_PATH.parents[2]
        manifest = json.loads((root / "development/lint/project-skill-ownership.json").read_text())
        routing = json.loads((root / "development/lint/project-skill-routing-cases.json").read_text())
        sources = {
            name: (root / directory / name / "SKILL.md").read_text()
            for section, directory in (("skills", ".skillshare/skills"), ("developmentSkills", "development/skills"))
            for name in manifest[section]
        }
        self.assertEqual(self.validate(manifest, routing, sources), [])
        for field, value in (
            ("sourceEvidence", None),
            ("sourceEvidence", "Use /tdd at every possible private function."),
            ("sourceEvidence", "Once done, use /code-review to review the work."),
            ("to", "onecrew-creator"),
            ("to", "missing-skill"),
        ):
            changed = copy.deepcopy(manifest)
            changed["skills"]["implement"]["relationships"][0][field] = value
            with self.subTest(field=field, value=value):
                self.assertTrue(any("sourceEvidence" in e for e in self.validate(changed, routing, sources)))
        changed_sources = dict(sources)
        changed_sources["implement"] = sources["implement"].replace("Use /tdd", "Do not use /tdd")
        self.assertTrue(any("sourceEvidence" in e for e in self.validate(manifest, routing, changed_sources)))
        # Incidental stage mentions and installation checks are not invocations.
        self.assertEqual([edge["to"] for edge in manifest["skills"]["tdd"]["relationships"]], ["codebase-design"])
        self.assertEqual(manifest["skills"]["setup-matt-pocock-skills"]["relationships"], [])
        self.assertNotIn("triage", manifest["skills"])
        changed = copy.deepcopy(self.manifest)
        changed["skills"]["onecrew-alpha"]["relationships"] = [{
            "to": "onecrew-beta", "kind": "pair", "when": "Needed.", "sourceEvidence": "onecrew-beta",
        }]
        self.assertTrue(any("only supported for approved imports" in e for e in self.validate(changed)))

    def test_upstream_workflows_remain_optional_and_conditional(self) -> None:
        root = MODULE_PATH.parents[2]
        manifest = json.loads((root / "development/lint/project-skill-ownership.json").read_text())
        routing = json.loads((root / "development/lint/project-skill-routing-cases.json").read_text())
        cases = {case["id"]: case for case in routing["cases"]}
        skills = manifest["skills"]
        for case_id in ("ordinary-product-implementation", "resume-authorized-provider-implementation"):
            self.assertTrue(set(AGENT_CONTEXT.UPSTREAM_SKILL_PATHS).issubset(cases[case_id]["reject"]))
            self.assertEqual(cases[case_id]["owner"], "onecrew-capability")
            self.assertEqual(cases[case_id]["crossCutting"], [])
        for name in ("to-spec", "to-tickets", "code-review"):
            self.assertEqual([(e["to"], e["kind"]) for e in skills[name]["relationships"]], [("setup-matt-pocock-skills", "handoff")])
        self.assertEqual(cases["test-first-unresolved-interface"]["crossCutting"], ["codebase-design"])
        self.assertIn("tdd", cases["explicit-implement-without-test-first-seam"]["reject"])
        self.assertIn("without an explicit compatibility requirement", cases["ticket-wide-refactor-without-compatibility"]["request"])
        self.assertIn("finite review", skills["code-review"]["what"])
        for name in ("onecrew-review", "onecrew-verify", "onecrew-creator", "onecrew-rust-architecture"):
            self.assertIn(name, skills)

    def test_upstream_implicit_policy_can_be_omitted_but_not_malformed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            policy = Path(directory) / "openai.yaml"
            policy.write_text('interface:\n  display_name: "Upstream skill"\n')
            errors: list[str] = []
            self.assertTrue(AGENT_CONTEXT.read_invocation_policy(
                policy, "openai.yaml", errors, allow_omitted_policy=True
            ))
            self.assertEqual(errors, [])
            AGENT_CONTEXT.read_invocation_policy(policy, "openai.yaml", errors)
            self.assertTrue(any("must be true or false" in e for e in errors))
            policy.write_text("policy:\n  allow_implicit_invocation: sometimes\n")
            errors = []
            AGENT_CONTEXT.read_invocation_policy(
                policy, "openai.yaml", errors, allow_omitted_policy=True
            )
            self.assertTrue(any("must be true or false" in e for e in errors))
            policy.write_text("policy:\n  allow_implicit_invocation: false\n")
            errors = []
            self.assertFalse(AGENT_CONTEXT.read_invocation_policy(
                policy, "openai.yaml", errors, allow_omitted_policy=True
            ))
            self.assertEqual(errors, [])

    def test_root_template_examples_do_not_disable_real_link_checks(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            template = root / "CONTEXT-FORMAT.md"
            example = "```md\n[Ordering](./src/ordering/CONTEXT.md)\n```\n"
            template.write_text(example)
            with patch.object(AGENT_CONTEXT, "ROOT", root):
                errors: list[str] = []
                AGENT_CONTEXT.validate_markdown_links(
                    root, errors, root_templates={"CONTEXT-FORMAT.md"}
                )
                self.assertEqual(errors, [])
                AGENT_CONTEXT.validate_markdown_links(root, errors)
                self.assertTrue(any("missing relative link" in e for e in errors))
                template.write_text(example + "\n[Real link](missing.md)\n")
                errors = []
                AGENT_CONTEXT.validate_markdown_links(
                    root, errors, root_templates={"CONTEXT-FORMAT.md"}
                )
                self.assertEqual(len(errors), 1)
                self.assertIn("missing.md", errors[0])


if __name__ == "__main__":
    unittest.main()
