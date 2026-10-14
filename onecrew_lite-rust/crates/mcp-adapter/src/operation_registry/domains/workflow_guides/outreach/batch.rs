use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "outreach-batch",
    title: "Contact in bulk",
    questions: &[
        "What are the contact selection criteria, maximum number for this request, channels, and collaboration points?",
    ],
    workflow: "Select contacts within this request's limit, resolve each person's channel, and prepare a distinct first message for each; show all recipients and drafts, then send and read back each item only after receiving explicit send authorization.",
    queries: &[
        "OneCrew contacts filter contact target resolve",
        "batch outreach drafts send readback",
    ],
    expected: "Recipient list and complete per-person drafts; itemized results after authorized sending",
    example: "Select contacts under [selection criteria] and prepare a different first message for each based on [collaboration points], up to [number] people in this request. Show the recipient list and all drafts first, then wait for my confirmation before sending.",
};
