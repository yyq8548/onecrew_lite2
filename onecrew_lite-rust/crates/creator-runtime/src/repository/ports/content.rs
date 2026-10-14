use crate::CommitContentReplyActionResult;

pub trait ContentRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(write))]
    fn record_creator_content_reply(
        &self,
        key: &str,
    ) -> Result<CommitContentReplyActionResult, crate::CreatorOutreachError>;
}
