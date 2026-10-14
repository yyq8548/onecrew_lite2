use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "find-replies",
    title: "Find people in replies",
    questions: &[
        "What are the post link, selection criteria, and maximum number for this request?",
    ],
    workflow: "Read replies to the specified post, filter accounts by the criteria, and show the original reply, source, and rationale.",
    queries: &["post comments replies creator profiles"],
    expected: "Accounts, original replies, selection rationale, and sources",
    example: "Review the comments and replies on [post link] and find suitable accounts under [selection criteria]. List each account, original reply, and selection rationale, then wait for my confirmation before adding them to the list.",
};
