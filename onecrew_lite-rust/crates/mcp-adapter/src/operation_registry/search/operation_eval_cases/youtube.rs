use super::OperationEvalCase;
use onecrew_local_runtime::ToolkitId;

pub(super) const CASES: &[OperationEvalCase] = &[
    OperationEvalCase {
        query: "YouTube channel by ID",
        toolkit: ToolkitId::Youtube,
        expected_id: "youtube.channel.get",
    },
    OperationEvalCase {
        query: "YouTube channel search",
        toolkit: ToolkitId::Youtube,
        expected_id: "youtube.channels.search",
    },
    OperationEvalCase {
        query: "YouTube self-check connection network reachability",
        toolkit: ToolkitId::Youtube,
        expected_id: "youtube.connection.check",
    },
    OperationEvalCase {
        query: "YouTube get connection detail",
        toolkit: ToolkitId::Youtube,
        expected_id: "youtube.connection.get",
    },
    OperationEvalCase {
        query: "YouTube connect credential",
        toolkit: ToolkitId::Youtube,
        expected_id: "youtube.credential.connect",
    },
    OperationEvalCase {
        query: "YouTube delete credential",
        toolkit: ToolkitId::Youtube,
        expected_id: "youtube.credential.delete",
    },
    OperationEvalCase {
        query: "YouTube rename credential",
        toolkit: ToolkitId::Youtube,
        expected_id: "youtube.credential.rename",
    },
    OperationEvalCase {
        query: "YouTube rotate credential",
        toolkit: ToolkitId::Youtube,
        expected_id: "youtube.credential.rotate",
    },
    OperationEvalCase {
        query: "YouTube video search",
        toolkit: ToolkitId::Youtube,
        expected_id: "youtube.videos.search",
    },
];
