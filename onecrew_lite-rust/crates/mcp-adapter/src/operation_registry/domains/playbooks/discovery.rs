use crate::operation_registry::descriptor::PlaybookDifficulty;
use crate::operation_registry::domains::playbooks::descriptor::{add, step};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};
use onecrew_local_runtime::ToolkitId;

pub(super) fn register(builder: &mut RegistryBuilder) -> Result<(), RegistryBuildError> {
    // Toolkit read playbooks.
    add(
        builder,
        "playbook.x.creator-discovery",
        "Discover X Authors Through Matching Posts",
        "Find X creators through advanced post search, deduplicate matching authors, and inspect profiles or recent posts only when the requested quality evidence is missing.",
        "Find X creator accounts through matching posts and inspect public profiles only when search results lack the requested quality evidence.",
        &[ToolkitId::X],
        const {
            &[
                onecrew_local_runtime::OperationId::X_POSTS_SEARCH_ADVANCED.as_str(),
                onecrew_local_runtime::OperationId::X_PROFILE_GET.as_str(),
                onecrew_local_runtime::OperationId::X_USER_POSTS_LIST.as_str(),
            ]
        },
        &[],
        &[
            step(
                "Search X posts with typed advanced criteria.",
                const { &[onecrew_local_runtime::OperationId::X_POSTS_SEARCH_ADVANCED.as_str()] },
            ),
            step(
                "Deduplicate matching authors by stable X identity and keep every source; use search evidence first, then inspect one profile or recent-post page only for one decision-changing gap.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::X_PROFILE_GET.as_str(),
                        onecrew_local_runtime::OperationId::X_USER_POSTS_LIST.as_str(),
                    ]
                },
            ),
        ],
        &["Use the returned X handle; do not substitute another platform identifier."],
        PlaybookDifficulty::Basic,
    )?;
    add(
        builder,
        "playbook.instagram.creator-discovery",
        "Find Creators on Instagram",
        "Search Instagram accounts and inspect one profile with Instagram-specific identifiers only for a decision-changing evidence gap.",
        "Find relevant Instagram accounts and inspect a selected profile only for a decision-changing evidence gap.",
        &[ToolkitId::Instagram],
        const {
            &[
                onecrew_local_runtime::OperationId::INSTAGRAM_USERS_SEARCH.as_str(),
                onecrew_local_runtime::OperationId::INSTAGRAM_PROFILE_GET.as_str(),
            ]
        },
        &[],
        &[
            step(
                "Search Instagram accounts by keyword.",
                const { &[onecrew_local_runtime::OperationId::INSTAGRAM_USERS_SEARCH.as_str()] },
            ),
            step(
                "Use returned account evidence first; inspect one selected profile only for one decision-changing gap, never as a mechanical prefetch.",
                const { &[onecrew_local_runtime::OperationId::INSTAGRAM_PROFILE_GET.as_str()] },
            ),
        ],
        &["Instagram handles are toolkit-specific locators."],
        PlaybookDifficulty::Basic,
    )?;
    add(
        builder,
        "playbook.telegram.community-discovery",
        "Find Telegram Communities",
        "Search Telegram communities and preserve the provider's public chat IDs and kinds.",
        "Find Telegram communities relevant to a topic.",
        &[ToolkitId::Telegram],
        const { &[onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITIES_SEARCH.as_str()] },
        &[],
        &[
            step(
                "Search Telegram communities by keyword.",
                const { &[onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITIES_SEARCH.as_str()] },
            ),
            step(
                "Review the native public chat objects and select their exact IDs and kinds.",
                &[],
            ),
        ],
        &["Telegram community search has no continuation cursor and creates no Creator snapshots."],
        PlaybookDifficulty::Basic,
    )?;
    add(
        builder,
        "playbook.youtube.channel-discovery",
        "Find YouTube Channels",
        "Search YouTube channels and deduplicate stable channel IDs. Connection and read depth identify equivalent reads. Inspect a channel only for a decision-changing quality evidence gap; pass depth: light or full; light omits comments.",
        "Discover YouTube channels and inspect one channel when quality evidence is missing; preserve every hit's source.",
        &[ToolkitId::Youtube],
        const {
            &[
                onecrew_local_runtime::OperationId::YOUTUBE_CHANNELS_SEARCH.as_str(),
                onecrew_local_runtime::OperationId::YOUTUBE_CHANNEL_GET.as_str(),
            ]
        },
        &[],
        &[
            step(
                "Search YouTube channels by keyword.",
                const { &[onecrew_local_runtime::OperationId::YOUTUBE_CHANNELS_SEARCH.as_str()] },
            ),
            step(
                "Deduplicate hits by stable channel ID, not connection or depth; those identify equivalent reads. Continue without an extra channel read when evidence suffices; read one channel for one decision-changing gap at depth: light or full.",
                const { &[onecrew_local_runtime::OperationId::YOUTUBE_CHANNEL_GET.as_str()] },
            ),
        ],
        &[
            "Search items retain channel_id; pass next_page_token as cursor. youtube.channel.get accepts a channel ID, not a handle.",
        ],
        PlaybookDifficulty::Basic,
    )?;
    add(
        builder,
        "playbook.x.thread-read",
        "Read an X Thread",
        "Locate X posts and read the thread for a selected post.",
        "Find an X post and read its conversation thread.",
        &[ToolkitId::X],
        const {
            &[
                onecrew_local_runtime::OperationId::X_POSTS_SEARCH_ADVANCED.as_str(),
                onecrew_local_runtime::OperationId::X_POST_THREAD_GET.as_str(),
            ]
        },
        &[],
        &[
            step(
                "Search X posts with typed advanced criteria.",
                const { &[onecrew_local_runtime::OperationId::X_POSTS_SEARCH_ADVANCED.as_str()] },
            ),
            step(
                "Read the selected post thread.",
                const { &[onecrew_local_runtime::OperationId::X_POST_THREAD_GET.as_str()] },
            ),
        ],
        &["Carry the exact post locator returned by X search."],
        PlaybookDifficulty::Basic,
    )?;

    Ok(())
}
