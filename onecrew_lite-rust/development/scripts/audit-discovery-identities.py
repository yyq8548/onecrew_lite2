#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Read-only audit of qualified account identity overlap in one Campaign.

This reports leads for human verification. Native account uniqueness and the
absence of leads do not prove that every account belongs to a different person.
"""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
from collections import defaultdict
from pathlib import Path
from urllib.parse import unquote, urlsplit


def normalized_profile_url(value: str) -> str | None:
    try:
        parsed = urlsplit(value)
    except ValueError:
        return None
    if (parsed.scheme not in ("http", "https") or not parsed.hostname
            or parsed.username is not None or parsed.password is not None):
        return None
    host = parsed.hostname.lower().removeprefix("www.").removeprefix("m.")
    if host in ("twitter.com", "x.com"):
        host = "x.com"
    path = unquote(parsed.path).rstrip("/")
    if host in ("x.com", "instagram.com", "tiktok.com"):
        path = path.casefold()
    return f"{host}{path}"


def linked_identity_url(value: str) -> str | None:
    """Recognize public profile locators, never products, posts or short links.

    A shared locator is only a review signal: descriptions can link sponsors,
    colleagues and guests. It is not proof of common ownership.
    """
    key = normalized_profile_url(value)
    if not key:
        return None
    host, _, path = key.partition("/")
    parts = path.split("/")
    if host in ("x.com", "instagram.com", "tiktok.com", "twitch.tv"):
        if len(parts) != 1:
            return None
        handle = parts[0]
        if host == "tiktok.com":
            if not handle.startswith("@"):
                return None
            handle = handle[1:]
        reserved = {"home", "explore", "search", "login", "signup", "settings",
                    "about", "privacy", "terms", "reels", "reel", "p", "i"}
        if handle.casefold() in reserved or not re.fullmatch(r"[A-Za-z0-9_.]{1,50}", handle):
            return None
        return f"{host}/{parts[0].casefold()}"
    if host == "youtube.com":
        if len(parts) == 1 and re.fullmatch(r"@[A-Za-z0-9_.-]{1,100}", parts[0]):
            return f"{host}/{parts[0].casefold()}"
        if len(parts) == 2 and parts[0] == "channel" and re.fullmatch(r"UC[A-Za-z0-9_-]{22}", parts[1]):
            return key
    if host in ("linktr.ee", "beacons.ai") and len(parts) == 1 and re.fullmatch(r"[A-Za-z0-9_.-]{1,100}", parts[0]):
        return key
    return None


def visible_handle(value: str) -> str | None:
    try:
        parsed = urlsplit(value)
    except ValueError:
        return None
    host = (parsed.hostname or "").lower().removeprefix("www.")
    segments = [segment for segment in parsed.path.split("/") if segment]
    if host in ("x.com", "twitter.com", "instagram.com", "tiktok.com") and len(segments) == 1:
        return segments[0].lstrip("@").casefold()
    if host == "youtube.com" and len(segments) == 1 and segments[0].startswith("@"):
        return segments[0][1:].casefold()
    return None


def audit(rows: list[dict]) -> dict:
    profiles: dict[str, list[int]] = defaultdict(list)
    names: dict[str, list[int]] = defaultdict(list)
    handles: dict[str, list[int]] = defaultdict(list)
    native_keys: dict[tuple[str, str], list[int]] = defaultdict(list)
    invalid_profile_ids: list[str] = []
    accounts_with_public_links = 0
    public_links_examined = 0
    linked_profiles: dict[str, set[int]] = defaultdict(set)
    accounts_without_identity_links: list[str] = []
    for index, row in enumerate(rows):
        key = normalized_profile_url(row["profile_url"])
        if key:
            profiles[key].append(index)
        else:
            invalid_profile_ids.append(row["id"])
        names[row["display_name"].strip().casefold()].append(index)
        handle = visible_handle(row["profile_url"])
        if handle:
            handles[handle].append(index)
        native_keys[(row["platform"], row["platform_account_id"])].append(index)
        if row["public_links"]:
            accounts_with_public_links += 1
            public_links_examined += len(row["public_links"])
        identity_links = {key for link in row["public_links"]
                          if (key := linked_identity_url(link)) is not None}
        for key in identity_links:
            linked_profiles[key].add(index)
        if not identity_links:
            accounts_without_identity_links.append(row["id"])

    leads: dict[tuple[int, int], set[str]] = defaultdict(set)
    for group, reason in ((profiles, "same_profile_url"), (names, "same_display_name"),
                          (handles, "same_visible_handle"), (native_keys, "same_native_account")):
        for members in group.values():
            for left_index in range(len(members)):
                for right_index in range(left_index + 1, len(members)):
                    pair = tuple(sorted((members[left_index], members[right_index])))
                    leads[pair].add(reason)

    shared_links: dict[tuple[int, int], list[str]] = defaultdict(list)
    for key, member_set in sorted(linked_profiles.items()):
        members = sorted(member_set)
        for left_index, left in enumerate(members):
            for right in members[left_index + 1:]:
                pair = (left, right)
                leads[pair].add("shared_linked_profile_requires_review")
                shared_links[pair].append(key)

    for index, row in enumerate(rows):
        for link in row["public_links"]:
            key = normalized_profile_url(link)
            if not key:
                continue
            for other in profiles.get(key, []):
                if other != index:
                    leads[tuple(sorted((index, other)))].add("public_link_to_other_profile")

    pairs = []
    for (left, right), reasons in sorted(leads.items()):
        pairs.append({
            "left": {field: rows[left][field] for field in ("id", "platform", "display_name", "profile_url")},
            "right": {field: rows[right][field] for field in ("id", "platform", "display_name", "profile_url")},
            "signals": sorted(reasons),
            "shared_profile_locators": shared_links.get((left, right), []),
        })
    return {
        "qualified_native_accounts": len(rows),
        "distinct_native_keys": len(native_keys),
        "valid_profile_urls": len(rows) - len(invalid_profile_ids),
        "invalid_profile_ids": invalid_profile_ids,
        "accounts_with_saved_public_links": accounts_with_public_links,
        "saved_public_links_examined": public_links_examined,
        "accounts_without_saved_identity_links": accounts_without_identity_links,
        "possible_duplicate_pairs": pairs,
        "natural_person_uniqueness_proven": False,
        "limitation": "Exact account keys and saved links cannot prove distinct real-world creators; review any pair against native identity evidence.",
    }


def read_qualified(database: Path, campaign_id: str) -> list[dict]:
    uri = database.resolve().as_uri() + "?mode=ro"
    with sqlite3.connect(uri, uri=True) as connection:
        cursor = connection.execute(
            "SELECT id,platform,platform_account_id,display_name,profile_url,evidence_json "
            "FROM discovery_candidates WHERE discovery_campaign_id=? AND verdict='qualified' "
            "ORDER BY platform,platform_account_id",
            (campaign_id,),
        )
        rows = []
        for candidate_id, platform, account_id, name, profile_url, evidence_json in cursor:
            evidence = json.loads(evidence_json)
            links = evidence.get("public_links", [])
            rows.append({
                "id": candidate_id,
                "platform": platform,
                "platform_account_id": account_id,
                "display_name": name,
                "profile_url": profile_url,
                "public_links": [link for link in links if isinstance(link, str)] if isinstance(links, list) else [],
            })
        return rows


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("database", type=Path)
    parser.add_argument("campaign_id")
    args = parser.parse_args()
    report = {"campaign_id": args.campaign_id, **audit(read_qualified(args.database, args.campaign_id))}
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
