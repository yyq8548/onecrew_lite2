#!/usr/bin/env -S uv run --script

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
CONFIG_PATH = Path(__file__).with_name("agent-context.config.json")
PROJECT_SKILL_OWNERSHIP_PATH = Path(__file__).with_name(
    "project-skill-ownership.json"
)
PROJECT_SKILL_ROUTING_CASES_PATH = Path(__file__).with_name(
    "project-skill-routing-cases.json"
)
ALLOWED_CONFIG_KEYS = {
    "budgets",
    "developmentSkillsDirectory",
    "projectInstructionFiles",
    "portablePythonFiles",
    "schemaVersion",
    "skillGraph",
    "skillsDirectory",
}
ALLOWED_FRONTMATTER_KEYS = {
    "allowed-tools",
    "argument-hint",
    "description",
    "disable-model-invocation",
    "license",
    "metadata",
    "name",
}
ALLOWED_SKILL_ENTRIES = {
    "SKILL.md",
    "agents",
    "assets",
    "evals",
    "references",
    "scripts",
    "templates",
}
REQUIRED_BUDGET_KEYS = {
    "maxDiscoverableSkills",
    "maxExplicitDescriptionCharactersPerSkill",
    "maxImplicitDescriptionCharacters",
    "maxImplicitDescriptionCharactersPerSkill",
    "maxImplicitSkills",
    "maxProjectInstructionBytes",
    "maxSkillBodyLines",
    "maxTotalDescriptionCharacters",
}
REQUIRED_GRAPH_CONFIG_KEYS = {"maxPositiveCaseOverlapRatio"}
REQUIRED_GRAPH_ENTRY_KEYS = {
    "effects",
    "negativeCases",
    "ownerConcern",
    "pathScopes",
    "positiveCases",
    "references",
    "relationships",
    "requiredReferences",
    "stopCondition",
    "verificationOwner",
    "what",
}
EFFECT_KEYS = {"externalEffect", "mutation", "package", "review"}
PATH_SCOPE_KEYS = {"appliesTo", "governs", "observes", "owns"}
RELATIONSHIP_KINDS = {"handoff", "pair", "review", "verify"}
SKILL_NAME_PREFIX = "onecrew-"
# Preserve these upstream names and layouts only with their pinned provenance.
UPSTREAM_REPOSITORY = "https://github.com/mattpocock/skills"
UPSTREAM_SKILL_PATHS = {
    "grill-with-docs": "skills/engineering/grill-with-docs",
    "grilling": "skills/productivity/grilling",
    "domain-modeling": "skills/engineering/domain-modeling",
    "to-spec": "skills/engineering/to-spec",
    "to-tickets": "skills/engineering/to-tickets",
    "implement": "skills/engineering/implement",
    "code-review": "skills/engineering/code-review",
    "tdd": "skills/engineering/tdd",
    "codebase-design": "skills/engineering/codebase-design",
    "setup-matt-pocock-skills": "skills/engineering/setup-matt-pocock-skills",
}
SKILL_LINKS = (".agents/skills", ".claude/skills")
PYTHON_COMMAND_PATTERN = re.compile(
    r"\bpython(?:3(?:\.\d+)?)?\s+(?:-[A-Za-z]|[A-Za-z0-9_./])"
)
PYTHON_SCAN_EXCLUDED_PARTS = {
    ".git",
    "dist",
    "libs",
    "logs",
    "node_modules",
    "target",
    "vendor",
}


def parse_scalar(value: str) -> Any:
    value = value.strip()
    if value == "true":
        return True
    if value == "false":
        return False
    if value.startswith('"') and value.endswith('"'):
        try:
            return json.loads(value)
        except json.JSONDecodeError:
            return value[1:-1]
    if value.startswith("'") and value.endswith("'"):
        return value[1:-1]
    return value


def parse_frontmatter(source: str, display_path: str, errors: list[str]) -> dict[str, Any]:
    match = re.match(r"^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)", source)
    if match is None:
        errors.append(f"{display_path}: missing or malformed YAML frontmatter")
        return {}
    if not source[match.end():].strip():
        errors.append(f"{display_path}: skill body is empty")

    fields: dict[str, Any] = {}
    lines = match.group(1).splitlines()
    index = 0
    while index < len(lines):
        line = lines[index]
        key_match = re.match(r"^([a-zA-Z0-9_-]+):(?:\s*(.*))?$", line)
        if key_match is None:
            index += 1
            continue
        key, raw_value = key_match.group(1), key_match.group(2) or ""
        if raw_value in {">", ">-", "|", "|-", ">+", "|+"}:
            block_lines: list[str] = []
            index += 1
            while index < len(lines) and (not lines[index] or lines[index][0].isspace()):
                block_lines.append(lines[index].strip())
                index += 1
            separator = " " if raw_value.startswith(">") else "\n"
            fields[key] = separator.join(block_lines).strip()
            continue
        fields[key] = parse_scalar(raw_value)
        index += 1

    for key in fields:
        if key not in ALLOWED_FRONTMATTER_KEYS:
            errors.append(f"{display_path}: unsupported frontmatter key: {key}")
    return fields


def read_invocation_policy(
    policy_path: Path, display_path: str, errors: list[str],
    *, allow_omitted_policy: bool = False,
) -> bool:
    if not policy_path.exists():
        return True
    source = policy_path.read_text(encoding="utf-8")
    match = re.search(r"allow_implicit_invocation:\s*(true|false)\b", source)
    if match is None:
        if allow_omitted_policy and "allow_implicit_invocation" not in source:
            return True
        errors.append(
            f"{display_path}: allow_implicit_invocation must be true or false"
        )
        return True
    return match.group(1) == "true"


