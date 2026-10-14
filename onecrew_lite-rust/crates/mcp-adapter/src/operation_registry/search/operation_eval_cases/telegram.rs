use super::OperationEvalCase;
use onecrew_local_runtime::ToolkitId;

pub(super) const CASES: &[OperationEvalCase] = &[
    OperationEvalCase {
        query: "Read Telegram community metadata by public username",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.community.get",
    },
    OperationEvalCase {
        query: "Read current Telegram community administrators and their roles",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.community.administrators.list",
    },
    OperationEvalCase {
        query: "Telegram community search",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.communities.search",
    },
    OperationEvalCase {
        query: "Telegram self-check connection network reachability",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.check",
    },
    OperationEvalCase {
        query: "Telegram disconnect operator account",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.disconnect",
    },
    OperationEvalCase {
        query: "Telegram read connection status",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.get",
    },
    OperationEvalCase {
        query: "Telegram cancel login",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.login.cancel",
    },
    OperationEvalCase {
        query: "Telegram submit login OTP",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.login.otp.submit",
    },
    OperationEvalCase {
        query: "Telegram start login",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.login.start",
    },
    OperationEvalCase {
        query: "Telegram read login status",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.login.status",
    },
    OperationEvalCase {
        query: "Telegram submit login two-factor password",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.login.two_factor.submit",
    },
    OperationEvalCase {
        query: "Telegram operator reconnect begin",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.reconnect.begin",
    },
    OperationEvalCase {
        query: "Telegram operator reconnect cancel",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.reconnect.cancel",
    },
    OperationEvalCase {
        query: "Telegram operator reconnect finalize",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.reconnect.finalize",
    },
    OperationEvalCase {
        query: "Telegram prepare operator reconnect",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.reconnect.prepare",
    },
    OperationEvalCase {
        query: "Telegram operator reconnect status",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.connection.reconnect.status",
    },
    OperationEvalCase {
        query: "Telegram sync one conversation thread into the local store",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.conversation.sync",
    },
    OperationEvalCase {
        query: "Telegram send a direct message reply inside an open conversation",
        toolkit: ToolkitId::Telegram,
        expected_id: "telegram.message.send",
    },
];
