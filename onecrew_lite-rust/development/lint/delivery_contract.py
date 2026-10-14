from __future__ import annotations

import hashlib
import json
import re
from collections.abc import Iterable, Mapping, Sequence
from typing import Any


CONTRACT_VERSION = 1
INITIAL_REVIEW_TRIGGERS = {
    "user_requested",
    "cross_boundary_handoff",
    "unreviewed_integration_handoff",
}
REREVIEW_BOUNDARIES = {
    "public_contract",
    "owner_or_dependency_direction",
    "persistence",
    "security_or_secret",
    "outside_review_scope",
}
TERMINAL_DELIVERY_STATES = {"accepted", "cancelled", "superseded"}
PLAN_LIFECYCLE_STATES = {
    "active",
    "archived in place",
    "superseded and closed",
    "deferred user acceptance",
}


def _stable_hash(payload: Mapping[str, Any]) -> str:
    canonical = json.dumps(
        payload,
        ensure_ascii=True,
        separators=(",", ":"),
        sort_keys=True,
    )
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def review_identity(
    *,
    base_commit: str,
    reviewed_paths: Iterable[str],
    diff_fingerprint: str,
    risk_boundaries: Iterable[str],
) -> str:
    return _stable_hash(
        {
            "baseCommit": base_commit,
            "reviewedPaths": sorted(set(reviewed_paths)),
            "diffFingerprint": diff_fingerprint,
            "riskBoundaries": sorted(set(risk_boundaries)),
            "contractVersion": CONTRACT_VERSION,
        }
    )


def evidence_identity(
    *,
    command: Sequence[str],
    cwd: str,
    relevant_diff_fingerprint: str,
    dependency_fingerprint: str,
) -> str:
    return _stable_hash(
        {
            "command": list(command),
            "cwd": cwd,
            "relevantDiffFingerprint": relevant_diff_fingerprint,
            "dependencyFingerprint": dependency_fingerprint,
            "contractVersion": CONTRACT_VERSION,
        }
    )


def plan_review(
    *,
    delivery_status: str,
    trigger: str,
    identity_already_reviewed: bool,
    initial_review_count: int,
    rereview_count: int,
    finding_severity: str | None = None,
    changed_boundaries: Iterable[str] = (),
) -> str:
    """Return STOP, REUSE, RUN_INITIAL, RUN_REREVIEW, TARGETED_CLOSURE, or SKIP."""

    if delivery_status in TERMINAL_DELIVERY_STATES:
        return "STOP"
    if identity_already_reviewed:
        return "REUSE"
    if initial_review_count == 0:
        return "RUN_INITIAL" if trigger in INITIAL_REVIEW_TRIGGERS else "SKIP"

    severity_allows = finding_severity in {"critical", "high"}
    boundary_allows = bool(set(changed_boundaries) & REREVIEW_BOUNDARIES)
    if severity_allows or boundary_allows:
        return "RUN_REREVIEW" if rereview_count == 0 else "STOP"
    if finding_severity in {"medium", "low"}:
        return "TARGETED_CLOSURE"
    return "SKIP"


def plan_evidence_reuse(
    *,
    current_identity: str,
    prior_identity: str | None,
    prior_status: str | None,
) -> str:
    if prior_identity == current_identity and prior_status == "PASS":
        return "REUSE"
    return "RUN"


def final_review_surface(
    *,
    final_paths: Iterable[str],
    phase_coverages: Iterable[Mapping[str, Any]],
) -> list[str]:
    remaining = set(final_paths)
    for coverage in phase_coverages:
        unchanged = (
            coverage.get("frozen") is True
            and coverage.get("reviewedFingerprint")
            == coverage.get("currentFingerprint")
        )
        if unchanged:
            remaining.difference_update(coverage.get("paths", []))
    return sorted(remaining)


def plan_terminal_delivery(
    *,
    delivery_status: str,
    acceptance_key_unchanged: bool,
    new_user_request: bool,
) -> dict[str, Any]:
    if delivery_status not in TERMINAL_DELIVERY_STATES:
        return {"stop": False, "action": "CONTINUE"}
    if new_user_request:
        return {"stop": True, "action": "NEW_DELIVERY"}
    if acceptance_key_unchanged:
        return {"stop": True, "action": "NO_ACTION"}
    return {"stop": True, "action": "NEW_DELIVERY_REQUIRED"}


def plan_lifecycle_state(readme_source: str) -> str | None:
    match = re.search(r"^Status:\s*([^\n.]+)", readme_source, re.M | re.I)
    if match is None:
        return None
    candidate = match.group(1).strip().lower()
    for state in PLAN_LIFECYCLE_STATES:
        if candidate.startswith(state):
            return state
    return None


def plan_routes_implementation(readme_source: str) -> bool:
    return plan_lifecycle_state(readme_source) == "active"