def load_project_skill_ownership(errors: list[str]) -> dict[str, Any]:
    if not PROJECT_SKILL_OWNERSHIP_PATH.is_file():
        errors.append("missing development/lint/project-skill-ownership.json")
        return {}
    manifest = json.loads(PROJECT_SKILL_OWNERSHIP_PATH.read_text(encoding="utf-8"))
    if manifest.get("schemaVersion") != 2:
        errors.append("Project skill ownership manifest must use schemaVersion 2")
        return {}
    for section in ("skills", "developmentSkills"):
        if not isinstance(manifest.get(section), dict):
            errors.append(
                f"Project skill ownership manifest must contain a {section} mapping"
            )
            return {}
    return manifest


def load_project_skill_routing_cases(errors: list[str]) -> dict[str, Any]:
    if not PROJECT_SKILL_ROUTING_CASES_PATH.is_file():
        errors.append("missing development/lint/project-skill-routing-cases.json")
        return {}
    routing = json.loads(PROJECT_SKILL_ROUTING_CASES_PATH.read_text(encoding="utf-8"))
    if routing.get("schemaVersion") != 1 or not isinstance(routing.get("cases"), list):
        errors.append("Project skill routing cases must use schemaVersion 1 with a cases list")
        return {}
    return routing


def markdown_targets(source: str) -> set[str]:
    return {
        target.split("#", 1)[0]
        for target in re.findall(r"!?\[[^\]]*\]\(([^)]+)\)", source)
        if target and not re.match(r"^(?:data:|doc:|https?:|mailto:|skill:|/)", target, re.I)
    }


def markdown_target_list(source: str) -> list[str]:
    return [
        target.split("#", 1)[0]
        for target in re.findall(r"!?\[[^\]]*\]\(([^)]+)\)", source)
        if target and not re.match(r"^(?:data:|doc:|https?:|mailto:|skill:|/)", target, re.I)
    ]


def approved_upstream_skill(skill_name: str, contract: dict[str, Any]) -> bool:
    upstream = contract.get("upstream")
    return (
        skill_name in UPSTREAM_SKILL_PATHS
        and isinstance(upstream, dict)
        and set(upstream) == {"repository", "revision", "path"}
        and upstream.get("repository") == UPSTREAM_REPOSITORY
        and upstream.get("path") == UPSTREAM_SKILL_PATHS[skill_name]
        and isinstance(upstream.get("revision"), str)
        and re.fullmatch(r"[0-9a-f]{40}", upstream["revision"]) is not None
    )


def upstream_root_references(skill_name: str, contract: dict[str, Any]) -> set[str]:
    # Approved imports retain their declared upstream layout. The ownership
    # graph remains the only reference inventory; undeclared root files fail.
    if not approved_upstream_skill(skill_name, contract):
        return set()
    references = contract.get("references", {})
    if not isinstance(references, dict):
        return set()
    return {
        name for name in references
        if Path(name).parent == Path(".") and Path(name).suffix == ".md"
        and name != "SKILL.md"
    }


def related_skill_targets(source: str) -> set[str]:
    match = re.search(
        r"^## Related Skills\s*$([\s\S]*?)(?=^## |\Z)",
        source,
        re.M,
    )
    if match is None:
        return set()
    return set(re.findall(r"\bonecrew-[a-z0-9-]+\b", match.group(1)))


def validate_reference_source(
    source: str,
    display_path: str,
    kind: str,
    errors: list[str],
) -> None:
    if not source.strip():
        errors.append(f"{display_path}: project reference is empty")
    if len(source.splitlines()) > 300:
        errors.append(f"{display_path}: project reference exceeds 300 lines; split the topic")

    # Kinds route readers; they do not prescribe a writing template. Meaningful
    # decision boundaries need semantic review, not padding or paired emojis.
    if kind not in {"pattern", "workflow", "map"}:
        errors.append(f"{display_path}: unsupported project reference kind: {kind}")


def validate_project_python_invocations(
    errors: list[str], portable_files: dict[str, str] | None = None,
) -> None:
    text_suffixes = {".md", ".sh", ".yaml", ".yml", ".toml"}
    portable_files = portable_files or {}
    for relative in portable_files:
        if not (ROOT / relative).is_file():
            errors.append(f"Portable Python declaration has no file: {relative}")
    for path in ROOT.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(ROOT)
        if PYTHON_SCAN_EXCLUDED_PARTS.intersection(relative.parts):
            continue
        if relative.as_posix() in portable_files:
            continue
        if path.suffix in text_suffixes:
            source = path.read_text(encoding="utf-8", errors="ignore")
            # uv's interpreter lookup does not execute a project Python tool.
            # Keep scanning the rest of the same line/file for bare invocations.
            source = re.sub(r"\buv\s+python\s+find\b", "uv-runtime-lookup", source)
            if PYTHON_COMMAND_PATTERN.search(source):
                errors.append(f"{relative}: invoke project Python tooling through uv run")
        if path.suffix == ".py":
            first_line = path.read_text(encoding="utf-8", errors="ignore").splitlines()[:1]
            if first_line and "python" in first_line[0] and "uv run" not in first_line[0]:
                errors.append(f"{relative}: project Python entry point must use a uv shebang")


