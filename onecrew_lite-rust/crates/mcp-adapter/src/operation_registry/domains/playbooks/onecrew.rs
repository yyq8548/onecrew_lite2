use crate::operation_registry::descriptor::PlaybookDifficulty;
use crate::operation_registry::domains::playbooks::descriptor::{add, step};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};
use onecrew_local_runtime::ToolkitId;

pub(super) fn register_creator_workflows(
    builder: &mut RegistryBuilder,
) -> Result<(), RegistryBuildError> {
    // Owner-policy playbooks.
    add(
        builder,
        "playbook.onecrew.creator-candidate-save",
        "Save One Creator Candidate",
        "Prepare and confirm saving one selected platform account as a Creator, then read it back.",
        "Save one selected platform target directly: let preparation obtain fresh platform data, review contact routes, confirm in the App and read back the Creator.",
        &[ToolkitId::OneCrew],
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str()] },
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
        &[
            step(
                "Use the current candidate-save input contract to pass the explicit per-candidate intent, target, connection, real provenance, notes, tags and quality directly; do not add a save-only prefetch; present the returned result_ref for review.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str()] },
            ),
            step(
                "The user confirms rows and routes through the App-only onecrew_result_state action confirm_creator_save_batch; read back the saved Creator after completion.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
            ),
        ],
        &[
            "Original discovery and candidate IDs are provenance, not cache keys. Preparation writes no Creator rows or reservations. Follow actual row_id values through the review and outcome.",
        ],
        PlaybookDifficulty::Intermediate,
    )?;
    add(
        builder,
        "playbook.onecrew.creator-candidate-batch-save",
        "Save Creator Candidates in a Batch",
        "Prepare several selected candidates together, review their contact routes in one batch card, confirm selected rows, and read back the saved Creators.",
        "Save multiple selected accounts through one OneCrew batch payload. The business batch supports up to 50 candidates; it is separate from the eight-call MCP transport envelope and provider-read concurrency.",
        &[ToolkitId::OneCrew],
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_BATCH_SAVE.as_str()] },
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
        &[
            step(
                "Fetch the current batch-save input contract and pass one candidates array with one consistent explicit intent plus target, connection, real provenance, notes, tags and quality per candidate; present the returned result_ref review.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_BATCH_SAVE
                            .as_str(),
                    ]
                },
            ),
            step(
                "The user confirms selected rows and contact routes through the App-only onecrew_result_state action confirm_creator_save_batch, then read back each saved Creator.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
            ),
        ],
        &[
            "Use the batch operation for more than one candidate; do not repeat onecrew.creator.candidate.save.",
            "No Creator is persisted before explicit route confirmation.",
        ],
        PlaybookDifficulty::Intermediate,
    )?;
    add(
        builder,
        "playbook.onecrew.creator-manual-batch-create",
        "Create Manual Creators in a Batch",
        "Create up to 50 explicit manual Creators from one confirmed payload and verify the persisted Creator records.",
        "Use onecrew.creator.batch.create once with the complete items array. The 50-item business limit is independent of the eight-call onecrew_tool_execute transport envelope.",
        &[ToolkitId::OneCrew],
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_BATCH_CREATE.as_str()] },
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
        &[
            step(
                "Map explicit Creator rows into one batch payload with a stable idempotency key.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_BATCH_CREATE.as_str()] },
            ),
            step(
                "Read back each accepted Creator from the authoritative Creator projection.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
            ),
        ],
        &[
            "Send the complete items array in one operation; do not emit one onecrew.creator.manual.create call per row.",
            "Set confirmed to true only when the user has authorized the complete batch.",
            "Use strict validation when every row must pass, and permissive validation when deterministic invalid rows may be reported individually.",
        ],
        PlaybookDifficulty::Intermediate,
    )?;
    add(
        builder,
        "playbook.onecrew.creator-contact-confirm",
        "Confirm Creator Contact",
        "Prepare fresh profile and link-hub data directly from a saved Creator's verified platform locator and explicitly confirm contact routes before treating them as durable facts.",
        "Prepare a fresh review from the selected locator, then confirm selected routes through the App.",
        &[ToolkitId::OneCrew],
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str()] },
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
        &[
            step(
                "Pass the verified locator, connection and real provenance directly using the current save contract. The runtime reads fresh profile and link-hub data; present the returned result_ref and inspect the identity resolution and offered routes.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str()] },
            ),
            step(
                "The user confirms only selected rows and routes in the App, then read back the Creator after completion.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
            ),
        ],
        &["Do not confirm an unverified route merely because it was discovered."],
        PlaybookDifficulty::Intermediate,
    )?;
    add(
        builder,
        "playbook.onecrew.creator-profile-update",
        "Update a Creator Profile",
        "Read the platform profile, write the Creator fields the user explicitly chose, and verify the authoritative Creator projection.",
        "Update a saved Creator's follower count or display name from a separate platform read, then read back its Creator projection.",
        &[ToolkitId::OneCrew],
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_UPDATE.as_str()] },
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
        &[
            step(
                "Read the account's platform-native profile with its platform profile or channel get, such as x.profile.get, instagram.profile.get or youtube.channel.get; the read writes no Creator data.",
                &[],
            ),
            step(
                "Write only the follower count or display name the user explicitly chose.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_UPDATE.as_str()] },
            ),
            step(
                "Read back the authoritative Creator projection.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
            ),
        ],
        &[
            "Avatar, handle, bio and homepage changes are observations, not Creator update fields; save the same account again through onecrew.creator.candidate.save to append them.",
        ],
        PlaybookDifficulty::Intermediate,
    )?;
    Ok(())
}

pub(super) fn register_direct_message(
    builder: &mut RegistryBuilder,
) -> Result<(), RegistryBuildError> {
    add(
        builder,
        "playbook.onecrew.direct-message",
        "Send a Direct Message",
        "Resolve a saved contact target, prepare a message, confirm the send, and verify action status.",
        "Send one Creator-routed platform message with confirmation and readback.",
        &[ToolkitId::OneCrew],
        const {
            &[
                onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_TARGET_RESOLVE
                    .as_str(),
                onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_PREPARE.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_SEND.as_str(),
            ]
        },
        const { &[onecrew_local_runtime::OperationId::ONECREW_OUTREACH_ACTION_STATUS.as_str()] },
        &[
            step(
                "Resolve the saved route and inspect sending capability and relationship preflight.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_TARGET_RESOLVE
                            .as_str(),
                        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_PREPARE
                            .as_str(),
                    ]
                },
            ),
            step(
                "Submit the exact message and stable key for a result-owned review, then confirm in the card and read back action status.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_SEND.as_str(),
                        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_ACTION_STATUS.as_str(),
                    ]
                },
            ),
        ],
        &[
            "Never guess a target when route resolution is unresolved.",
            "Provider verification is incomplete until Creator recording succeeds. A record_pending receipt must be recovered without sending again.",
        ],
        PlaybookDifficulty::Advanced,
    )?;

    Ok(())
}
