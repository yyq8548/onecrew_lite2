from __future__ import annotations

import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).parents[2]
PLANS_ROOT = REPO_ROOT / "docs/plans"


class PhasedDeliveryContractTests(unittest.TestCase):
    def test_active_trackers_freeze_completed_phases_with_commits(self) -> None:
        for readme_path in sorted(PLANS_ROOT.glob("*/README.md")):
            readme = readme_path.read_text()
            lifecycle = next(
                (
                    line.removeprefix("Status: ").strip()
                    for line in readme.splitlines()
                    if line.startswith("Status: ")
                ),
                None,
            )
            if lifecycle != "active":
                continue

            tracker_path = readme_path.with_name("implementation-tracker.md")
            self.assertTrue(
                tracker_path.is_file(),
                f"active plan is missing its tracker: {tracker_path.relative_to(REPO_ROOT)}",
            )
            tracker = tracker_path.read_text()
            sections = tracker.split("\n## Phase ")[1:]
            self.assertTrue(
                sections,
                f"active plan tracker has no phases: {tracker_path.relative_to(REPO_ROOT)}",
            )
            for section in sections:
                if "- Status: frozen/completed." not in section:
                    continue
                phase = section.splitlines()[0]
                self.assertIn(
                    "- Commit: `",
                    section,
                    f"completed phase lacks a commit: {readme_path.parent.name}/{phase}",
                )


if __name__ == "__main__":
    unittest.main()
