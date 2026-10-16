use super::OperationEvalCase;
use onecrew_local_runtime::ToolkitId;

pub(super) const CASES: &[OperationEvalCase] = &[
    OperationEvalCase {
        query: "X cancel browser connection",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.browser.cancel",
    },
    OperationEvalCase {
        query: "X complete browser connection",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.browser.complete",
    },
    OperationEvalCase {
        query: "X start browser connection",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.browser.start",
    },
    OperationEvalCase {
        query: "X read browser connection status",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.browser.status",
    },
    OperationEvalCase {
        query: "X self-check connection network reachability",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.check",
    },
    OperationEvalCase {
        query: "X disconnect operator account",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.disconnect",
    },
    OperationEvalCase {
        query: "X get connection detail",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.get",
    },
    OperationEvalCase {
        query: "X operator reconnect begin",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.reconnect.begin",
    },
    OperationEvalCase {
        query: "X operator reconnect cancel",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.reconnect.cancel",
    },
    OperationEvalCase {
        query: "X operator reconnect finalize",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.reconnect.finalize",
    },
    OperationEvalCase {
        query: "X prepare operator reconnect",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.reconnect.prepare",
    },
    OperationEvalCase {
        query: "X operator reconnect status",
        toolkit: ToolkitId::X,
        expected_id: "x.connection.reconnect.status",
    },
    OperationEvalCase {
        query: "X post thread",
        toolkit: ToolkitId::X,
        expected_id: "x.post_thread.get",
    },
    OperationEvalCase {
        query: "Advanced X post search with account language date and engagement filters",
        toolkit: ToolkitId::X,
        expected_id: "x.posts.search.advanced",
    },
    OperationEvalCase {
        query: "X profile by handle",
        toolkit: ToolkitId::X,
        expected_id: "x.profile.get",
    },
    OperationEvalCase {
        query: "X list user posts",
        toolkit: ToolkitId::X,
        expected_id: "x.user_posts.list",
    },
];
