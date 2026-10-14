import importlib.util
import unittest
from pathlib import Path


MODULE = Path(__file__).with_name("audit-discovery-identities.py")
spec = importlib.util.spec_from_file_location("audit_discovery_identities", MODULE)
assert spec and spec.loader
audit_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit_module)


def row(candidate_id, platform, account_id, name, url, links=()):
    return {"id": candidate_id, "platform": platform, "platform_account_id": account_id,
            "display_name": name, "profile_url": url, "public_links": list(links)}


class IdentityAuditTests(unittest.TestCase):
    def test_cross_platform_link_is_a_lead_not_a_merged_person(self):
        report = audit_module.audit([
            row("a", "youtube", "UCa", "Creator A", "https://www.youtube.com/channel/UCa",
                ["https://x.com/Creator_A?ref=video"]),
            row("b", "twitter", "42", "Creator A", "https://twitter.com/creator_a/"),
        ])
        self.assertEqual(report["distinct_native_keys"], 2)
        self.assertFalse(report["natural_person_uniqueness_proven"])
        self.assertEqual(report["possible_duplicate_pairs"][0]["signals"],
                         ["public_link_to_other_profile", "same_display_name"])

    def test_distinct_accounts_remain_unproven_people_even_without_leads(self):
        report = audit_module.audit([
            row("a", "youtube", "UCa", "A", "https://www.youtube.com/channel/UCa"),
            row("b", "youtube", "UCb", "B", "https://www.youtube.com/channel/UCb"),
        ])
        self.assertEqual(report["qualified_native_accounts"], 2)
        self.assertEqual(report["valid_profile_urls"], 2)
        self.assertEqual(report["possible_duplicate_pairs"], [])
        self.assertFalse(report["natural_person_uniqueness_proven"])

    def test_invalid_profile_does_not_make_audit_look_complete(self):
        report = audit_module.audit([row("a", "youtube", "UCa", "A", "missing")])
        self.assertEqual(report["valid_profile_urls"], 0)
        self.assertEqual(report["invalid_profile_ids"], ["a"])

    def test_shared_profile_is_review_lead_not_automatic_merge(self):
        report = audit_module.audit([
            row("a", "youtube", "UCa", "A", "https://youtube.com/channel/UCa",
                ["https://instagram.com/Guest/?utm_source=a", "https://instagram.com/guest"]),
            row("b", "tiktok", "42", "B", "https://tiktok.com/@different",
                ["https://www.instagram.com/GUEST"]),
        ])
        self.assertEqual(report["distinct_native_keys"], 2)
        self.assertEqual(len(report["possible_duplicate_pairs"]), 1)
        pair = report["possible_duplicate_pairs"][0]
        self.assertEqual(pair["signals"], ["shared_linked_profile_requires_review"])
        self.assertEqual(pair["shared_profile_locators"], ["instagram.com/guest"])
        self.assertFalse(report["natural_person_uniqueness_proven"])

    def test_products_posts_homepages_and_credentials_are_not_identity_links(self):
        links = ["https://www.nike.com/shoes", "https://x.com/a/status/123",
                 "https://instagram.com/p/123", "https://instagram.com/explore",
                 "https://youtube.com/watch?v=123", "https://bit.ly/shared",
                 "https://instagram.com.evil.test/artist", "https://user:secret@instagram.com/artist",
                 "https://linktr.ee/", "https://tiktok.com/tag/shoes"]
        for link in links:
            with self.subTest(link=link):
                self.assertIsNone(audit_module.linked_identity_url(link))
        report = audit_module.audit([
            row("a", "youtube", "UCa", "A", "https://youtube.com/channel/UCa", links),
            row("b", "youtube", "UCb", "B", "https://youtube.com/channel/UCb", links),
        ])
        self.assertEqual(report["possible_duplicate_pairs"], [])
        self.assertEqual(report["accounts_without_saved_identity_links"], ["a", "b"])

    def test_same_youtube_channel_is_case_sensitive_and_handles_are_not_channels(self):
        channel = "UC" + "a" * 22
        self.assertEqual(audit_module.linked_identity_url(f"https://youtube.com/channel/{channel}"),
                         f"youtube.com/channel/{channel}")
        self.assertIsNone(audit_module.linked_identity_url("https://youtube.com/channel/not-a-channel"))
        self.assertNotEqual(audit_module.linked_identity_url(f"https://youtube.com/@{channel}"),
                            audit_module.linked_identity_url(f"https://youtube.com/channel/{channel}"))


if __name__ == "__main__":
    unittest.main()
