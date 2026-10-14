use super::OperationEvalCase;
use onecrew_local_runtime::ToolkitId;

pub(super) const CASES: &[OperationEvalCase] = &[
    OperationEvalCase {
        query: "read saved identity comparison material",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.identity.material.get",
    },
    OperationEvalCase {
        query: "read campaign author groups",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.identity.groups.get",
    },
    OperationEvalCase {
        query: "read saved identity review",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.identity.review.get",
    },
    OperationEvalCase {
        query: "submit source-bound identity review",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.identity.review.submit",
    },
    OperationEvalCase {
        query: "revoke an identity review",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.identity.review.revoke",
    },
    OperationEvalCase {
        query: "increase discovery Campaign target preserving criteria sources and read budget",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.target.increase",
    },
    OperationEvalCase {
        query: "list live registered OpenCLI creator discovery sources and browser profiles",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.sources.get",
    },
    OperationEvalCase {
        query: "validate a one-sentence discovery Campaign proposal against the customer's exact hard requirements",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.intent.validate",
    },
    OperationEvalCase {
        query: "Read a saved Campaign review receipt by request ID",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.review.get",
    },
    OperationEvalCase {
        query: "Submit a source-bound Campaign review with cited findings",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.review.submit",
    },
    OperationEvalCase {
        query: "Read source-bound Campaign review material",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.review_material.get",
    },
    OperationEvalCase {
        query: "Read saved Campaign post-author identity",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.identity.get",
    },
    OperationEvalCase {
        query: "Read saved Campaign author profile",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.profile.get",
    },
    OperationEvalCase {
        query: "Read saved Campaign Bilibili video evidence",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.content.get",
    },
    OperationEvalCase {
        query: "Read saved Campaign transcript",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.transcript.get",
    },
    OperationEvalCase {
        query: "Read saved Campaign public-page evidence",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.public_page.get",
    },
    OperationEvalCase {
        query: "advance approved discovery Campaign automatically from checkpoint",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.advance",
    },
    OperationEvalCase {
        query: "create discovery Campaign draft",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.campaign.create",
    },
    OperationEvalCase {
        query: "approve discovery Campaign criteria and budget",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.campaign.approve",
    },
    OperationEvalCase {
        query: "run approved discovery Bilibili or X creator search once",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.search_once",
    },
    OperationEvalCase {
        query: "run approved discovery YouTube search once",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.youtube.search_once",
    },
    OperationEvalCase {
        query: "read one saved discovery YouTube hit video for candidate verification",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.youtube.verify_hit_once",
    },
    OperationEvalCase {
        query: "get discovery Campaign frozen criteria and checkpoints",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.campaign.get",
    },
    OperationEvalCase {
        query: "list discovery Campaign candidates and evidence",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.candidates.list",
    },
    OperationEvalCase {
        query: "count qualified pending and rejected discovery Campaign candidates",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.candidates.count",
    },
    OperationEvalCase {
        query: "list discovery Campaign search hits",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.discovery.hits.list",
    },
    OperationEvalCase {
        query: "workflow guidance",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.workflow.guide",
    },
    OperationEvalCase {
        query: "Prepare editable search plan with acceptance criteria and keywords for user confirmation before external creator search",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.search.plan.prepare",
    },
    OperationEvalCase {
        query: "archive creator",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.archive",
    },
    OperationEvalCase {
        query: "save creator candidate",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.candidate.save",
    },
    OperationEvalCase {
        query: "Save several creator candidates in one batch",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.candidate.batch.save",
    },
    OperationEvalCase {
        query: "add creator contact route",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.contact_route.add",
    },
    OperationEvalCase {
        query: "confirm creator contact route deletion",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.contact_route.delete.confirm",
    },
    OperationEvalCase {
        query: "prepare creator contact route deletion",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.contact_route.delete.prepare",
    },
    OperationEvalCase {
        query: "update creator contact route",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.contact_route.update",
    },
    OperationEvalCase {
        query: "prepare creator content reply",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.content_reply.prepare",
    },
    OperationEvalCase {
        query: "send creator content reply",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.content_reply.send",
    },
    OperationEvalCase {
        query: "delete creator permanently",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.delete",
    },
    OperationEvalCase {
        query: "export creators",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.export",
    },
    OperationEvalCase {
        query: "get creator",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.get",
    },
    OperationEvalCase {
        query: "resolve creator identity",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.identity.resolve",
    },
    OperationEvalCase {
        query: "extract links and emails from a link hub page",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.link_hub.extract",
    },
    OperationEvalCase {
        query: "list creators",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.list",
    },
    OperationEvalCase {
        query: "list creator folders",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.group.list",
    },
    OperationEvalCase {
        query: "create creator folder",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.group.create",
    },
    OperationEvalCase {
        query: "rename creator folder",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.group.update",
    },
    OperationEvalCase {
        query: "delete creator folder",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.group.delete",
    },
    OperationEvalCase {
        query: "move one creator to folder",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.group.set",
    },
    OperationEvalCase {
        query: "bulk move creators to folder",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.group.bulk_move",
    },
    OperationEvalCase {
        query: "create manual creator",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.manual.create",
    },
    OperationEvalCase {
        query: "create Creator batch",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.batch.create",
    },
    OperationEvalCase {
        query: "update creator stage",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.stage.update",
    },
    OperationEvalCase {
        query: "update creator",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.creator.update",
    },
    OperationEvalCase {
        query: "get outreach action status",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.action.status",
    },
    OperationEvalCase {
        query: "get contact route last outbound account",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.contact_route.last_contact_account",
    },
    OperationEvalCase {
        query: "resolve outreach contact target",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.contact_target.resolve",
    },
    OperationEvalCase {
        query: "get local outreach conversation",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.conversation.get",
    },
    OperationEvalCase {
        query: "list local outreach conversations",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.conversation.list",
    },
    OperationEvalCase {
        query: "prepare outreach message",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.message.prepare",
    },
    OperationEvalCase {
        query: "send outreach message",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.message.send",
    },
    OperationEvalCase {
        query: "get outreach relationship",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.relationship.get",
    },
    OperationEvalCase {
        query: "prepare outreach relationship action",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.relationship.prepare",
    },
    OperationEvalCase {
        query: "update outreach relationship",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.outreach.relationship.update",
    },
    OperationEvalCase {
        query: "acknowledge reply triage",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.triage.acknowledge",
    },
    OperationEvalCase {
        query: "list reply triage",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.triage.list",
    },
    OperationEvalCase {
        query: "record reply triage",
        toolkit: ToolkitId::OneCrew,
        expected_id: "onecrew.triage.record",
    },
];
