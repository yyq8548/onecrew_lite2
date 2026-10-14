use crate::{GmailBulkBatch, GmailBulkItem};

pub trait GmailBulkRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn reserve_gmail_send_attempt(&self, key: &str) -> Result<bool, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn gmail_bulk_recipients(&self) -> Result<Vec<crate::GmailBulkRecipient>, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn create_gmail_bulk(&self, batch: GmailBulkBatch) -> Result<GmailBulkBatch, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_gmail_bulk(&self) -> Result<Vec<GmailBulkBatch>, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn control_gmail_bulk(&self, id: &str, action: &str) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_gmail_bulk(&self) -> Result<Option<(GmailBulkBatch, GmailBulkItem)>, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn finish_gmail_bulk(&self, item: GmailBulkItem) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn interrupt_gmail_bulk(&self) -> Result<(), String>;
}
