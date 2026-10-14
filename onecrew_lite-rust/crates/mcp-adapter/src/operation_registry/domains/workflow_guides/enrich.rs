mod followers;
mod one;
mod routes;
mod scores;

use super::definition::GuideGroup;

pub(super) const GROUP: GuideGroup = GuideGroup {
    category: "enrich",
    title: "Enrich creator data",
    definitions: &[
        followers::DEFINITION,
        scores::DEFINITION,
        routes::DEFINITION,
        one::DEFINITION,
    ],
};
