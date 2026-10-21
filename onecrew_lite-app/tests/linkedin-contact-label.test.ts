import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { linkedinProfilePlan } from "../src/platforms/opencli/linkedin.ts";
import { extractDiscoveryContacts } from "../src/discovery/contacts.ts";

test("真实LinkedIn简介粘连Telegram标签时保留原文，分别提取邮箱和明确声明的账号", () => {
  const actual = JSON.parse(readFileSync(new URL("./fixtures/linkedin-glued-contact-20261020.json", import.meta.url), "utf8"));
  const profile = linkedinProfilePlan("thanhphank34").decode([actual]);
  const contacts = extractDiscoveryContacts(profile.bio, profile.profile_url, profile.account_id, 1792554700, "author_published");
  assert.deepEqual(contacts.filter(contact => contact.kind === "email").map(contact => contact.normalized_value), ["thanh.phan@bybit.com"]);
  assert.ok(contacts.some(contact => contact.kind === "telegram" && contact.normalized_value === "https://t.me/thanhphank34"));
  for (const contact of contacts) assert.ok(profile.bio.includes(contact.source_excerpt));
});
