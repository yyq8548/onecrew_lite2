use super::SqliteRepository;
use crate::repository::GmailBulkRepository;
use crate::{GmailBulkBatch, GmailBulkItem, MailboxRefId};
use rusqlite::{Connection, OptionalExtension, params};

fn load(db: &Connection, id: &str) -> Result<GmailBulkBatch, String> {
    let mut batch = db.query_row("SELECT id,mailbox_ref_id,subject,body,state,created_at FROM gmail_bulk_batches WHERE id=?1", [id], |r| Ok(GmailBulkBatch {
        id:r.get(0)?, mailbox_ref_id:MailboxRefId::new(r.get::<_,String>(1)?), subject:r.get(2)?, body:r.get(3)?, state:r.get(4)?, created_at:r.get(5)?,items:vec![],
    })).map_err(|e|e.to_string())?;
    let mut stmt = db.prepare("SELECT id,recipient,source,fingerprint,state,provider_message_id,detail FROM gmail_bulk_items WHERE batch_id=?1 ORDER BY rowid").map_err(|e|e.to_string())?;
    batch.items = stmt
        .query_map([id], |r| {
            Ok(GmailBulkItem {
                id: r.get(0)?,
                recipient: r.get(1)?,
                source: r.get(2)?,
                fingerprint: r.get(3)?,
                state: r.get(4)?,
                provider_message_id: r.get(5)?,
                detail: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    Ok(batch)
}

impl GmailBulkRepository for SqliteRepository {
    fn reserve_gmail_send_attempt(&self, key: &str) -> Result<bool, String> {
        let db = self.lock()?;
        db.execute(
            "INSERT OR IGNORE INTO gmail_send_attempts(idempotency_key) VALUES (?1)",
            [key],
        )
        .map(|count| count == 1)
        .map_err(|e| e.to_string())
    }
    fn gmail_bulk_recipients(&self) -> Result<Vec<crate::GmailBulkRecipient>, String> {
        let db = self.read_connection()?;
        let mut stmt=db.prepare("SELECT l.normalized_value,c.display_name || ' · ' || r.confidence FROM contact_locators l JOIN creator_contact_routes r ON r.locator_id=l.id JOIN creators c ON c.id=r.creator_id WHERE l.channel='email' AND l.kind='email_address' AND c.archived_at IS NULL UNION SELECT o.normalized_value,o.source_url || ' · ' || o.attribution FROM discovery_contact_observations o JOIN discovery_campaigns c ON c.id=o.discovery_campaign_id WHERE o.kind='email' AND o.criteria_revision=c.current_revision ORDER BY 1,2 LIMIT 500").map_err(|e|e.to_string())?;
        stmt.query_map([], |r| {
            Ok(crate::GmailBulkRecipient {
                email: r.get(0)?,
                source: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())
    }
    fn create_gmail_bulk(&self, mut batch: GmailBulkBatch) -> Result<GmailBulkBatch, String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO gmail_bulk_batches VALUES (?1,?2,?3,?4,'draft',?5)",
            params![
                batch.id,
                batch.mailbox_ref_id.as_str(),
                batch.subject,
                batch.body,
                batch.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
        for item in &mut batch.items {
            let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM gmail_bulk_items WHERE fingerprint=?1 AND state IN ('attempting','accepted','sent','unknown'))",[&item.fingerprint],|r|r.get(0)).map_err(|e|e.to_string())?;
            if duplicate {
                item.detail = Some(
                    "Identical mail was already attempted; this recipient will be skipped".into(),
                );
            }
            tx.execute(
                "INSERT INTO gmail_bulk_items VALUES (?1,?2,?3,?4,?5,'pending',NULL,?6)",
                params![
                    item.id,
                    batch.id,
                    item.recipient,
                    item.source,
                    item.fingerprint,
                    item.detail
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(batch)
    }
    fn list_gmail_bulk(&self) -> Result<Vec<GmailBulkBatch>, String> {
        let mut db = self.read_connection()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred).map_err(|e| e.to_string())?;
        let ids = tx
            .prepare(
                "SELECT id FROM gmail_bulk_batches ORDER BY created_at DESC,rowid DESC LIMIT 100",
            )
            .map_err(|e| e.to_string())?
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let batches=ids.iter().map(|id| load(&tx, id)).collect::<Result<Vec<_>,_>>()?;
        tx.commit().map_err(|e|e.to_string())?;
        Ok(batches)
    }
    fn control_gmail_bulk(&self, id: &str, action: &str) -> Result<(), String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let batch = load(&tx, id)?;
        let next = match (batch.state.as_str(), action) {
            ("draft", "confirm") | ("paused", "resume") => "running",
            ("running", "pause") => "paused",
            ("draft" | "running" | "paused", "cancel") => "cancelled",
            ("running", "confirm" | "resume") | ("paused", "pause") | ("cancelled", "cancel") => {
                return Ok(());
            }
            _ => return Err("invalid bulk mail state transition".into()),
        };
        tx.execute(
            "UPDATE gmail_bulk_batches SET state=?2 WHERE id=?1",
            params![id, next],
        )
        .map_err(|e| e.to_string())?;
        if next == "cancelled" {
            tx.execute("UPDATE gmail_bulk_items SET state='cancelled' WHERE batch_id=?1 AND state='pending'",[id]).map_err(|e|e.to_string())?;
        }
        tx.execute("UPDATE gmail_bulk_batches SET state='completed' WHERE id=?1 AND state='running' AND NOT EXISTS (SELECT 1 FROM gmail_bulk_items WHERE batch_id=?1 AND state IN ('pending','attempting'))",[id]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    fn claim_gmail_bulk(&self) -> Result<Option<(GmailBulkBatch, GmailBulkItem)>, String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let next:Option<(String,String)>=tx.query_row("SELECT b.id,i.id FROM gmail_bulk_batches b JOIN gmail_bulk_items i ON i.batch_id=b.id WHERE b.state='running' AND i.state='pending' ORDER BY b.created_at,i.rowid LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        let Some((batch_id, item_id)) = next else {
            return Ok(None);
        };
        let batch = load(&tx, &batch_id)?;
        let mut item = batch
            .items
            .iter()
            .find(|i| i.id == item_id)
            .cloned()
            .ok_or("bulk item missing")?;
        let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM gmail_bulk_items WHERE fingerprint=?1 AND id<>?2 AND state IN ('attempting','accepted','sent','unknown'))",params![item.fingerprint,item.id],|r|r.get(0)).map_err(|e|e.to_string())?;
        item.state = if duplicate { "failed" } else { "attempting" }.into();
        item.detail =
            duplicate.then(|| "Identical mail was already attempted; duplicate blocked".into());
        tx.execute(
            "UPDATE gmail_bulk_items SET state=?2,detail=?3 WHERE id=?1",
            params![item.id, item.state, item.detail],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Some((batch, item)))
    }
    fn finish_gmail_bulk(&self, item: GmailBulkItem) -> Result<(), String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute("UPDATE gmail_bulk_items SET state=?2,provider_message_id=?3,detail=?4 WHERE id=?1 AND state IN ('attempting','failed')",params![item.id,item.state,item.provider_message_id,item.detail]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE gmail_bulk_batches SET state='completed' WHERE state='running' AND NOT EXISTS (SELECT 1 FROM gmail_bulk_items i WHERE i.batch_id=gmail_bulk_batches.id AND i.state IN ('pending','attempting'))",[]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    fn interrupt_gmail_bulk(&self) -> Result<(), String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute("UPDATE gmail_bulk_items SET state='unknown',detail='Service interrupted after reservation; automatic resend disabled' WHERE state='attempting'",[]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE gmail_bulk_batches SET state='paused' WHERE state='running'",
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
}
