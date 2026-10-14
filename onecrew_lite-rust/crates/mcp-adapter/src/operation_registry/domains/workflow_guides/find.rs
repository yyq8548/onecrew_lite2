mod admins;
mod batch;
mod one;
mod replies;

use super::definition::GuideGroup;

pub(super) const GROUP: GuideGroup = GuideGroup {
    category: "find",
    title: "Find creators",
    definitions: &[
        batch::DEFINITION,
        one::DEFINITION,
        replies::DEFINITION,
        admins::DEFINITION,
    ],
};
