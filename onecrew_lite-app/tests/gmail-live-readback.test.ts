import assert from "node:assert/strict";
import { mkdtemp, readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { simpleParser } from "mailparser";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { GmailBulkRepository } from "../src/mail/gmail-bulk-repository.ts";
import { gmailMessageId, messageForGmailBulkClaim, messageForGmailBulkReceipt, verifyGmailBulkReadback } from "../src/mail/gmail-mime.ts";

const evidence = process.env.ONECREW_GMAIL_READBACK_EVIDENCE;
test("真实 Gmail 已送达邮件保留服务端 Message-ID，并恢复原发送记录", { skip: !evidence }, async () => {
  const { actual, providerId, expected } = JSON.parse(await readFile(evidence!, "utf8"));
  const parsed = await simpleParser(Buffer.from(actual.raw, "base64url"));
  assert(actual.labelIds.includes("SENT") && actual.labelIds.includes("INBOX"));
  assert.notEqual(parsed.messageId, gmailMessageId(expected.from, expected.idempotencyKey));
  const receipt = await verifyGmailBulkReadback(actual, providerId, expected);
  assert.equal(receipt.internetMessageId, parsed.messageId);
  assert.equal(receipt.subject, expected.subject);
  assert.equal(receipt.body, expected.body);
  for (const changed of [{ from: "other@example.com" }, { to: "other@example.com" }, { subject: "不同主题" }, { body: "不同正文" }]) {
    await assert.rejects(verifyGmailBulkReadback(actual, providerId, { ...expected, ...changed }));
  }
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/gmail-live-", import.meta.url)));
  const path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path);
  try {
    db.exec("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON");
    db.prepare("INSERT INTO mailbox_refs VALUES ('box','gmail','Gmail',?,NULL,'[\"send\",\"sent_readback\"]',1,NULL,1,1)").run(expected.from);
    const repo = new GmailBulkRepository(db);
    const batch = repo.create({ mailbox_ref_id: "box", subject: expected.subject, body: expected.body, recipients: [{ email: expected.to, source: "真实 Gmail 验收回执" }] });
    repo.control(batch.id, "confirm");
    const claim = repo.claim()!;
    assert(repo.reserveSendAttempt(batch.id, claim.item, messageForGmailBulkClaim(claim.batch, claim.item, expected.from)));
    repo.recordProviderAcceptance(batch.id, claim.item, providerId);
    repo.finish(batch.id, { ...claim.item, state: "accepted", provider_message_id: providerId, detail: "gmail_sent_content_mismatch" });
    const accepted = repo.get(batch.id).items[0]!;
    const frozen = messageForGmailBulkReceipt(repo.get(batch.id), accepted, expected.from);
    const verified = await verifyGmailBulkReadback(actual, providerId, frozen);
    assert.throws(() => repo.stageVerifiedReceipt(batch.id, accepted, { ...verified, providerMessageId: "bad" }));
    repo.stageVerifiedReceipt(batch.id, accepted, verified);
    assert.equal(repo.recoverVerifiedReceipts(), 1);
    assert.equal(repo.get(batch.id).items[0]?.state, "sent");
    assert.equal(db.prepare("SELECT internet_message_id FROM emails WHERE provider_message_id=?").get(providerId)?.internet_message_id, parsed.messageId);
    assert.equal(repo.recoverVerifiedReceipts(), 0);
    assert.equal(repo.claim(), null);
  } finally { db.close(); }
});
