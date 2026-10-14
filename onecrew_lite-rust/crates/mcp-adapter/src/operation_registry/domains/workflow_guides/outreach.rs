mod batch;
mod followup;
mod send;
mod sync;

use super::definition::GuideGroup;

pub(super) const GROUP: GuideGroup = GuideGroup {
    category: "outreach",
    title: "Outreach",
    definitions: &[
        send::DEFINITION,
        batch::DEFINITION,
        followup::DEFINITION,
        sync::DEFINITION,
    ],
};
