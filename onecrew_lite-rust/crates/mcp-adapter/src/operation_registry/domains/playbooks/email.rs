use crate::operation_registry::descriptor::PlaybookDifficulty;
use crate::operation_registry::domains::playbooks::descriptor::{add, step};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};
use onecrew_local_runtime::ToolkitId;

pub(super) fn register(builder: &mut RegistryBuilder) -> Result<(), RegistryBuildError> {
    add(
        builder,
        "playbook.email.draft-and-send",
        "Draft and Send Email",
        "Save a draft, send that exact revision with confirmation, and verify the thread.",
        "Create and send one canonical email draft with provider readback.",
        &[ToolkitId::Email],
        const {
            &[
                onecrew_local_runtime::OperationId::EMAIL_DRAFT_SAVE.as_str(),
                onecrew_local_runtime::OperationId::EMAIL_DRAFT_SEND.as_str(),
            ]
        },
        const {
            &[
                onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str(),
                onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str(),
            ]
        },
        &[
            step(
                "Save and inspect the draft revision.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::EMAIL_DRAFT_SAVE.as_str(),
                        onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str(),
                    ]
                },
            ),
            step(
                "Send the confirmed revision and verify its thread.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::EMAIL_DRAFT_SEND.as_str(),
                        onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str(),
                    ]
                },
            ),
        ],
        &["A saved draft is not evidence that the provider sent it."],
        PlaybookDifficulty::Advanced,
    )?;
    add(
        builder,
        "playbook.email.inbox-triage",
        "Triage Email Inbox",
        "Sync a mailbox, inspect threads, and record one actionable Creator triage verdict.",
        "Sync and triage new inbound email.",
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
            ]
        },
        &[
            step(
                "Sync the mailbox and list current threads.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::EMAIL_MAILBOX_SYNC.as_str(),
                        onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str(),
                    ]
                },
            ),
            step(
                "Record and verify one triage verdict.",
                const {
                    &[
                        onecrew_local_runtime::OperationId::ONECREW_TRIAGE_RECORD.as_str(),
                        onecrew_local_runtime::OperationId::ONECREW_TRIAGE_LIST.as_str(),
                    ]
                },
            ),
        ],
        &["Record one verdict per inbound reply rather than per sync run."],
        PlaybookDifficulty::Advanced,
    )?;
    Ok(())
}
