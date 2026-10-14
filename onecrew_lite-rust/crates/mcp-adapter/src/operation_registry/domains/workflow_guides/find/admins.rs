use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "find-admins",
    title: "Find group administrators",
    questions: &[
        "What is the Telegram group or channel link? Should this include human administrators only?",
    ],
    workflow: "Check whether the current account can access the target, read the group or channel administrators, and exclude bots; clearly state when access is unavailable.",
    queries: &["Telegram community administrators discover"],
    expected: "Confirmed human administrators, public profiles, and access limitations",
    example: "Review the administrators of [Telegram group or channel link], excluding bots; list confirmed human administrators and public profiles, then wait for my confirmation before adding them to the list and saving Telegram contact routes. State directly if the current account cannot view them.",
};