def validate_skill_references(
    skill_directory: Path,
    reference_contract: dict[str, Any],
    skill_label: str,
    source: str,
    errors: list[str],
    *,
    root_references: set[str] | None = None,
) -> None:
    root_references = root_references or set()
    references_directory = skill_directory / "references"
    actual_references = (
        {
            str(path.relative_to(skill_directory))
            for path in references_directory.rglob("*.md")
        }
        if references_directory.exists()
        else set()
    )
    actual_references.update(
        name for name in root_references if (skill_directory / name).is_file()
    )
    expected_references = set(reference_contract)
    if actual_references != expected_references:
        missing = sorted(actual_references - expected_references)
        extra = sorted(expected_references - actual_references)
        if missing:
            errors.append(f"{skill_label}: manifest missing references: {', '.join(missing)}")
        if extra:
            errors.append(f"{skill_label}: manifest references do not exist: {', '.join(extra)}")

    validate_reference_links(
        source, expected_references, skill_label, errors,
        root_references=root_references,
    )

    for relative_reference in sorted(actual_references & expected_references):
        if (
            Path(relative_reference).parent != Path("references")
            and relative_reference not in root_references
        ):
            errors.append(f"{skill_label}/{relative_reference}: references must be direct children")
        reference_path = skill_directory / relative_reference
        spec = reference_contract[relative_reference]
        if not isinstance(spec, dict):
            errors.append(f"{skill_label}/{relative_reference}: invalid manifest entry")
            continue
        validate_reference_source(
            reference_path.read_text(encoding="utf-8"),
            str(reference_path.relative_to(ROOT)),
            str(spec.get("kind")),
            errors,
        )


def validate_reference_links(
    source: str,
    expected_references: set[str],
    skill_label: str,
    errors: list[str],
    *,
    root_references: set[str] | None = None,
) -> None:
    root_references = root_references or set()
    linked_reference_list = [
        target
        for raw_target in markdown_target_list(source)
        for target in [raw_target.removeprefix("./")]
        if target.startswith("references/") or target in root_references
    ]
    linked_references = set(linked_reference_list)
    link_counts = {
        target: linked_reference_list.count(target) for target in linked_references
    }
    if (
        linked_references != expected_references
        or any(count != 1 for count in link_counts.values())
    ):
        errors.append(f"{skill_label}: SKILL.md must link every project reference exactly once")


def validate_project_skill_contract(
    skills_directory: Path,
    project_contract: dict[str, dict[str, Any]],
    observed_invocation: dict[str, str],
    errors: list[str],
) -> None:
    project_directories = {
        path.name
        for path in skills_directory.iterdir()
        if path.is_dir() and not path.name.startswith(".")
    }
    contract_names = set(project_contract)
    if project_directories != contract_names:
        missing = sorted(project_directories - contract_names)
        extra = sorted(contract_names - project_directories)
        if missing:
            errors.append(f"Project skill ownership manifest missing: {', '.join(missing)}")
        if extra:
            errors.append(f"Project skill ownership manifest has unknown skills: {', '.join(extra)}")

    concern_owners: dict[str, str] = {}
    for skill_name in sorted(project_directories & contract_names):
        contract = project_contract[skill_name]
        upstream_approved = approved_upstream_skill(skill_name, contract)
        if "upstream" in contract and not upstream_approved:
            errors.append(f"{skill_name}: invalid approved upstream provenance")
        if not skill_name.startswith(SKILL_NAME_PREFIX) and not upstream_approved:
            errors.append(
                f"{skill_name}: project skills must use the {SKILL_NAME_PREFIX} prefix"
            )
        invocation = contract.get("invocation")
        if invocation not in {"implicit", "explicit"}:
            errors.append(f"{skill_name}: manifest invocation must be implicit or explicit")
        elif observed_invocation.get(skill_name) != invocation:
            errors.append(f"{skill_name}: invocation policy differs from manifest")

        concerns = contract.get("concerns")
        if not isinstance(concerns, list) or not concerns:
            errors.append(f"{skill_name}: manifest concerns must be a non-empty list")
            concerns = []
        for concern in concerns:
            if not isinstance(concern, str) or not concern:
                errors.append(f"{skill_name}: concern names must be non-empty strings")
            elif concern in concern_owners:
                errors.append(
                    f"{skill_name}: concern {concern} already owned by {concern_owners[concern]}"
                )
            else:
                concern_owners[concern] = skill_name

        skill_directory = skills_directory / skill_name
        skill_file = skill_directory / "SKILL.md"
        source = skill_file.read_text(encoding="utf-8")
        fields = parse_frontmatter(source, str(skill_file.relative_to(ROOT)), errors)
        if invocation == "explicit" and fields.get("disable-model-invocation") is not True:
            errors.append(
                f"{skill_file.relative_to(ROOT)}: explicit skill must set disable-model-invocation: true"
            )

        reference_contract = contract.get("references")
        if not isinstance(reference_contract, dict):
            errors.append(f"{skill_name}: manifest references must be a mapping")
            continue
        validate_skill_references(
            skill_directory, reference_contract, skill_name, source, errors,
            root_references=upstream_root_references(skill_name, contract),
        )
        validate_feature_packet(skill_directory, contract.get("featurePacket"), errors)


def validate_development_skill_contract(
    development_directory: Path,
    development_contract: dict[str, dict[str, Any]],
    errors: list[str],
) -> None:
    if not development_directory.is_dir():
        errors.append(
            f"{development_directory.relative_to(ROOT)}: development skills directory is missing"
        )
        return
    development_directories = {
        path.name
        for path in development_directory.iterdir()
        if path.is_dir() and not path.name.startswith(".")
    }
    contract_names = set(development_contract)
    if development_directories != contract_names:
        missing = sorted(development_directories - contract_names)
        extra = sorted(contract_names - development_directories)
        if missing:
            errors.append(f"Development skill manifest missing: {', '.join(missing)}")
        if extra:
            errors.append(f"Development skill manifest has unknown skills: {', '.join(extra)}")

    for skill_name in sorted(development_directories & contract_names):
        if not skill_name.startswith(SKILL_NAME_PREFIX):
            errors.append(
                f"{skill_name}: development skills must use the {SKILL_NAME_PREFIX} prefix"
            )
        skill_directory = development_directory / skill_name
        entries = {path.name for path in skill_directory.iterdir()}
        for entry in entries - ALLOWED_SKILL_ENTRIES:
            errors.append(
                f"{(skill_directory / entry).relative_to(ROOT)}: unsupported top-level skill entry"
            )
        skill_file = skill_directory / "SKILL.md"
        if not skill_file.exists():
            errors.append(
                f"{skill_directory.relative_to(ROOT)}: every skill directory must contain SKILL.md"
            )
            continue
        source = skill_file.read_text(encoding="utf-8")
        display_path = str(skill_file.relative_to(ROOT))
        fields = parse_frontmatter(source, display_path, errors)
        name = fields.get("name")
        if not isinstance(name, str) or name != skill_name:
            errors.append(f"{display_path}: name must match folder {skill_name}")
        description = fields.get("description")
        if not isinstance(description, str) or not description.strip():
            errors.append(f"{display_path}: description must be a non-empty string")
        validate_markdown_links(skill_directory, errors)

        reference_contract = contract_reference_mapping(
            development_contract[skill_name], skill_name, errors
        )
        if reference_contract is None:
            continue
        validate_skill_references(
            skill_directory, reference_contract, skill_name, source, errors
        )


