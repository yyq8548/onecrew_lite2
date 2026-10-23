import { readStorageSql } from "./sql-resource.ts";
import { lstat } from "node:fs/promises";
import { resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { rebuildCreatorTable, rebuildCreatorTableChanges, validateCreatorSchemaVersion } from "./creator-schema.ts";
import { verifyPrivateFile } from "./private-backup.ts";
import { migrateCreatorWebSourceEdge } from "./creator-web-source-edges.ts";
import { migrateCreatorFacebookProfileEdge } from "./creator-facebook-profile-edge.ts";
import { migrateCreatorCandidateSourceEdge } from "./creator-candidate-edge.ts";
import { migrateCreatorReceiptPlatformEdge } from "./creator-receipt-platform-edge.ts";
import { migrateCreatorLimitEdge } from "./creator-limit-edge.ts";
import { migrateCreatorFacebookContentEdge } from "./creator-facebook-content-edge.ts";
import { migrateCreatorRetiredStorageEdge } from "./creator-retire-edge.ts";
import { migrateCreatorCampaignRetirement } from "./creator-campaign-retirement.ts";
import { migrateCreatorTagsEdge } from "./creator-tags-edge.ts";
import { migrateCreatorTikTokEdge } from "./creator-tiktok-edge.ts";
import { migrateCreatorMailboxEdge } from "./creator-mailbox-edge.ts";
import { migrateCreatorTaskOriginEdge } from "./creator-task-origin-edge.ts";
import { migrateCreatorPublicContactEdge } from "./creator-public-contact-edge.ts";
import { migrateCreatorWebProfileEdge } from "./creator-web-profile-edge.ts";
import { migrateCreatorInstagramDomEdge } from "./creator-instagram-dom-edge.ts";

export async function upgradeCreatorStagingCopy({ sourcePath, stagingPath }: { sourcePath: string; stagingPath: string }): Promise<{ fromVersion: number; toVersion: 96 }> {
  const source = resolve(sourcePath);
  const staging = resolve(stagingPath);
  if (source === staging) throw new Error("Creator upgrade requires a distinct staging copy");
  await verifyPrivateFile(staging);
  const original = await lstat(source, { bigint: true });
  const isolated = await lstat(staging, { bigint: true });
  if (!original.isFile() || original.isSymbolicLink() || !isolated.isFile() || isolated.nlink !== 1n ||
      (original.dev === isolated.dev && original.ino === isolated.ino)) throw new Error("Creator staging file must have an independent regular identity");
  const db = new DatabaseSync(staging);
  let committed = false;
  try {
    // 原 v87→88 重建关联父表，明确要求外层关闭 FK，提交前完整 foreign_key_check。
    db.exec("PRAGMA busy_timeout=5000; PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE");
    const fromVersion = Number(db.prepare("PRAGMA user_version").get()!.user_version);
    if (fromVersion < 27 || fromVersion > 96) throw new Error(`Creator upgrade from ${fromVersion} is not implemented; existing data remains unchanged`);
    validateCreatorSchemaVersion(db, fromVersion);
    for (let version = fromVersion; version < 96; version++) {
      switch (version) {
        case 27: case 28: case 29:
          db.exec(readStorageSql(`creator-${version}-to-${version + 1}.sql`));
          break;
        case 30:
          migrateCreatorTaskOriginEdge(db);
          break;
        case 31:
          migrateCreatorMailboxEdge(db);
          break;
        case 32:
          migrateCreatorTikTokEdge(db);
          break;
        case 33:
          db.exec(readStorageSql("creator-33-to-34.sql"));
          break;
        case 34:
          migrateCreatorTagsEdge(db);
          break;
        case 35:
          migrateCreatorCampaignRetirement(db);
          break;
        case 36:
          migrateCreatorRetiredStorageEdge(db);
          break;
        case 37: case 38: case 39: case 40: case 41: case 42: case 43:
          db.exec(readStorageSql(`creator-${version}-to-${version + 1}.sql`));
          break;
        case 44:
          rebuildCreatorTable(db, "discovery_profile_reads", "('bilibili','tiktok')", "('bilibili','tiktok','twitter')");
          break;
        case 45:
          rebuildCreatorTable(db, "discovery_content_reads", "platform='bilibili'", "platform IN ('bilibili','twitter')");
          break;
        case 46:
        case 47:
        case 48:
          db.exec(readStorageSql(`creator-${version}-to-${version + 1}.sql`));
          break;
        case 49:
          db.exec(readStorageSql("creator-49-to-50.sql"));
          break;
        case 50:
          db.exec(readStorageSql("creator-50-to-51.sql"));
          break;
        case 51:
          db.exec(readStorageSql("creator-51-to-52.sql"));
          break;
        case 52:
          db.exec(readStorageSql("creator-52-to-53.sql"));
          break;
        case 53:
          rebuildCreatorTableChanges(db, "discovery_evidence_reviews", [
            ["json_extract(material_json,'$.format_version') IS 1", "coalesce(json_extract(material_json,'$.format_version'),0) IN (1,2)"],
            ["REFERENCES discovery_video_identity_bindings(search_hit_id)", "REFERENCES discovery_search_hits(id)"],
          ]);
          break;
        case 54:
          db.exec(readStorageSql("creator-54-to-55.sql"));
          break;
        case 55:
          db.exec(readStorageSql("creator-55-to-56.sql"));
          break;
        case 56:
          db.exec(readStorageSql("creator-56-to-57.sql"));
          break;
        case 57:
          if (db.prepare("SELECT 1 FROM discovery_evidence_reviews WHERE NOT json_valid(findings_json) OR json_type(findings_json)!='array' OR NOT json_valid(material_json) OR json_type(material_json)!='object' LIMIT 1").get()) throw new Error("Creator review encoding is invalid");
          break;
        case 58:
          db.exec(readStorageSql("creator-58-to-59.sql"));
          break;
        case 59:
          db.exec(readStorageSql("creator-59-to-60.sql"));
          break;
        case 60:
          db.exec(readStorageSql("creator-60-to-61.sql"));
          break;
        case 61:
          migrateCreatorFacebookContentEdge(db, false, 61);
          break;
        case 62:
          migrateCreatorFacebookContentEdge(db, false, 62);
          break;
        case 63:
          migrateCreatorFacebookContentEdge(db);
          db.exec(readStorageSql("creator-63-to-64.sql"));
          break;
        case 64:
          migrateCreatorLimitEdge(db);
          db.exec(readStorageSql("creator-64-to-65.sql"));
          break;
        case 65:
          db.exec(readStorageSql("creator-65-to-66.sql"));
          break;
        case 66: case 67: case 68: case 69: case 70: case 71: case 73:
          migrateCreatorReceiptPlatformEdge(db, version);
          break;
        case 72:
          rebuildCreatorTable(db, "discovery_contact_observations", "'email','telegram')", "'email','telegram','hidden_email','social_profile')");
          break;
        case 74:
          db.exec(readStorageSql("creator-74-to-75.sql"));
          break;
        case 75:
          db.exec(readStorageSql("creator-75-to-76.sql"));
          break;
        case 76:
          migrateCreatorCandidateSourceEdge(db);
          break;
        case 77:
          migrateCreatorWebSourceEdge(db, 77);
          break;
        case 78:
          db.exec(readStorageSql("creator-78-to-79.sql"));
          break;
        case 79:
          migrateCreatorFacebookProfileEdge(db);
          break;
        case 80:
          db.exec(readStorageSql("creator-80-to-81.sql"));
          break;
        case 81:
        case 82:
          migrateCreatorWebSourceEdge(db, version);
          break;
        case 83:
          rebuildCreatorTable(db, "discovery_contact_observations",
            "'email','telegram','hidden_email','social_profile')",
            "'email','telegram','hidden_email','social_profile','whatsapp','wechat','wechat_official')");
          break;
        case 84:
          db.exec(readStorageSql("creator-84-to-85.sql"));
          break;
        case 85:
          rebuildCreatorTable(db, "mailbox_refs", "'resend','smtp','imap'", "'resend','smtp','imap','gmail'");
          break;
        case 86:
          db.exec(readStorageSql("creator-86-to-87.sql"));
          break;
        case 87:
          for (const table of ["contact_locators", "conversations"]) rebuildCreatorTable(db, table,
            "'email','x','instagram','telegram','youtube','tiktok','website','link_hub'",
            "'email','x','instagram','telegram','youtube','tiktok','website','link_hub','wechat'");
          break;
        case 88:
          rebuildCreatorTable(db, "discovery_target_increases", "new_target_count > previous_target_count", "new_target_count != previous_target_count");
          break;
        case 89:
          db.exec(readStorageSql("creator-89-to-90.sql"));
          break;
        case 90:
          migrateCreatorPublicContactEdge(db);
          break;
        case 91:
          db.exec(readStorageSql("creator-91-to-92.sql"));
          break;
        case 92:
          db.exec(readStorageSql("creator-92-to-93.sql"));
          migrateCreatorWebProfileEdge(db);
          break;
        case 93:
          db.exec(readStorageSql("creator-93-to-94.sql"));
          break;
        case 94:
          db.exec(readStorageSql("creator-94-to-95.sql"));
          break;
        case 95:
          migrateCreatorInstagramDomEdge(db);
          break;
        default: throw new Error(`Creator migration edge ${version} is not registered`);
      }
      db.exec(`PRAGMA user_version=${version + 1}`);
      validateCreatorSchemaVersion(db, version + 1);
    }
    await verifyPrivateFile(staging);
    const after = await lstat(staging, { bigint: true });
    if (after.dev !== isolated.dev || after.ino !== isolated.ino || after.nlink !== 1n) throw new Error("Creator staging file identity changed");
    db.exec("COMMIT");
    committed = true;
    return { fromVersion, toVersion: 96 };
  } finally {
    try { if (!committed && db.isTransaction) db.exec("ROLLBACK"); }
    finally { db.close(); }
  }
}
