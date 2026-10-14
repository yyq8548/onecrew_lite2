use super::super::definition::GuideDefinition;

pub(super) const DEFINITION: GuideDefinition = GuideDefinition {
    operation_ids: &[],
    topic: "find-one",
    title: "Find one account",
    questions: &[
        "What are the account's platform and username or profile link? Is this add_new or save_or_update?",
    ],
    workflow: "First use current discovery and onecrew_tool_get to obtain the exact contract; if external search is needed, run the same conditions under the user's confirmed search plan. Preserve the selected hit's stable locator, connection, source, and real provenance. Read profile, channel, or recent content only when quality or geographic evidence requested by the user is missing; when search evidence is sufficient, do not pre-read anything extra for a later save. Merge duplicate YouTube hits by stable channel ID only within the same connection and read depth; when channel verification is needed, pass depth: light or full explicitly, with light intentionally omitting comments and full requesting comments where supported; never merge by display name or similar handle. Submit one intent per candidate under the current save contract (add_new or save_or_update); the owner still obtains fresh platform data during prepare, and its returned status and reasons are what you present—do not accept Agent profiles or caches. An add_new existing_skipped result is not a new addition; show only result_ref before confirmation and read back the real result after confirmation.",
    queries: &[
        "public profile recent posts",
        "creator candidate save review",
    ],
    expected: "The specified account's profile, scoring rationale, and public sources",
    example: "Review [username or profile link] on [platform]. Check the necessary public evidence against the user's criteria and organize the score, rationale, and sources; prepare a save with the explicit [add_new/save_or_update] intent, show me the owner's returned result first, and wait for my confirmation before adding or updating the list.",
};
