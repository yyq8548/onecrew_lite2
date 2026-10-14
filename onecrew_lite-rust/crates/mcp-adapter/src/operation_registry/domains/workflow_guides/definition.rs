//! Scenario authors define content here without routing or serialization code.
//! To add a scenario, create its module under a category and add its DEFINITION
//! to that category's GROUP. Directory, lookup and the topic schema follow that
//! registration automatically; no operation handler or topic match is edited.
#[derive(Clone, Copy, Debug)]
pub(super) struct GuideDefinition {
    pub(super) topic: &'static str,
    pub(super) operation_ids: &'static [onecrew_local_runtime::OperationId],
    pub(super) title: &'static str,
    pub(super) questions: &'static [&'static str],
    pub(super) workflow: &'static str,
    pub(super) queries: &'static [&'static str],
    pub(super) expected: &'static str,
    pub(super) example: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct GuideGroup {
    pub(super) category: &'static str,
    pub(super) title: &'static str,
    pub(super) definitions: &'static [GuideDefinition],
}