def contract_reference_mapping(
    contract: dict[str, Any], skill_name: str, errors: list[str]
) -> dict[str, Any] | None:
    reference_contract = contract.get("references")
    if not isinstance(reference_contract, dict):
        errors.append(f"{skill_name}: manifest references must be a mapping")
        return None
    return reference_contract


def validate_feature_packet(
    skill_directory: Path,
    packet: Any,
    errors: list[str],
) -> None:
    if packet is None:
        return
    display = str(skill_directory.relative_to(ROOT))
    required_keys = {
        "caseTypes",
        "evalManifest",
        "passThreshold",
        "readiness",
        "template",
    }
    if not isinstance(packet, dict) or set(packet) != required_keys:
        errors.append(f"{display}: featurePacket must declare the complete packet contract")
        return
    for key in ("readiness", "template", "evalManifest"):
        relative = packet.get(key)
        if not isinstance(relative, str) or not (skill_directory / relative).is_file():
            errors.append(f"{display}: featurePacket {key} does not exist: {relative}")
    if not (skill_directory / "agents" / "openai.yaml").is_file():
        errors.append(f"{display}: featurePacket requires agents/openai.yaml")
    if packet.get("caseTypes") != ["golden", "negative", "verification"]:
        errors.append(f"{display}: featurePacket must cover golden, negative, verification")
    if packet.get("passThreshold") != 90:
        errors.append(f"{display}: featurePacket passThreshold must be 90")

    manifest_path = skill_directory / str(packet.get("evalManifest"))
    if not manifest_path.is_file():
        return
    manifest_source = manifest_path.read_text(encoding="utf-8")
    listed_cases = re.findall(r"^\s*-\s+(cases/[A-Za-z0-9_.-]+\.yaml)\s*$", manifest_source, re.M)
    actual_cases = sorted(
        str(path.relative_to(skill_directory / "evals"))
        for path in (skill_directory / "evals" / "cases").glob("*.yaml")
    )
    listed_relative_to_evals = sorted(
        str(Path(case_path)) for case_path in listed_cases
    )
    if listed_relative_to_evals != actual_cases:
        errors.append(f"{display}: eval manifest must list every case exactly once")
    observed_types: set[str] = set()
    for relative_case in actual_cases:
        case_source = (skill_directory / "evals" / relative_case).read_text(encoding="utf-8")
        type_match = re.search(r"^type:\s*(golden|negative|verification)\s*$", case_source, re.M)
        if type_match:
            observed_types.add(type_match.group(1))
        for required_fragment in (
            "must_include:",
            "must_not_include:",
            "critical_fail_if:",
            "pass: 90",
        ):
            if required_fragment not in case_source:
                errors.append(f"{display}/evals/{relative_case}: missing {required_fragment}")
    if observed_types != {"golden", "negative", "verification"}:
        errors.append(f"{display}: eval cases must cover golden, negative, verification")
    rubric_path = skill_directory / "evals" / "rubric.md"
    if not rubric_path.is_file():
        errors.append(f"{display}: featurePacket requires evals/rubric.md")
    else:
        rubric = rubric_path.read_text(encoding="utf-8")
        for heading in ("## Pass Criteria", "## Critical Fail Criteria"):
            if heading not in rubric:
                errors.append(f"{display}/evals/rubric.md: missing {heading}")


def _string_list(value: Any) -> list[str] | None:
    if (
        not isinstance(value, list)
        or not value
        or any(not isinstance(item, str) or not item for item in value)
        or len(set(value)) != len(value)
    ):
        return None
    return value


def _reference_full_path(
    skill_name: str,
    relative_reference: str,
    product_names: set[str],
    skills_directory: str,
    development_directory: str,
) -> str:
    root = skills_directory if skill_name in product_names else development_directory
    return f"{root}/{skill_name}/{relative_reference}"


