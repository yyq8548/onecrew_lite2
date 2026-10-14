use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "enrich-followers",
    title: "Update follower counts",
    questions: &["Which list or selection scope needs follower counts filled in?"],
    workflow: "Read the accounts in the specified list that are missing follower counts, review them again on their platforms, show field changes and failures, then save and read back under the live update contract.",
    queries: &[
        "OneCrew creator list missing follower counts",
        "creator update follower counts",
    ],
    expected: "Follower-count changes, accounts that could not be read, and save readback results",
    example: "Find the people in the list who have no follower count, review their public profiles again on their respective platforms, and fill in the counts; list any accounts that cannot be read separately.",
};
