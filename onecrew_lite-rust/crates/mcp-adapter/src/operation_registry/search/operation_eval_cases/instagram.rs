use super::OperationEvalCase;
use onecrew_local_runtime::ToolkitId;

pub(super) const CASES: &[OperationEvalCase] = &[
    OperationEvalCase {
        query: "Instagram cancel browser connection",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.browser.cancel",
    },
    OperationEvalCase {
        query: "Instagram complete browser connection",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.browser.complete",
    },
    OperationEvalCase {
        query: "Instagram start browser connection",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.browser.start",
    },
    OperationEvalCase {
        query: "Instagram read browser connection status",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.browser.status",
    },
    OperationEvalCase {
        query: "Instagram self-check connection network reachability",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.check",
    },
    OperationEvalCase {
        query: "Instagram disconnect operator account",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.disconnect",
    },
    OperationEvalCase {
        query: "Instagram get connection detail",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.get",
    },
    OperationEvalCase {
        query: "Instagram operator reconnect begin",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.reconnect.begin",
    },
    OperationEvalCase {
        query: "Instagram operator reconnect cancel",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.reconnect.cancel",
    },
    OperationEvalCase {
        query: "Instagram operator reconnect finalize",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.reconnect.finalize",
    },
    OperationEvalCase {
        query: "Instagram prepare operator reconnect",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.reconnect.prepare",
    },
    OperationEvalCase {
        query: "Instagram operator reconnect status",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.connection.reconnect.status",
    },
    OperationEvalCase {
        query: "Instagram profile by handle",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.profile.get",
    },
    OperationEvalCase {
        query: "Instagram user search",
        toolkit: ToolkitId::Instagram,
        expected_id: "instagram.users.search",
    },
];
