use crate::operation_registry::descriptor::PlaybookDifficulty;
use crate::operation_registry::domains::playbooks::descriptor::{add, step};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};
use onecrew_local_runtime::ToolkitId;

pub(super) fn register(builder: &mut RegistryBuilder) -> Result<(), RegistryBuildError> {
    // Phase 6 connection lifecycle playbooks. These were deliberately held
    // back until every referenced typed operation existed in the registry.
    add(
        builder,
        "playbook.x.connection-browser",
        "Connect an X Operator",
        "Start, inspect, complete, or cancel one visible X browser login.",
        "Connect one X operator account through the attended browser lifecycle.",
        &[ToolkitId::X],
        const {
            &[
                onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_START.as_str(),
                onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_STATUS.as_str(),
                onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_COMPLETE.as_str(),
                onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_CANCEL.as_str(),
            ]
        },
        &[],
        &[
            step(
                "Start the X browser login with its selected proxy and X Chat preference.",
                const { &[onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_START.as_str()] },
            ),
            step(
                "Complete the visible provider login, polling bounded status when needed.",
                const { &[onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_STATUS.as_str()] },
            ),
            step(
                "Confirm completion only after the browser interaction succeeds.",
                const { &[onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_COMPLETE.as_str()] },
            ),
            step(
                "Cancel the same operation if the attended login will not continue.",
                const { &[onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_CANCEL.as_str()] },
            ),
        ],
        &[
            "Never reuse an Instagram operation ID or submit an X Chat passcode through another toolkit.",
        ],
        PlaybookDifficulty::Advanced,
    )?;
    add(
        builder,
        "playbook.instagram.connection-browser",
        "Connect an Instagram Operator",
        "Start, inspect, complete, or cancel one visible Instagram browser login.",
        "Connect one Instagram operator account through the attended browser lifecycle.",
        &[ToolkitId::Instagram],
        const {
            &[
                onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_START.as_str(),
                onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_STATUS.as_str(),
                onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_COMPLETE.as_str(),
                onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_CANCEL.as_str(),
            ]
        },
        &[],
        &[
            step(
                "Start the Instagram browser login with its selected proxy.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_START
                            .as_str(),
                    ]
                },
            ),
            step(
                "Complete the visible provider login, polling bounded status when needed.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_STATUS
                            .as_str(),
                    ]
                },
            ),
            step(
                "Confirm completion only after the browser interaction succeeds.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_COMPLETE
                            .as_str(),
                    ]
                },
            ),
            step(
                "Cancel the same operation if the attended login will not continue.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_CANCEL
                            .as_str(),
                    ]
                },
            ),
        ],
        &["Instagram login never accepts an X Chat passcode or an X operation ID."],
        PlaybookDifficulty::Advanced,
    )?;
    add(
        builder,
        "playbook.telegram.connection-qr",
        "Connect Telegram by QR",
        "Start and monitor a Telegram QR login, including a possible 2FA continuation and cancellation.",
        "Connect one Telegram operator account with QR login and bounded continuation handling.",
        &[ToolkitId::Telegram],
        const {
            &[
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_START.as_str(),
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS.as_str(),
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_TWO_FACTOR_SUBMIT
                    .as_str(),
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_CANCEL.as_str(),
            ]
        },
        &[],
        &[
            step(
                "Start Telegram with method=qr and the required SOCKS5 proxy.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_START
                            .as_str(),
                    ]
                },
            ),
            step(
                "Poll status only until the projected operation expiry.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS
                            .as_str(),
                    ]
                },
            ),
            step(
                "If status requests 2FA, submit it only through the transient secret field.",
                const {
                    &[
                    onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_TWO_FACTOR_SUBMIT
                        .as_str(),
                ]
                },
            ),
            step(
                "Cancel the same operation when the QR flow expires or is abandoned.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_CANCEL
                            .as_str(),
                    ]
                },
            ),
        ],
        &["Do not substitute phone login or expose the QR token outside its rendered handoff."],
        PlaybookDifficulty::Advanced,
    )?;
    add(
        builder,
        "playbook.telegram.connection-phone",
        "Connect Telegram by Phone",
        "Start and monitor Telegram phone login, submit OTP and possible 2FA, or cancel the operation.",
        "Connect one Telegram operator account through the phone, OTP, and 2FA continuation lifecycle.",
        &[ToolkitId::Telegram],
        const {
            &[
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_START.as_str(),
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS.as_str(),
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_OTP_SUBMIT.as_str(),
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_TWO_FACTOR_SUBMIT
                    .as_str(),
            ]
        },
        const { &[onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_CANCEL.as_str()] },
        &[
            step(
                "Start Telegram with method=phone, E.164 phone, and the required SOCKS5 proxy.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_START
                            .as_str(),
                    ]
                },
            ),
            step(
                "Poll status only until the projected operation expiry.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS
                            .as_str(),
                    ]
                },
            ),
            step(
                "Submit the OTP and, only if requested, the two-factor password through their transient fields.",
                const {
                    &[
                    onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_OTP_SUBMIT
                        .as_str(),
                    onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_TWO_FACTOR_SUBMIT
                        .as_str(),
                ]
                },
            ),
            step(
                "Cancel the same operation when the attended flow will not continue.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_CANCEL
                            .as_str(),
                    ]
                },
            ),
        ],
        &["Never log, persist, or reuse OTP and two-factor values."],
        PlaybookDifficulty::Advanced,
    )?;
    add(
        builder,
        "playbook.youtube.credential-lifecycle",
        "Manage a YouTube Credential",
        "Connect, inspect, rename, rotate, or delete one YouTube API credential through its typed lifecycle.",
        "Manage one YouTube service credential while retaining its opaque credential ID.",
        &[ToolkitId::Youtube],
        const {
            &[
                onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_CONNECT.as_str(),
                onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_RENAME.as_str(),
                onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_ROTATE.as_str(),
                onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_DELETE.as_str(),
            ]
        },
        const { &[onecrew_local_runtime::OperationId::YOUTUBE_CONNECTION_GET.as_str()] },
        &[
            step(
                "Connect the API key and retain the returned credential ID.",
                const { &[onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_CONNECT.as_str()] },
            ),
            step(
                "Inspect that exact credential before choosing a mutation.",
                const { &[onecrew_local_runtime::OperationId::YOUTUBE_CONNECTION_GET.as_str()] },
            ),
            step(
                "Rename or rotate the same credential without translating its ID.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_RENAME.as_str(),
                        onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_ROTATE.as_str(),
                    ]
                },
            ),
            step(
                "Delete only the confirmed selected credential.",
                const { &[onecrew_local_runtime::OperationId::YOUTUBE_CREDENTIAL_DELETE.as_str()] },
            ),
        ],
        &[
            "A successful local connection does not imply unlimited YouTube quota or provider entitlement.",
        ],
        PlaybookDifficulty::Intermediate,
    )?;
    Ok(())
}
