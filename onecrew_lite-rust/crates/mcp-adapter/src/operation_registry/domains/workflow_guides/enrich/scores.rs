use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "enrich-scores",
    title: "Add scores",
    questions: &[
        "Which existing candidates should be assessed, and what matching criteria and evidence window did the user specify?",
    ],
    workflow: "Process content or account leads under the user's original request and confirmed interpretation; classify matches only as matched, unmatched, or unverified, and unknown does not mean unmatched. Group results by the stable platform identity returned by each platform and preserve sources; do not merge same-named accounts across platforms. Reuse equivalent requests with the same platform, connection, operation/schema revision, native arguments, depth, and cursor; do not resend because of call_id, display name, shape, or presentation reference. Start with existing evidence; make one targeted supplementary read only for a clear gap that could change the current decision, and mark unknown and stop when the same gap remains unverifiable. Keep scoring separate from matching: first read the single shared four-tier rubric from the find-batch guide, choose the tier there, then fill the fixed score without rewriting the standard; each per-candidate assessment must state its own concrete facts, relation to the requirement, and actual limitations, do not re-verify candidates in the same tier. Do not add follower, geographic, short-form-content, originality, or commercial-collaboration gates. Stop searching after reaching the matching target; when more pages are needed, use the actual cursor returned and do not fetch the first page again. Save and read back under the live update contract.",
    queries: &[
        "OneCrew creators missing scores",
        "public profile recent content creator assessment update",
    ],
    expected: "Scores, rationale, evidence, and save readback results",
    example: "Find the people in the list who need assessment. Use their platforms' existing public content and necessary evidence to choose the excellent, good, medium, or low tier, then fill in the corresponding score and per-candidate rationale; keep unverifiable items unknown.",
};
