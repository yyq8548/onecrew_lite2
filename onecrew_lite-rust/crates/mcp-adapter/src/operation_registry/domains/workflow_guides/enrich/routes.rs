use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "enrich-routes",
    title: "Add contact routes",
    questions: &["For which people should which contact routes be found?"],
    workflow: "Verify the specified accounts' platform identifiers, connections, and real sources, then submit directly under the current candidate-save contract; during prepare, obtain current profile and link-page data, and show the returned result_ref for the user to confirm identity, rows, and contact routes. The App confirmation submits the immutable data already shown; read back Creator after completion. The prepare stage does not write Creator and does not need a separate candidate cache.",
    queries: &[
        "creator candidate save selected locator",
        "candidate save prepare contact routes review",
    ],
    expected: "Contact routes, sources, and save readback results after confirmation",
    example: "For [name, or \"everyone missing contact routes\"], review each person's platform profile, bio, and public links; list the email addresses or social accounts found and their sources, then wait for my confirmation before saving.",
};
