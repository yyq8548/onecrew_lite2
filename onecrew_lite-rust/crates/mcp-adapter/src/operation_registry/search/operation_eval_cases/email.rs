use super::OperationEvalCase;
use onecrew_local_runtime::ToolkitId;

pub(super) const CASES: &[OperationEvalCase] = &[
    OperationEvalCase {
        query: "Email self-check mailbox connection network reachability",
        toolkit: ToolkitId::Email,
        expected_id: "email.connection.check",
    },
    OperationEvalCase {
        query: "Email get mail draft",
        toolkit: ToolkitId::Email,
        expected_id: "email.draft.get",
    },
    OperationEvalCase {
        query: "Email list mail drafts",
        toolkit: ToolkitId::Email,
        expected_id: "email.draft.list",
    },
    OperationEvalCase {
        query: "Email list mail folders",
        toolkit: ToolkitId::Email,
        expected_id: "email.folder.list",
    },
    OperationEvalCase {
        query: "Email get mailbox",
        toolkit: ToolkitId::Email,
        expected_id: "email.mailbox.get",
    },
    OperationEvalCase {
        query: "Email sync mailbox",
        toolkit: ToolkitId::Email,
        expected_id: "email.mailbox.sync",
    },
    OperationEvalCase {
        query: "Email get mailbox sync status",
        toolkit: ToolkitId::Email,
        expected_id: "email.mailbox.sync_status",
    },
    OperationEvalCase {
        query: "Email get mail message",
        toolkit: ToolkitId::Email,
        expected_id: "email.message.get",
    },
    OperationEvalCase {
        query: "Email get mail template",
        toolkit: ToolkitId::Email,
        expected_id: "email.template.get",
    },
    OperationEvalCase {
        query: "Email list mail templates",
        toolkit: ToolkitId::Email,
        expected_id: "email.template.list",
    },
    OperationEvalCase {
        query: "Email get mail thread",
        toolkit: ToolkitId::Email,
        expected_id: "email.thread.get",
    },
    OperationEvalCase {
        query: "Email list mail threads",
        toolkit: ToolkitId::Email,
        expected_id: "email.thread.list",
    },
    OperationEvalCase {
        query: "Email update mail thread disposition",
        toolkit: ToolkitId::Email,
        expected_id: "email.thread.update_disposition",
    },
];
