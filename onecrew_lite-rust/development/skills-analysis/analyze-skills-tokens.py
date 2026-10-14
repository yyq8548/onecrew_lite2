#!/usr/bin/env -S uv run --script

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SKILL_ROOTS = (
    ROOT / ".skillshare" / "skills",
    ROOT / "development" / "skills",
)
MAX_SKILL_TOKENS = 5_000
MAX_SKILL_LINES = 200
REFERENCE_SPLIT_REVIEW_BYTES = 10_000


def analyze_skill(skill_dir: Path) -> dict[str, object]:
    files = [skill_dir / "SKILL.md"]
    references_dir = skill_dir / "references"
    if references_dir.exists():
        files.extend(sorted(references_dir.rglob("*.md")))

    breakdown = []
    total_characters = 0
    skill_tokens = 0
    skill_lines = 0
    reference_split_reviews: list[str] = []
    for file_path in files:
        if not file_path.exists():
            continue
        source = file_path.read_text(encoding="utf-8")
        characters = len(source)
        tokens = math.ceil(characters / 4)
        total_characters += characters
        if file_path.name == "SKILL.md":
            skill_tokens = tokens
            skill_lines = len(source.splitlines())
        elif len(source.encode("utf-8")) > REFERENCE_SPLIT_REVIEW_BYTES:
            reference_split_reviews.append(str(file_path.relative_to(skill_dir)))
        breakdown.append(
            {
                "name": str(file_path.relative_to(skill_dir)),
                "characters": characters,
                "tokens": tokens,
            }
        )
    return {
        "name": skill_dir.name,
        "root": skill_dir.parent.relative_to(ROOT).as_posix(),
        "characters": total_characters,
        "tokens": math.ceil(total_characters / 4),
        "skillTokens": skill_tokens,
        "skillLines": skill_lines,
        "referenceSplitReviews": reference_split_reviews,
        "files": breakdown,
    }


def recommendation(tokens: int) -> str:
    if tokens > 10_000:
        return "URGENT: split before proceeding"
    if tokens > 5_000:
        return "CONSIDER: split independent topics"
    if tokens > 2_000:
        return "MONITOR"
    return "OK"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sort-by-size", action="store_true")
    parser.add_argument("--detailed", action="store_true")
    parser.add_argument("--json", action="store_true")
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail when a skill exceeds 10,000 estimated tokens.",
    )
    args = parser.parse_args()

    results = [
        analyze_skill(path)
        for skills_root in SKILL_ROOTS
        for path in skills_root.iterdir()
        if path.is_dir() and not path.name.startswith(".")
    ]
    if args.sort_by_size:
        results.sort(key=lambda item: int(item["tokens"]), reverse=True)
    else:
        results.sort(key=lambda item: str(item["name"]))

    if args.json:
        print(json.dumps({"schemaVersion": 1, "skills": results}, indent=2))
    else:
        print(f"{'Skill':44} {'Entry':>7} {'Bundle':>8} {'Lines':>6}  Recommendation")
        print("-" * 102)
        for item in results:
            tokens = int(item["tokens"])
            skill_tokens = int(item["skillTokens"])
            skill_lines = int(item["skillLines"])
            print(
                f"{str(item['name']):44} {skill_tokens:7,d} {tokens:8,d} {skill_lines:6,d}  "
                f"{recommendation(tokens)}"
            )
            for reference in item["referenceSplitReviews"]:
                print(f"  REVIEW >10KB reference: {reference}")
            if args.detailed:
                for file_info in item["files"]:
                    print(
                        f"  {str(file_info['name']):42} {int(file_info['tokens']):8,d} tokens"
                    )
    if args.check:
        oversized_entries = [
            item
            for item in results
            if int(item["skillTokens"]) >= MAX_SKILL_TOKENS
            or int(item["skillLines"]) > MAX_SKILL_LINES
        ]
        if oversized_entries:
            names = ", ".join(str(item["name"]) for item in oversized_entries)
            raise SystemExit(
                "Skill entries exceed the 5,000-token or 200-line entry budget: "
                f"{names}"
            )


if __name__ == "__main__":
    main()
