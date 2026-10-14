use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "outreach-sync",
    title: "Review latest messages",
    questions: &["What are the platform, contact, and message time range for this request?"],
    workflow: "Sync messages for the specified contact under the live contract, distinguish newly received from newly sent content, and prepare a summary; do not send messages.",
    queries: &["outreach conversation sync messages"],
    expected: "Summary of newly received and newly sent messages, plus synchronization failures",
    example: "Sync the latest messages with [name or username] on [platform], organize only newly received and newly sent content, and provide a summary; do not send any messages.",
};
