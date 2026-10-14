use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "outreach-send",
    title: "Contact one person",
    questions: &["What are the recipient, channel, collaboration details, and tone?"],
    workflow: "Resolve the current contact and channel, then read the available connection and contact route; show the recipient and complete draft, and send and read back under the live contract only after receiving explicit send authorization.",
    queries: &[
        "contact target resolve connection availability",
        "message draft prepare send readback",
    ],
    expected: "Recipient and complete draft; result and readback status after authorized sending",
    example: "Contact [name] through [Telegram, email, X, or Instagram] to explain [collaboration details] in a [formal or casual] tone. Show the recipient and complete draft first, then wait for my confirmation before sending.",
};
