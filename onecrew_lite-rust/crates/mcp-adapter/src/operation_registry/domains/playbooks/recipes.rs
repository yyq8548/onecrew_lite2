use onecrew_local_runtime::ToolkitId;

use crate::operation_registry::domains::playbooks::descriptor::{recipe, step};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};

const MATCH_AND_DEDUP: &str = "Apply the user's original request and confirmed criteria. Distinguish matched, unmatched and unverified; group hits by each platform's stable identity, retain every source, and never merge same names across platforms.";
const TARGETED_EVIDENCE: &str = "Continue without an extra profile read when evidence suffices. Reuse reads by connection, operation revision, params, depth and cursor. Read once per decision-changing gap; if still unknown, stop. Follow the live workflow guide.";
const GRADE_AND_STOP: &str = "Retrieve the shared grade rubric from the live find-batch workflow guide; keep match separate from grade and write an individual evidence-based reason. Stop when the match target is met; use the returned cursor for an approved next page.";
const PREPARE_REVIEW: &str = "Use explicit add_new by default, or save_or_update only when requested; pass the exact locator, tagged connection, real provenance, quality and intent. Prepare gets fresh data, does not write or auto-confirm; present result_ref and wait.";
const CONFIRM_AND_READBACK: &str = "After the user confirms the reviewed rows in the App, read back each saved Creator; preparation does not write.";