def validate_skill_graph_data(
    manifest: dict[str, Any],
    routing: dict[str, Any],
    skill_sources: dict[str, str],
    *,
    skills_directory: str,
    development_directory: str,
    max_overlap_ratio: float,
    errors: list[str],
) -> None:
    product_contract = manifest.get("skills", {})
    development_contract = manifest.get("developmentSkills", {})
    all_contracts = {**product_contract, **development_contract}
    all_names = set(all_contracts)
    product_names = set(product_contract)
    owner_concerns: dict[str, str] = {}
    exact_path_owners: dict[str, str] = {}

    raw_cases = routing.get("cases", [])
    cases: dict[str, dict[str, Any]] = {}
    for case in raw_cases:
        if not isinstance(case, dict) or not isinstance(case.get("id"), str):
            errors.append("Project skill routing case must be an object with a string id")
            continue
        case_id = case["id"]
        if case_id in cases:
            errors.append(f"Routing case id is duplicated: {case_id}")
            continue
        cases[case_id] = case

    for skill_name, contract in all_contracts.items():
        missing_fields = REQUIRED_GRAPH_ENTRY_KEYS - set(contract)
        if missing_fields:
            errors.append(
                f"{skill_name}: graph entry missing fields: {', '.join(sorted(missing_fields))}"
            )
            continue

        owner_concern = contract.get("ownerConcern")
        if not isinstance(owner_concern, str) or not owner_concern:
            errors.append(f"{skill_name}: ownerConcern must be a non-empty string")
        elif owner_concern in owner_concerns:
            errors.append(
                f"{skill_name}: ownerConcern {owner_concern} already owned by "
                f"{owner_concerns[owner_concern]}"
            )
        else:
            owner_concerns[owner_concern] = skill_name

        expected_invocation = (
            contract.get("invocation") in {"implicit", "explicit"}
            if skill_name in product_names
            else contract.get("invocation") == "manual"
        )
        if not expected_invocation:
            errors.append(
                f"{skill_name}: product invocation must be implicit/explicit and development invocation must be manual"
            )

        if not isinstance(contract.get("what"), str) or not contract["what"].strip():
            errors.append(f"{skill_name}: what must be a non-empty string")
        if (
            not isinstance(contract.get("stopCondition"), str)
            or not contract["stopCondition"].strip()
        ):
            errors.append(f"{skill_name}: stopCondition must be a non-empty string")

        positive_cases = _string_list(contract.get("positiveCases"))
        negative_cases = _string_list(contract.get("negativeCases"))
        if positive_cases is None:
            errors.append(f"{skill_name}: positiveCases must be a unique non-empty string list")
            positive_cases = []
        if negative_cases is None:
            errors.append(f"{skill_name}: negativeCases must be a unique non-empty string list")
            negative_cases = []
        path_scopes = contract.get("pathScopes")
        if not isinstance(path_scopes, dict) or set(path_scopes) != PATH_SCOPE_KEYS:
            errors.append(
                f"{skill_name}: pathScopes must declare exactly "
                f"{', '.join(sorted(PATH_SCOPE_KEYS))}"
            )
            path_scopes = {key: [] for key in PATH_SCOPE_KEYS}
        for role in sorted(PATH_SCOPE_KEYS):
            scoped_paths = path_scopes.get(role)
            if not isinstance(scoped_paths, list) or any(
                not isinstance(scoped_path, str) or not scoped_path
                for scoped_path in scoped_paths
            ) or len(scoped_paths) != len(set(scoped_paths)):
                errors.append(f"{skill_name}: pathScopes.{role} must be a unique string list")
                continue
            if role == "owns" and not scoped_paths:
                errors.append(f"{skill_name}: pathScopes.owns must not be empty")
            for scoped_path in scoped_paths:
                path_value = Path(scoped_path)
                if path_value.is_absolute() or ".." in path_value.parts:
                    errors.append(
                        f"{skill_name}: {role} path must be repository-relative: {scoped_path}"
                    )
                    continue
                if role != "owns":
                    continue
                previous_owner = exact_path_owners.get(scoped_path)
                if previous_owner is not None and previous_owner != skill_name:
                    errors.append(
                        f"{skill_name}: owned path {scoped_path} already claimed by {previous_owner}"
                    )
                else:
                    exact_path_owners[scoped_path] = skill_name

        verification_owner = contract.get("verificationOwner")
        if verification_owner not in all_names:
            errors.append(f"{skill_name}: unknown verificationOwner {verification_owner}")

        effects = contract.get("effects")
        if not isinstance(effects, dict) or set(effects) != EFFECT_KEYS:
            errors.append(
                f"{skill_name}: effects must declare exactly {', '.join(sorted(EFFECT_KEYS))}"
            )
        elif any(not isinstance(effects[key], bool) for key in EFFECT_KEYS):
            errors.append(f"{skill_name}: every effect permission must be boolean")

        references = contract.get("references")
        if not isinstance(references, dict):
            errors.append(f"{skill_name}: references must be a mapping")
            references = {}
        required_references = contract.get("requiredReferences")
        if not isinstance(required_references, list) or any(
            not isinstance(reference, str) for reference in required_references
        ):
            errors.append(f"{skill_name}: requiredReferences must be a string list")
            required_references = []
        if not set(required_references).issubset(references):
            errors.append(f"{skill_name}: requiredReferences must be declared references")

        relationships = contract.get("relationships")
        if not isinstance(relationships, list):
            errors.append(f"{skill_name}: relationships must be a list")
            relationships = []
        relationship_ids: set[tuple[str, str]] = set()
        relationship_targets: set[str] = set()
        imported = skill_name in product_names and approved_upstream_skill(skill_name, contract)
        for relationship in relationships:
            if not isinstance(relationship, dict):
                errors.append(f"{skill_name}: relationship entries must be objects")
                continue
            target = relationship.get("to")
            kind = relationship.get("kind")
            when = relationship.get("when")
            if target == skill_name:
                errors.append(f"{skill_name}: relationship cannot target itself")
            elif target not in all_names:
                errors.append(f"{skill_name}: relationship target does not exist: {target}")
            if kind not in RELATIONSHIP_KINDS:
                errors.append(f"{skill_name}: unsupported relationship kind: {kind}")
            if not isinstance(when, str) or not when.strip():
                errors.append(f"{skill_name}: relationship must declare a non-empty when")
            identity = (str(target), str(kind))
            if identity in relationship_ids:
                errors.append(
                    f"{skill_name}: duplicate relationship {kind} -> {target}; merge its conditions"
                )
            relationship_ids.add(identity)
            if isinstance(target, str):
                relationship_targets.add(target)
            evidence = relationship.get("sourceEvidence")
            if imported:
                # Imported prose has no common Related Skills section. Review
                # the declared edges, then pin each to verbatim source evidence;
                # mentions in examples or stage descriptions create no edges.
                if (
                    not isinstance(evidence, str) or not evidence.strip()
                    or evidence not in skill_sources.get(skill_name, "")
                    or not isinstance(target, str)
                    or re.search(r"(?<![a-z0-9-])" + re.escape(target) + r"(?![a-z0-9-])", evidence) is None
                    or target not in product_names
                    or not approved_upstream_skill(target, all_contracts.get(target, {}))
                ):
                    errors.append(f"{skill_name}: imported relationship needs sourceEvidence naming its approved target")
            elif evidence is not None:
                errors.append(f"{skill_name}: sourceEvidence is only supported for approved imports")

        source_targets = related_skill_targets(skill_sources.get(skill_name, ""))
        if not imported and source_targets != relationship_targets:
            errors.append(
                f"{skill_name}: Related Skills targets must equal executable graph targets"
            )

        for case_id in positive_cases:
            case = cases.get(case_id)
            if case is None:
                errors.append(f"{skill_name}: unknown positive routing case {case_id}")
            elif case.get("owner") != skill_name:
                errors.append(f"{skill_name}: positive case {case_id} must name it as owner")
        for case_id in negative_cases:
            case = cases.get(case_id)
            if case is None:
                errors.append(f"{skill_name}: unknown negative routing case {case_id}")
            elif skill_name not in case.get("reject", []):
                errors.append(f"{skill_name}: negative case {case_id} must reject it")

    selected_references: dict[str, set[str]] = {name: set() for name in all_names}
    for case_id, case in cases.items():
        owner = case.get("owner")
        cross_cutting = case.get("crossCutting", [])
        rejected = case.get("reject", [])
        references = case.get("references", [])
        if owner not in all_names:
            errors.append(f"{case_id}: routing owner does not exist: {owner}")
            continue
        for field_name, value in (
            ("crossCutting", cross_cutting),
            ("reject", rejected),
            ("references", references),
        ):
            if not isinstance(value, list) or len(value) != len(set(value)):
                errors.append(f"{case_id}: {field_name} must be a unique list")
        if len(cross_cutting) > 2:
            errors.append(f"{case_id}: at most two optional cross-cutting skills are allowed")
        unknown_targets = (set(cross_cutting) | set(rejected)) - all_names
        if unknown_targets:
            errors.append(
                f"{case_id}: routing targets do not exist: {', '.join(sorted(unknown_targets))}"
            )
        if owner in rejected or set(cross_cutting) & set(rejected):
            errors.append(f"{case_id}: activate and reject sets must not overlap")

        owner_contract = all_contracts[owner]
        if case.get("verificationOwner") != owner_contract.get("verificationOwner"):
            errors.append(f"{case_id}: verification owner differs from owning skill")
        if case.get("stopCondition") != owner_contract.get("stopCondition"):
            errors.append(f"{case_id}: stop condition differs from owning skill")
        if not isinstance(case.get("request"), str) or not case["request"].strip():
            errors.append(f"{case_id}: request must be a non-empty string")

        declared_full_paths = {
            _reference_full_path(
                owner,
                relative_reference,
                product_names,
                skills_directory,
                development_directory,
            )
            for relative_reference in owner_contract.get("references", {})
        }
        if not set(references).issubset(declared_full_paths):
            errors.append(f"{case_id}: references must belong to and be declared by its owner")
        required_full_paths = {
            _reference_full_path(
                owner,
                relative_reference,
                product_names,
                skills_directory,
                development_directory,
            )
            for relative_reference in owner_contract.get("requiredReferences", [])
        }
        if not required_full_paths.issubset(references):
            errors.append(f"{case_id}: missing owner required references")
        selected_references[owner].update(references)

    for skill_name, contract in all_contracts.items():
        declared = {
            _reference_full_path(
                skill_name,
                relative_reference,
                product_names,
                skills_directory,
                development_directory,
            )
            for relative_reference in contract.get("references", {})
        }
        if selected_references.get(skill_name, set()) != declared:
            errors.append(f"{skill_name}: every reference must be selected by a positive routing case")

    activation_cases = {
        skill_name: {
            case_id
            for case_id, case in cases.items()
            if case.get("owner") == skill_name
            or skill_name in case.get("crossCutting", [])
        }
        for skill_name in all_names
    }
    names = sorted(all_names)
    for index, first in enumerate(names):
        first_cases = activation_cases[first]
        for second in names[index + 1 :]:
            second_cases = activation_cases[second]
            denominator = min(len(first_cases), len(second_cases))
            if denominator == 0:
                continue
            shared_cases = first_cases & second_cases
            # An owner explicitly pairing a distinct cross-cutting concern is
            # composition, not ambiguous primary ownership. Undeclared overlap
            # (including two helpers co-selected by a third owner) still counts.
            unexplained_cases = {
                case_id for case_id in shared_cases
                if not any(
                    cases[case_id].get("owner") == owner
                    and any(
                        edge.get("to") == companion and edge.get("kind") == "pair"
                        for edge in all_contracts[owner].get("relationships", [])
                    )
                    for owner, companion in ((first, second), (second, first))
                )
            }
            ratio = len(unexplained_cases) / denominator
            if ratio > max_overlap_ratio:
                errors.append(
                    f"{first} and {second}: positive routing overlap {ratio:.2f} exceeds "
                    f"{max_overlap_ratio:.2f}; merge or separate the cases"
                )


