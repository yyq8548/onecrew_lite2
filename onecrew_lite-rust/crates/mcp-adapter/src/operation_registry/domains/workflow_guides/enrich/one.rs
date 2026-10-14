use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: const {
        &[
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_UPDATE,
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE,
            onecrew_local_runtime::OperationId::ONECREW_LINK_HUB_EXTRACT,
        ]
    },
    topic: "enrich-one",
    title: "Update one account",
    questions: &["What are the platform, account, and fields to update for this request?"],
    workflow: "Follow three independent paths: first, read the account's native platform data through profile or channel on the target platform, compare it with the saved Creator, and show changes; the read itself does not write Creator. Second, write follower counts or display names that the user explicitly wants changed through onecrew.creator.update. Third, for observed changes to the avatar, handle, bio, homepage, or similar fields, run prepare and confirmation for the same account through onecrew.creator.candidate.save and append them as historical observations. Expand new contact routes through onecrew.link_hub.extract and merge them through onecrew.creator.candidate.save as well. Finally read back Creator.",
    queries: &[
        "profile get channel get",
        "creator update follower counts",
        "creator candidate save link hub extract",
    ],
    expected: "Changes in native platform data, Creator updates or candidate-save confirmation results, and readback results",
    example: "Review the public profile at [username or profile link] on [platform] again and list the changes; update the latest follower count on the saved Creator, and route avatar, bio, and homepage changes through candidate-save confirmation.",
};
