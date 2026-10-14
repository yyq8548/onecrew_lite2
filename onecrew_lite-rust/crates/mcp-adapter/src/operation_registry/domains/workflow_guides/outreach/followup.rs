use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "outreach-followup",
    title: "Follow up with non-responders",
    questions: &[
        "How long without a reply should this request look back, and how many contacts should it include at most?",
    ],
    workflow: "Check contact history and unreplied status only for this execution, and prepare a short follow-up draft; show the recipient and full text, then send and read back only after receiving explicit send authorization. This flow does not create monitoring or scheduled tasks.",
    queries: &[
        "outreach conversation history unreplied contacts",
        "followup draft prepare send readback",
    ],
    expected: "Non-responding contacts for this request and follow-up drafts; readback results after authorized sending",
    example: "Find people contacted [number of days] ago who have not replied, and prepare a short follow-up message for each. Show the recipients and drafts first, then wait for my confirmation before sending.",
};