def validate_markdown_links(
    skill_directory: Path, errors: list[str], *,
    root_templates: set[str] | None = None,
) -> None:
    for file_path in skill_directory.rglob("*.md"):
        source = file_path.read_text(encoding="utf-8")
        if file_path.parent == skill_directory and file_path.name in (root_templates or set()):
            # Upstream templates contain example documents, not live links.
            source = re.sub(r"^```[^\n]*\n[\s\S]*?^```[ \t]*$", "", source, flags=re.M)
        for raw_target in re.findall(r"!?\[[^\]]*\]\(([^)]+)\)", source):
            target = raw_target.split("#", 1)[0]
            if (
                not target
                or re.match(r"^(?:data:|doc:|https?:|mailto:|skill:|/)", target, re.I)
                or "<" in target
                or "*" in target
            ):
                continue
            resolved = (file_path.parent / target).resolve()
            if not resolved.exists():
                errors.append(
                    f"{file_path.relative_to(ROOT)}: missing relative link: {target}"
                )
            elif (
                root_templates
                and resolved.parent == skill_directory.resolve()
                and resolved.suffix == ".md"
                and resolved.name not in root_templates | {"SKILL.md"}
            ):
                errors.append(
                    f"{file_path.relative_to(ROOT)}: undeclared root reference: {target}"
                )
            elif (
                file_path.parent.name == "references"
                and resolved.parent == file_path.parent.resolve()
                and resolved.suffix == ".md"
            ):
                errors.append(
                    f"{file_path.relative_to(ROOT)}: references must not route to sibling references: {target}"
                )


