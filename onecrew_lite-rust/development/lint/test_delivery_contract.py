from __future__ import annotations

import unittest
from pathlib import Path

import delivery_contract as contract


class DeliveryContractTests(unittest.TestCase):
    def test_review_identity_is_order_independent_but_diff_sensitive(self) -> None:
        first = contract.review_identity(
            base_commit="abc",
            reviewed_paths=["b", "a", "a"],
            diff_fingerprint="diff-1",
            risk_boundaries=["security", "owner"],
        )
        reordered = contract.review_identity(
            base_commit="abc",
            reviewed_paths=["a", "b"],
            diff_fingerprint="diff-1",
            risk_boundaries=["owner", "security"],
        )
        changed = contract.review_identity(
            base_commit="abc",
            reviewed_paths=["a", "b"],
            diff_fingerprint="diff-2",
            risk_boundaries=["owner", "security"],
        )

        self.assertEqual(first, reordered)
        self.assertNotEqual(first, changed)

    def test_review_runs_once_and_reuses_the_same_identity(self) -> None:
        self.assertEqual(
            contract.plan_review(
                delivery_status="active",
                trigger="cross_boundary_handoff",
                identity_already_reviewed=False,
                initial_review_count=0,
                rereview_count=0,
            ),
            "RUN_INITIAL",
        )
        self.assertEqual(
            contract.plan_review(
                delivery_status="active",
                trigger="agent_returned",
                identity_already_reviewed=True,
                initial_review_count=1,
                rereview_count=0,
            ),
            "REUSE",
        )

    def test_lifecycle_events_do_not_start_review(self) -> None:
        for trigger in (
            "subtask_completed",
            "agent_returned",
            "asset_regenerated",
            "version_synchronized",
            "wording_only",
        ):
            with self.subTest(trigger=trigger):
                self.assertEqual(
                    contract.plan_review(
                        delivery_status="active",
                        trigger=trigger,
                        identity_already_reviewed=False,
                        initial_review_count=0,
                        rereview_count=0,
                    ),
                    "SKIP",
                )

    def test_medium_and_low_findings_close_without_rereview(self) -> None:
        for severity in ("medium", "low"):
            with self.subTest(severity=severity):
                self.assertEqual(
                    contract.plan_review(
                        delivery_status="active",
                        trigger="finding_fixed",
                        identity_already_reviewed=False,
                        initial_review_count=1,
                        rereview_count=0,
                        finding_severity=severity,
                    ),
                    "TARGETED_CLOSURE",
                )

    def test_rereview_is_limited_to_one_eligible_delta(self) -> None:
        self.assertEqual(
            contract.plan_review(
                delivery_status="active",
                trigger="finding_fixed",
                identity_already_reviewed=False,
                initial_review_count=1,
                rereview_count=0,
                finding_severity="high",
            ),
            "RUN_REREVIEW",
        )
        self.assertEqual(
            contract.plan_review(
                delivery_status="active",
                trigger="finding_fixed",
                identity_already_reviewed=False,
                initial_review_count=1,
                rereview_count=1,
                changed_boundaries=["public_contract"],
            ),
            "STOP",
        )

    def test_only_identical_pass_evidence_is_reused(self) -> None:
        identity = contract.evidence_identity(
            command=["uv", "run", "check.py"],
            cwd=".",
            relevant_diff_fingerprint="diff",
            dependency_fingerprint="deps",
        )
        self.assertEqual(
            contract.plan_evidence_reuse(
                current_identity=identity,
                prior_identity=identity,
                prior_status="PASS",
            ),
            "REUSE",
        )
        for status in ("FAIL", "BLOCKED", None):
            with self.subTest(status=status):
                self.assertEqual(
                    contract.plan_evidence_reuse(
                        current_identity=identity,
                        prior_identity=identity,
                        prior_status=status,
                    ),
                    "RUN",
                )

    def test_final_review_excludes_unchanged_frozen_phase_coverage(self) -> None:
        surface = contract.final_review_surface(
            final_paths=["phase-a", "phase-b", "integration"],
            phase_coverages=[
                {
                    "paths": ["phase-a"],
                    "frozen": True,
                    "reviewedFingerprint": "same",
                    "currentFingerprint": "same",
                },
                {
                    "paths": ["phase-b"],
                    "frozen": True,
                    "reviewedFingerprint": "old",
                    "currentFingerprint": "new",
                },
            ],
        )
        self.assertEqual(surface, ["integration", "phase-b"])

    def test_user_acceptance_is_terminal(self) -> None:
        self.assertEqual(
            contract.plan_terminal_delivery(
                delivery_status="accepted",
                acceptance_key_unchanged=True,
                new_user_request=False,
            ),
            {"stop": True, "action": "NO_ACTION"},
        )
        self.assertEqual(
            contract.plan_terminal_delivery(
                delivery_status="accepted",
                acceptance_key_unchanged=True,
                new_user_request=True,
            ),
            {"stop": True, "action": "NEW_DELIVERY"},
        )
        self.assertEqual(
            contract.plan_review(
                delivery_status="accepted",
                trigger="unreviewed_integration_handoff",
                identity_already_reviewed=False,
                initial_review_count=0,
                rereview_count=0,
            ),
            "STOP",
        )

if __name__ == "__main__":
    unittest.main()
