use super::OperationEvalCase;
use onecrew_local_runtime::ToolkitId;

pub(super) const CASES: &[OperationEvalCase] = &[
    OperationEvalCase {
        query: "Email self-check mailbox connection network reachability",
        toolkit: ToolkitId::Email,
        expected_id: "email.connection.check",
    },
    OperationEvalCase {
        query: "Email prepare direct email",
        toolkit: ToolkitId::Email,
        expected_id: "email.direct.prepare",
    },
    OperationEvalCase {
        query: "Email send direct email",
        toolkit: ToolkitId::Email,
        expected_id: "email.direct.send",
    },
    OperationEvalCase {
        query: "Email send many emails",
        toolkit: ToolkitId::Email,
        expected_id: "email.send_many",
    },
    OperationEvalCase {
        query: "Email reconcile accepted batch",
        toolkit: ToolkitId::Email,
        expected_id: "email.send_many.reconcile",
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
        query: "Email restore mail draft",
        toolkit: ToolkitId::Email,
        expected_id: "email.draft.restore",
    },
    OperationEvalCase {
        query: "Email save mail draft",
        toolkit: ToolkitId::Email,
        expected_id: "email.draft.save",
    },
    OperationEvalCase {
        query: "Email send mail draft",
        toolkit: ToolkitId::Email,
        expected_id: "email.draft.send",
    },
    OperationEvalCase {
        query: "Email trash mail draft",
        toolkit: ToolkitId::Email,
        expected_id: "email.draft.trash",
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
        query: "Email prepare manual email",
        toolkit: ToolkitId::Email,
        expected_id: "email.manual.prepare",
    },
    OperationEvalCase {
        query: "Email send manual email",
        toolkit: ToolkitId::Email,
        expected_id: "email.manual.send",
    },
    OperationEvalCase {
        query: "Email get mail message",
        toolkit: ToolkitId::Email,
        expected_id: "email.message.get",
    },
    OperationEvalCase {
        query: "Email delete mail template",
        toolkit: ToolkitId::Email,
        expected_id: "email.template.delete",
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
        query: "Email render mail template",
        toolkit: ToolkitId::Email,
        expected_id: "email.template.render",
    },
    OperationEvalCase {
        query: "Email save mail template",
        toolkit: ToolkitId::Email,
        expected_id: "email.template.save",
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