pub(super) fn register(builder: &mut RegistryBuilder) -> Result<(), RegistryBuildError> {
    // Genuine cross-toolkit recipes.
    recipe(
        builder,
        "playbook.recipe.find-x-creators",
        "Find and Save X Creators",
        &[ToolkitId::X, ToolkitId::OneCrew],
        const {
            &[
                onecrew_local_runtime::OperationId::X_POSTS_SEARCH_ADVANCED.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str(),
            ]
        },
        const {
            &[
                onecrew_local_runtime::OperationId::X_PROFILE_GET.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str(),
            ]
        },
        &[
            step(
                "Search X posts with advanced criteria and retain matching author locators and original source metadata.",
                const { &[onecrew_local_runtime::OperationId::X_POSTS_SEARCH_ADVANCED.as_str()] },
            ),
            step(MATCH_AND_DEDUP, &[]),
            step(
                TARGETED_EVIDENCE,
                const { &[onecrew_local_runtime::OperationId::X_PROFILE_GET.as_str()] },
            ),
            step(GRADE_AND_STOP, &[]),
            step(
                PREPARE_REVIEW,
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str()] },
            ),
            step(
                CONFIRM_AND_READBACK,
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
            ),
        ],
        "Search X, inspect candidates, then save one selected candidate as a Creator.",
    )?;
    recipe(
        builder,
        "playbook.recipe.find-instagram-creators",
        "Find and Save Instagram Creators",
        &[ToolkitId::Instagram, ToolkitId::OneCrew],
        const {
            &[
                onecrew_local_runtime::OperationId::INSTAGRAM_USERS_SEARCH.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str(),
            ]
        },
        const {
            &[
                onecrew_local_runtime::OperationId::INSTAGRAM_PROFILE_GET.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str(),
            ]
        },
        &[
            step(
                "Search Instagram accounts and retain selected account locators and original source metadata.",
                const { &[onecrew_local_runtime::OperationId::INSTAGRAM_USERS_SEARCH.as_str()] },
            ),
            step(MATCH_AND_DEDUP, &[]),
            step(
                TARGETED_EVIDENCE,
                const { &[onecrew_local_runtime::OperationId::INSTAGRAM_PROFILE_GET.as_str()] },
            ),
            step(GRADE_AND_STOP, &[]),
            step(
                PREPARE_REVIEW,
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str()] },
            ),
            step(
                CONFIRM_AND_READBACK,
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
            ),
        ],
        "Search Instagram, inspect candidates, then save one selected candidate as a Creator.",
    )?;
    recipe(
        builder,
        "playbook.recipe.find-youtube-creators",
        "Find and Save YouTube Creators",
        &[ToolkitId::Youtube, ToolkitId::OneCrew],
        const {
            &[
                onecrew_local_runtime::OperationId::YOUTUBE_CHANNELS_SEARCH.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str(),
            ]
        },
        const {
            &[
                onecrew_local_runtime::OperationId::YOUTUBE_CHANNEL_GET.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str(),
            ]
        },
        &[
            step(
                "Search YouTube channels and retain selected channel locators and original source metadata.",
                const { &[onecrew_local_runtime::OperationId::YOUTUBE_CHANNELS_SEARCH.as_str()] },
            ),
            step(MATCH_AND_DEDUP, &[]),
            step(
                "Merge duplicate hits by stable channel ID, not connection or depth. Continue without an extra channel read when evidence suffices; for a decision-changing gap use depth: light or full once, then leave unknown.",
                const { &[onecrew_local_runtime::OperationId::YOUTUBE_CHANNEL_GET.as_str()] },
            ),
            step(GRADE_AND_STOP, &[]),
            step(
                PREPARE_REVIEW,
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str()] },
            ),
            step(
                CONFIRM_AND_READBACK,
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
            ),
        ],
        "Search YouTube channels, inspect one channel, then save the selected candidate as a Creator.",
    )?;
    recipe(
        builder,
        "playbook.recipe.find-telegram-administrators",
        "Find and Save Telegram Administrators",
        &[ToolkitId::Telegram, ToolkitId::OneCrew],
        const {
            &[
                onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITIES_SEARCH.as_str(),
                onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITY_GET.as_str(),
                onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITY_ADMINISTRATORS_LIST.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str(),
            ]
        },
        const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
        &[
            step(
                "Search Telegram communities and keep the exact selected community locator.",
                const { &[onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITIES_SEARCH.as_str()] },
            ),
            step(MATCH_AND_DEDUP, &[]),
            step(
                "Read the selected community and administrator page only for one decision-changing gap; keep public identity and role, never phone numbers or access hashes. If it stays unknown, stop.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITY_GET.as_str(),
                        onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITY_ADMINISTRATORS_LIST
                            .as_str(),
                    ]
                },
            ),
            step(GRADE_AND_STOP, &[]),
            step(
                PREPARE_REVIEW,
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str()] },
            ),
            step(
                CONFIRM_AND_READBACK,
                const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
            ),
        ],
        "Discover administrators of a Telegram community, select one account to save as a Creator, then review, confirm and read back its saved record.",
    )?;
    recipe(
        builder,
        "playbook.recipe.email-best-route",
        "Outreach by Email Route",
        &[ToolkitId::OneCrew, ToolkitId::Email],
        const {
            &[
                onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_TARGET_RESOLVE
                    .as_str(),
                onecrew_local_runtime::OperationId::EMAIL_DRAFT_SAVE.as_str(),
                onecrew_local_runtime::OperationId::EMAIL_DRAFT_SEND.as_str(),
            ]
        },
        const { &[onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()] },
        &[
            step(
                "Resolve the best saved Creator contact target without guessing a route.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_TARGET_RESOLVE
                            .as_str(),
                    ]
                },
            ),
            step(
                "Save the canonical email draft and retain its revision.",
                const { &[onecrew_local_runtime::OperationId::EMAIL_DRAFT_SAVE.as_str()] },
            ),
            step(
                "Send the confirmed draft revision.",
                const { &[onecrew_local_runtime::OperationId::EMAIL_DRAFT_SEND.as_str()] },
            ),
            step(
                "Verify provider delivery in the canonical email thread list.",
                const { &[onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str()] },
            ),
        ],
        "Resolve the best saved contact route, draft the email, send with confirmation, and verify the thread.",
    )?;
    recipe(
        builder,
        "playbook.recipe.x-best-route",
        "Outreach by X Route",
        &[ToolkitId::X, ToolkitId::OneCrew],
        const {
            &[
                onecrew_local_runtime::OperationId::X_CONNECTION_GET.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_TARGET_RESOLVE
                    .as_str(),
                onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_SEND.as_str(),
            ]
        },
        const { &[onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_PREPARE.as_str()] },
        &[
            step(
                "Verify the selected X operator connection is available.",
                const { &[onecrew_local_runtime::OperationId::X_CONNECTION_GET.as_str()] },
            ),
            step(
                "Resolve the saved Creator contact target without guessing a route.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_TARGET_RESOLVE
                            .as_str(),
                    ]
                },
            ),
            step(
                "Prepare the exact direct-message intent and retain its operation ID.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_PREPARE
                            .as_str(),
                    ]
                },
            ),
            step(
                "Confirm and send that prepared message intent.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_SEND.as_str()] },
            ),
        ],
        "Verify X connection availability, resolve the Creator route, and send a confirmed message.",
    )?;
    recipe(
        builder,
        "playbook.recipe.instagram-best-route",
        "Outreach by Instagram Route",
        &[ToolkitId::Instagram, ToolkitId::OneCrew],
        const {
            &[
                onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_GET.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_TARGET_RESOLVE
                    .as_str(),
                onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_SEND.as_str(),
            ]
        },
        const { &[onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_PREPARE.as_str()] },
        &[
            step(
                "Verify the selected Instagram operator connection is available.",
                const { &[onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_GET.as_str()] },
            ),
            step(
                "Resolve the saved Creator contact target without guessing a route.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_TARGET_RESOLVE
                            .as_str(),
                    ]
                },
            ),
            step(
                "Prepare the exact direct-message intent and retain its operation ID.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_PREPARE
                            .as_str(),
                    ]
                },
            ),
            step(
                "Confirm and send that prepared message intent.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_SEND.as_str()] },
            ),
        ],
        "Verify Instagram connection availability, resolve the Creator route, and send a confirmed message.",
    )?;
    recipe(
        builder,
        "playbook.recipe.triage-email-and-onecrew",
        "Triage Email into OneCrew",
        &[ToolkitId::Email, ToolkitId::OneCrew],
        const {
            &[
                onecrew_local_runtime::OperationId::EMAIL_MAILBOX_SYNC.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_TRIAGE_RECORD.as_str(),
            ]
        },
        const {
            &[
                onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_TRIAGE_LIST.as_str(),
                onecrew_local_runtime::OperationId::ONECREW_TRIAGE_ACKNOWLEDGE.as_str(),
            ]
        },
        &[
            step(
                "Sync the mailbox and list its current canonical threads.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::EMAIL_MAILBOX_SYNC.as_str(),
                        onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str(),
                    ]
                },
            ),
            step(
                "Record one OneCrew triage verdict per inbound reply.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_TRIAGE_RECORD.as_str()] },
            ),
            step(
                "Read back the durable triage state before deciding it is resolved.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_TRIAGE_LIST.as_str()] },
            ),
            step(
                "Acknowledge the durable reminder only after the resolved verdict is verified.",
                const { &[onecrew_local_runtime::OperationId::ONECREW_TRIAGE_ACKNOWLEDGE.as_str()] },
            ),
        ],
        "Sync email, record one OneCrew verdict per reply, and acknowledge the durable reminder when resolved.",
    )?;

    Ok(())
}