def validate_skill_mirrors(
    root: Path,
    skills_directory: Path,
    errors: list[str],
) -> None:
    expected_target = skills_directory.resolve()
    for relative_link in SKILL_LINKS:
        link = root / relative_link
        if (
            not link.is_symlink()
            or link.readlink().is_absolute()
            or link.resolve() != expected_target
        ):
            errors.append(f"{relative_link}: expected relative symlink to .skillshare/skills")


def add_budget_error(
    errors: list[str],
    label: str,
    actual: int,
    maximum: int,
) -> None:
    if actual > maximum:
        errors.append(f"{label}: {actual} exceeds budget {maximum}")


def validate_config(config: dict[str, Any], errors: list[str]) -> bool:
    for key in config:
        if key not in ALLOWED_CONFIG_KEYS:
            errors.append(f"Agent context config: unsupported field: {key}")
    if config.get("schemaVersion") != 1:
        errors.append(
            f"Unsupported agent-context config schema: {config.get('schemaVersion')}"
        )
    portable_files = config.get("portablePythonFiles", {})
    if not isinstance(portable_files, dict):
        errors.append("Agent context config: portablePythonFiles must map exact files to reasons")
    else:
        for path, reason in portable_files.items():
            if (
                not isinstance(path, str) or not path
                or Path(path).is_absolute() or ".." in Path(path).parts
                or any(char in path for char in "*?[]")
                or not isinstance(reason, str) or not reason.strip()
            ):
                errors.append("Agent context config: portable Python entries need exact relative files and a reason")
    budgets = config.get("budgets")
    if not isinstance(budgets, dict):
        errors.append("Agent context config: budgets must be an object")
        return False
    for key in REQUIRED_BUDGET_KEYS:
        value = budgets.get(key)
        if not isinstance(value, (int, float)) or value < 0:
            errors.append(f"Agent context config: invalid or missing budget: {key}")
    for key in budgets:
        if key not in REQUIRED_BUDGET_KEYS:
            errors.append(f"Agent context config: unsupported budget: {key}")
    skill_graph = config.get("skillGraph")
    if not isinstance(skill_graph, dict):
        errors.append("Agent context config: skillGraph must be an object")
    else:
        for key in REQUIRED_GRAPH_CONFIG_KEYS:
            value = skill_graph.get(key)
            if not isinstance(value, (int, float)) or not 0 <= value <= 1:
                errors.append(f"Agent context config: invalid graph setting: {key}")
        for key in skill_graph:
            if key not in REQUIRED_GRAPH_CONFIG_KEYS:
                errors.append(f"Agent context config: unsupported graph setting: {key}")
    return not errors


def audit() -> tuple[list[str], dict[str, int]]:
    errors: list[str] = []
    config = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
    if not validate_config(config, errors):
        return errors, {}

    budgets = config["budgets"]
    skills_directory = ROOT / config["skillsDirectory"]
    development_directory = ROOT / config["developmentSkillsDirectory"]
    manifest = load_project_skill_ownership(errors)
    routing = load_project_skill_routing_cases(errors)
    project_contract = manifest.get("skills", {}) if manifest else {}
    development_contract = manifest.get("developmentSkills", {}) if manifest else {}
    observed_invocation: dict[str, str] = {}
    stats = {
        "discoverableSkills": 0,
        "implicitSkills": 0,
        "explicitSkills": 0,
        "implicitDescriptionCharacters": 0,
        "totalDescriptionCharacters": 0,
        "developmentSkills": 0,
        "projectInstructionBytes": 0,
        "routingCases": 0,
        "graphRelationships": 0,
    }

    for skill_directory in sorted(
        path for path in skills_directory.iterdir() if path.is_dir() and not path.name.startswith(".")
    ):
        entries = {path.name for path in skill_directory.iterdir()}
        allowed_entries = ALLOWED_SKILL_ENTRIES | upstream_root_references(
            skill_directory.name, project_contract.get(skill_directory.name, {})
        )
        for entry in entries - allowed_entries:
            errors.append(
                f"{(skill_directory / entry).relative_to(ROOT)}: unsupported top-level skill entry"
            )
        skill_file = skill_directory / "SKILL.md"
        if not skill_file.exists():
            errors.append(
                f"{skill_directory.relative_to(ROOT)}: every skill directory must contain SKILL.md"
            )
            continue

        source = skill_file.read_text(encoding="utf-8")
        display_path = str(skill_file.relative_to(ROOT))
        fields = parse_frontmatter(source, display_path, errors)
        name = fields.get("name")
        description = fields.get("description")
        disable_model_invocation = fields.get("disable-model-invocation")
        stats["discoverableSkills"] += 1

        if not isinstance(name, str) or re.fullmatch(r"[a-z0-9-]{1,64}", name) is None:
            errors.append(f"{display_path}: name must be 1-64 hyphen-case characters")
        elif name != skill_directory.name:
            errors.append(
                f"{display_path}: name {name} must match folder {skill_directory.name}"
            )
        if not isinstance(description, str) or not description.strip():
            errors.append(f"{display_path}: description must be a non-empty string")
            description = ""
        if disable_model_invocation not in (None, True, False):
            errors.append(
                f"{display_path}: disable-model-invocation must be true or false"
            )

        implicit = read_invocation_policy(
            skill_directory / "agents" / "openai.yaml",
            f"{skill_directory.relative_to(ROOT)}/agents/openai.yaml",
            errors,
            allow_omitted_policy=(
                approved_upstream_skill(
                    skill_directory.name, project_contract.get(skill_directory.name, {})
                )
                and project_contract[skill_directory.name].get("invocation") == "implicit"
            ),
        )
        observed_invocation[skill_directory.name] = "implicit" if implicit else "explicit"
        description_length = len(description)
        stats["totalDescriptionCharacters"] += description_length
        if implicit:
            stats["implicitSkills"] += 1
            stats["implicitDescriptionCharacters"] += description_length
            add_budget_error(
                errors,
                f"{display_path} implicit description characters",
                description_length,
                budgets["maxImplicitDescriptionCharactersPerSkill"],
            )
        else:
            stats["explicitSkills"] += 1
            add_budget_error(
                errors,
                f"{display_path} explicit description characters",
                description_length,
                budgets["maxExplicitDescriptionCharactersPerSkill"],
            )
        if disable_model_invocation is True and implicit:
            errors.append(
                f"{display_path}: disable-model-invocation requires agents/openai.yaml policy.allow_implicit_invocation: false"
            )

        add_budget_error(
            errors,
            f"{display_path} body lines",
            len(source.splitlines()),
            budgets["maxSkillBodyLines"],
        )
        validate_markdown_links(
            skill_directory, errors,
            root_templates=upstream_root_references(
                skill_directory.name, project_contract.get(skill_directory.name, {})
            ),
        )

    validate_project_skill_contract(
        skills_directory,
        project_contract,
        observed_invocation,
        errors,
    )
    stats["developmentSkills"] = len(development_contract)
    validate_development_skill_contract(
        development_directory,
        development_contract,
        errors,
    )
    skill_sources: dict[str, str] = {}
    for directory in (skills_directory, development_directory):
        for skill_directory in directory.iterdir():
            skill_file = skill_directory / "SKILL.md"
            if skill_directory.is_dir() and skill_file.is_file():
                skill_sources[skill_directory.name] = skill_file.read_text(encoding="utf-8")
    if manifest and routing:
        validate_skill_graph_data(
            manifest,
            routing,
            skill_sources,
            skills_directory=config["skillsDirectory"],
            development_directory=config["developmentSkillsDirectory"],
            max_overlap_ratio=config["skillGraph"]["maxPositiveCaseOverlapRatio"],
            errors=errors,
        )
        stats["routingCases"] = len(routing.get("cases", []))
        stats["graphRelationships"] = sum(
            len(entry.get("relationships", []))
            for section in ("skills", "developmentSkills")
            for entry in manifest.get(section, {}).values()
        )
    validate_project_python_invocations(errors, config.get("portablePythonFiles", {}))

    for instruction_file in config["projectInstructionFiles"]:
        path = ROOT / instruction_file
        if path.exists():
            size = path.stat().st_size
            stats["projectInstructionBytes"] = max(
                stats["projectInstructionBytes"], size
            )
            add_budget_error(
                errors,
                f"{instruction_file} bytes",
                size,
                budgets["maxProjectInstructionBytes"],
            )

    add_budget_error(
        errors,
        "Discoverable skills",
        stats["discoverableSkills"],
        budgets["maxDiscoverableSkills"],
    )
    add_budget_error(
        errors,
        "Implicit skills",
        stats["implicitSkills"],
        budgets["maxImplicitSkills"],
    )
    add_budget_error(
        errors,
        "Total description characters",
        stats["totalDescriptionCharacters"],
        budgets["maxTotalDescriptionCharacters"],
    )
    add_budget_error(
        errors,
        "Implicit description characters",
        stats["implicitDescriptionCharacters"],
        budgets["maxImplicitDescriptionCharacters"],
    )

    validate_skill_mirrors(ROOT, skills_directory, errors)
    agents_file = ROOT / "AGENTS.md"
    if not agents_file.is_symlink() or agents_file.readlink() != Path("CLAUDE.md"):
        errors.append("AGENTS.md: expected relative symlink to CLAUDE.md")

    return errors, stats


def main() -> int:
    config = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
    errors, stats = audit()
    if stats:
        budgets = config["budgets"]
        print(
            f"[agent-context] skills: {stats['discoverableSkills']}/{budgets['maxDiscoverableSkills']} discoverable, "
            f"{stats['implicitSkills']}/{budgets['maxImplicitSkills']} implicit, {stats['explicitSkills']} explicit, "
            f"{stats['developmentSkills']} development"
        )
        print(
            f"[agent-context] descriptions: {stats['implicitDescriptionCharacters']}/{budgets['maxImplicitDescriptionCharacters']} implicit chars, "
            f"{stats['totalDescriptionCharacters']}/{budgets['maxTotalDescriptionCharacters']} total chars"
        )
        print(
            f"[agent-context] largest project instruction: {stats['projectInstructionBytes']}/{budgets['maxProjectInstructionBytes']} bytes"
        )
        print(
            f"[agent-context] graph: {stats['routingCases']} routing cases, "
            f"{stats['graphRelationships']} typed relationships"
        )
    if errors:
        print(
            "[agent-context] failed with "
            f"{len(errors)} violation(s):\n- " + "\n- ".join(errors),
            file=sys.stderr,
        )
        return 1
    print("[agent-context] passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
