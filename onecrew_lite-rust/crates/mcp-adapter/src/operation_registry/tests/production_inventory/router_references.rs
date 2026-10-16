//! Drift check between the executable registry and the non-Rust texts that
//! name its contracts: the cached Codex Router and plugin manifest, the MCP
//! App fixtures and preview exports, and the web runtime-change scope union.
//! `include_str!` makes every input a compile-time dependency of this test.

use super::*;

use onecrew_creator_runtime::CreatorChangeScope;
use onecrew_local_runtime::PlatformChangeScope;

use crate::{
    operation_registry::{
        OperationRegistry,
        v2::{RETIRED_PLAN_PROPERTIES, RETIRED_PLAN_WIRE_NAMES, RETIRED_SEARCH_OPERATIONS},
    },
    server::{McpAdapterServer, McpSurface, surface_instructions},
};

#[derive(Clone, Copy)]
struct ReferenceFile {
    path: &'static str,
    text: &'static str,
}

/// Includes a repository-relative file; this module sits six directories
/// below the repository root.
macro_rules! repo_file {
    ($path:literal) => {
        ReferenceFile {
            path: $path,
            text: include_str!(concat!("../../../../../../", $path)),
        }
    };
}

const ROUTER_SKILL: ReferenceFile =
    repo_file!("integrations/codex-marketplace/plugins/onecrew/skills/onecrew/SKILL.md");
const ROUTER_REFERENCES: [ReferenceFile; 3] = [
    repo_file!(
        "integrations/codex-marketplace/plugins/onecrew/skills/onecrew/references/campaign-discovery.md"
    ),
    repo_file!(
        "integrations/codex-marketplace/plugins/onecrew/skills/onecrew/references/onecrew-writes.md"
    ),
    repo_file!(
        "integrations/codex-marketplace/plugins/onecrew/skills/onecrew/references/toolkit-connections.md"
    ),
];
const PLUGIN_MANIFEST: ReferenceFile =
    repo_file!("integrations/codex-marketplace/plugins/onecrew/.codex-plugin/plugin.json");
const MCP_UI_TEST_SUPPORT_DIRECTORY: &str = "apps/mcp-ui/src/test-support";
const MCP_UI_TEST_SUPPORT: [ReferenceFile; 18] = [
    repo_file!("apps/mcp-ui/src/test-support/app-mock.ts"),
    repo_file!("apps/mcp-ui/src/test-support/choice-five-decisions.json"),
    repo_file!("apps/mcp-ui/src/test-support/choice-result.ts"),
    repo_file!("apps/mcp-ui/src/test-support/creator-batch-fixture.ts"),
    repo_file!("apps/mcp-ui/src/test-support/creator-save-fixture.ts"),
    repo_file!("apps/mcp-ui/src/test-support/creator-save-legacy-wire.json"),
    repo_file!("apps/mcp-ui/src/test-support/demo-locale.ts"),
    repo_file!("apps/mcp-ui/src/test-support/design-scenarios.ts"),
    repo_file!("apps/mcp-ui/src/test-support/dm-failure-fixture.ts"),
    repo_file!("apps/mcp-ui/src/test-support/email-fixture.ts"),
    repo_file!("apps/mcp-ui/src/test-support/fixtures.ts"),
    repo_file!("apps/mcp-ui/src/test-support/flow-simulator.test.ts"),
    repo_file!("apps/mcp-ui/src/test-support/flow-simulator.ts"),
    repo_file!("apps/mcp-ui/src/test-support/round3.ts"),
    repo_file!("apps/mcp-ui/src/test-support/round4.ts"),
    repo_file!("apps/mcp-ui/src/test-support/search-plan-fixture.ts"),
    repo_file!("apps/mcp-ui/src/test-support/search-plan-read-current.json"),
    repo_file!("apps/mcp-ui/src/test-support/search-plan-wire.json"),
];
const MCP_UI_PREVIEW_DIRECTORY: &str = "apps/mcp-ui/preview";
const MCP_UI_PREVIEWS: [ReferenceFile; 4] = [
    repo_file!("apps/mcp-ui/preview/15-flow-cases.json"),
    repo_file!("apps/mcp-ui/preview/20-scenarios.json"),
    repo_file!("apps/mcp-ui/preview/round-3-cases.json"),
    repo_file!("apps/mcp-ui/preview/round-4-cases.json"),
];
const WEB_RUNTIME_CHANGE: ReferenceFile = repo_file!("apps/web/src/lib/runtime-change.ts");
/// `onecrew-local-service` lists its bootstrap tools ahead of this crate's
/// business surface on the plugin's stdio connection. It depends on this
/// crate, so the test reads its source instead of linking it.
const LOCAL_SERVICE_MCP: ReferenceFile = repo_file!("crates/local-service/src/mcp.rs");

/// Last segments that mark a file name or a host rather than a registry ID.
const NON_ID_SUFFIXES: [&str; 22] = [
    "json", "md", "ts", "tsx", "js", "mjs", "cjs", "html", "css", "svg", "png", "rs", "toml",
    "yaml", "yml", "db", "txt", "com", "io", "org", "net", "ai",
];

fn router_files() -> impl Iterator<Item = ReferenceFile> {
    std::iter::once(ROUTER_SKILL).chain(ROUTER_REFERENCES)
}

fn reference_inputs() -> impl Iterator<Item = ReferenceFile> {
    router_files()
        .chain([PLUGIN_MANIFEST])
        .chain(MCP_UI_TEST_SUPPORT)
        .chain(MCP_UI_PREVIEWS)
        .chain([WEB_RUNTIME_CHANGE])
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Bytes that make a following dotted run part of a larger token: URL hosts
/// and paths, e-mail domains, member chains, and identifiers.
fn joins_preceding_token(byte: u8) -> bool {
    is_identifier_byte(byte) || b".-@/:$\\".contains(&byte)
}

fn is_id_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
}

fn live_registry() -> OperationRegistry {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap()
}

fn registered_entry_ids(registry: &OperationRegistry) -> Vec<EntryId> {
    let mut ids = registry
        .operation_ids_and_surfaces()
        .into_iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    ids.extend(registry.playbook_ids());
    ids.extend(registry.toolkit_guide_ids());
    ids
}

/// First segments that make a dotted literal a registry reference: the
/// toolkit of every registered entry plus the playbook and guide namespaces.
fn registry_prefixes(registry: &OperationRegistry) -> BTreeSet<String> {
    let mut prefixes = BTreeSet::new();
    for id in registered_entry_ids(registry) {
        let entry = McpSurface::ALL
            .into_iter()
            .find_map(|surface| registry.entry(&id, surface.registry_id()))
            .unwrap_or_else(|| panic!("registered entry {id} is on no surface"));
        prefixes.insert(entry.metadata().toolkit.as_str().to_owned());
    }
    for id in registry
        .playbook_ids()
        .into_iter()
        .chain(registry.toolkit_guide_ids())
    {
        let (namespace, _) = id
            .as_str()
            .split_once('.')
            .unwrap_or_else(|| panic!("registry ID {id} has no namespace"));
        prefixes.insert(namespace.to_owned());
    }
    prefixes
}

/// Dotted lowercase literals (`onecrew.creator.get`,
/// `playbook.x.creator-discovery`) whose first segment is a registry prefix.
/// URL hosts and paths, e-mail domains, member chains, calls, file names,
/// version numbers, and abbreviations such as `e.g.` are not literals.
fn operation_id_literals<'a>(line: &'a str, prefixes: &BTreeSet<String>) -> Vec<&'a str> {
    let bytes = line.as_bytes();
    let mut literals = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let joined_before = index
            .checked_sub(1)
            .is_some_and(|previous| joins_preceding_token(bytes[previous]));
        if !bytes[index].is_ascii_lowercase() || joined_before {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_id_byte(bytes[index]) {
            index += 1;
        }
        // Sentence punctuation after an ID is not part of it.
        let end = start + line[start..index].trim_end_matches(['.', '-']).len();
        let literal = &line[start..end];
        let joined_after = bytes
            .get(end)
            .is_some_and(|next| is_identifier_byte(*next) || b"(/".contains(next));
        let Some((first, rest)) = literal.split_once('.') else {
            continue;
        };
        let file_or_host = literal
            .rsplit('.')
            .next()
            .is_some_and(|last| NON_ID_SUFFIXES.contains(&last));
        if !joined_after
            && !file_or_host
            && prefixes.contains(first)
            && !rest.split('.').any(str::is_empty)
        {
            literals.push(literal);
        }
    }
    literals
}

fn skip_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    index
}

/// String-aware match for the `{` at `open` in JSON or TypeScript source.
fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    let mut quote = None;
    let mut escaped = false;
    for (offset, byte) in bytes[open..].iter().copied().enumerate() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == delimiter {
                quote = None;
            }
            continue;
        }
        match byte {
            b'"' | b'\'' | b'`' => quote = Some(byte),
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Blanks every operation-owned `arguments` object literal while keeping
/// line breaks, so line numbers still match the source file.
fn without_operation_arguments(file: ReferenceFile) -> String {
    let text = file.text;
    let bytes = text.as_bytes();
    let mut blanked = bytes.to_vec();
    let mut cursor = 0;
    while let Some(found) = text[cursor..].find("arguments") {
        let start = cursor + found;
        cursor = start + "arguments".len();
        let bounded = !start
            .checked_sub(1)
            .is_some_and(|previous| is_identifier_byte(bytes[previous]))
            && !bytes.get(cursor).copied().is_some_and(is_identifier_byte);
        if !bounded {
            continue;
        }
        let mut index = cursor;
        if matches!(bytes.get(index), Some(b'"' | b'\'')) {
            index += 1;
        }
        index = skip_whitespace(bytes, index);
        if bytes.get(index) != Some(&b':') {
            continue;
        }
        index = skip_whitespace(bytes, index + 1);
        if bytes.get(index) != Some(&b'{') {
            continue;
        }
        let close = matching_brace(bytes, index)
            .unwrap_or_else(|| panic!("{} has an unbalanced `arguments` object", file.path));
        for byte in &mut blanked[index..=close] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
        cursor = close + 1;
    }
    String::from_utf8(blanked).expect("blanking replaces whole characters with ASCII spaces")
}

/// Offsets of `token` bounded by non-identifier bytes.
fn token_offsets<'a>(line: &'a str, token: &'a str) -> impl Iterator<Item = usize> + 'a {
    let bytes = line.as_bytes();
    line.match_indices(token)
        .map(|(offset, _)| offset)
        .filter(move |offset| {
            let before = offset
                .checked_sub(1)
                .is_some_and(|previous| is_identifier_byte(bytes[previous]));
            let after = bytes
                .get(offset + token.len())
                .copied()
                .is_some_and(is_identifier_byte);
            !before && !after
        })
}

#[derive(Clone, Copy)]
enum RetiredForm {
    /// Case-insensitive substring anywhere.
    Substring,
    /// Whole identifier or dotted ID.
    Token,
    /// Whole identifier used as an object key.
    Key,
}

struct RetiredToken {
    token: &'static str,
    form: RetiredForm,
    /// Retired from the plan envelope only: a scope's operation-owned
    /// `arguments` may carry a live provider field of the same name.
    plan_envelope_only: bool,
    retired_as: &'static str,
}

fn retired_tokens() -> Vec<RetiredToken> {
    let plan_properties = RETIRED_PLAN_PROPERTIES.map(|token| RetiredToken {
        token,
        form: RetiredForm::Key,
        plan_envelope_only: true,
        retired_as: "retired plan property",
    });
    let plan_wire = RETIRED_PLAN_WIRE_NAMES.map(|token| RetiredToken {
        token,
        form: RetiredForm::Token,
        plan_envelope_only: true,
        retired_as: "retired plan wire name",
    });
    let operations = RETIRED_SEARCH_OPERATIONS
        .into_iter()
        .chain([
            "onecrew.creator.discover",
            "task.choice.request",
            // Replaced by the per-toolkit `<toolkit>.conversation.sync`
            // operations, with no alias.
            "onecrew.outreach.conversation.sync",
            // The snapshot-keyed contact enrichment retired with its
            // confirmation claim; a link hub is read with
            // `onecrew.link_hub.extract` and saved through the candidate save.
            "onecrew.creator.contact.enrich",
            "onecrew.creator.link_hub.enrich",
            "onecrew.creator.contact.confirm",
            // The Creator profile refresh retired with its playbook and no
            // alias: a platform read, `onecrew.creator.update` and the
            // candidate save compose the same outcome as independent calls.
            "onecrew.creator.profile.refresh",
            "playbook.onecrew.creator-profile-refresh",
        ])
        .map(|token| RetiredToken {
            token,
            form: RetiredForm::Token,
            plan_envelope_only: false,
            retired_as: "retired operation",
        });
    let concepts = [
        ("SearchPlanDraft", RetiredForm::Token, "retired plan type"),
        // A confirmed plan sets criteria and parameters only: the page ledger,
        // its output fields, and its two refusal codes are retired.
        (
            "plan_budget",
            RetiredForm::Token,
            "retired plan ledger field",
        ),
        (
            "plan_novelty",
            RetiredForm::Token,
            "retired plan ledger field",
        ),
        (
            "search_plan_page_budget_exhausted",
            RetiredForm::Token,
            "retired plan ledger code",
        ),
        (
            "search_plan_candidate_budget_reached",
            RetiredForm::Token,
            "retired plan ledger code",
        ),
        (
            "crm.",
            RetiredForm::Substring,
            "retired CRM operation prefix",
        ),
        (
            "/mcp/domains/",
            RetiredForm::Substring,
            "retired domain-surface path",
        ),
        // The TikTok One toolkit and its cloud-authorization connection kind
        // are retired; the `tiktok` Creator platform stays live.
        (
            "tiktok_one",
            RetiredForm::Substring,
            "retired TikTok One toolkit",
        ),
        (
            "tiktok one",
            RetiredForm::Substring,
            "retired TikTok One toolkit",
        ),
        (
            "tiktok-one",
            RetiredForm::Substring,
            "retired TikTok One toolkit",
        ),
        (
            "cloud_oauth",
            RetiredForm::Substring,
            "retired cloud OAuth connection kind",
        ),
        (
            "cloud authorization",
            RetiredForm::Substring,
            "retired cloud OAuth connection kind",
        ),
    ]
    .map(|(token, form, retired_as)| RetiredToken {
        token,
        form,
        plan_envelope_only: false,
        retired_as,
    });
    plan_properties
        .into_iter()
        .chain(plan_wire)
        .chain(operations)
        .chain(concepts)
        .collect()
}

fn names_retired(line: &str, retired: &RetiredToken) -> bool {
    match retired.form {
        RetiredForm::Substring => line
            .to_ascii_lowercase()
            .contains(&retired.token.to_ascii_lowercase()),
        RetiredForm::Token => token_offsets(line, retired.token).next().is_some(),
        RetiredForm::Key => token_offsets(line, retired.token).any(|offset| {
            line[offset + retired.token.len()..]
                .trim_start_matches(['"', '\''])
                .trim_start()
                .starts_with(':')
        }),
    }
}

/// `onecrew_*` meta-tool names in text order.
fn meta_tool_tokens(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    text.match_indices("onecrew_")
        .map(|(offset, _)| offset)
        .filter(|offset| {
            !offset
                .checked_sub(1)
                .is_some_and(|previous| is_identifier_byte(bytes[previous]))
        })
        .map(|offset| {
            let length = bytes[offset..]
                .iter()
                .position(|byte| !is_identifier_byte(*byte))
                .unwrap_or(bytes.len() - offset);
            &text[offset..offset + length]
        })
        // The product name shares the tool prefix but is not a tool identifier.
        .filter(|token| *token != "onecrew_lite")
        .collect()
}

fn first_occurrences<'a>(tokens: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = BTreeSet::new();
    tokens
        .into_iter()
        .filter(|token| seen.insert(*token))
        .collect()
}

fn model_visible(tool: &rmcp::model::Tool) -> bool {
    tool.meta
        .as_ref()
        .and_then(|meta| meta.0.get("ui"))
        .and_then(|ui| ui.get("visibility"))
        .and_then(Value::as_array)
        .is_none_or(|visibility| visibility.iter().any(|audience| audience == "model"))
}

/// Live full-surface meta tools split into model-visible and App-only names.
fn live_meta_tools() -> (BTreeSet<String>, BTreeSet<String>) {
    let (visible, app_only): (Vec<_>, Vec<_>) =
        McpAdapterServer::surface_tool_router(McpSurface::Full)
            .list_all()
            .into_iter()
            .partition(model_visible);
    let names = |tools: Vec<rmcp::model::Tool>| {
        tools
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect::<BTreeSet<_>>()
    };
    (names(visible), names(app_only))
}

/// Meta tools in the order the live full-surface instructions introduce them.
fn surface_tool_order() -> Vec<String> {
    first_occurrences(meta_tool_tokens(&surface_instructions(McpSurface::Full)))
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// Bootstrap tool names in `onecrew-local-service` `login_tools()` order.
fn bootstrap_tools() -> Vec<&'static str> {
    let source = LOCAL_SERVICE_MCP.text;
    let start = source
        .find("fn login_tools()")
        .unwrap_or_else(|| panic!("{} no longer defines login_tools()", LOCAL_SERVICE_MCP.path));
    let body = &source[start..];
    let body = &body[..body
        .find("\n}\n")
        .or_else(|| body.find("\r\n}\r\n"))
        .unwrap_or_else(|| panic!("{} login_tools() has no end", LOCAL_SERVICE_MCP.path))];
    let names = body
        .match_indices("(\"onecrew_")
        .map(|(offset, _)| {
            let name = &body[offset + 2..];
            &name[..name
                .find('"')
                .expect("bootstrap tool name is a string literal")]
        })
        .collect::<Vec<_>>();
    assert!(
        !names.is_empty(),
        "{} login_tools() lists no onecrew_ tool",
        LOCAL_SERVICE_MCP.path
    );
    names
}

fn serde_name(scope: impl serde::Serialize) -> String {
    serde_json::to_value(scope)
        .unwrap()
        .as_str()
        .expect("owner change scope serializes as a string")
        .to_owned()
}

#[test]
fn operation_id_scanner_ignores_urls_file_names_versions_members_and_prose() {
    let prefixes = ["email", "onecrew", "x"]
        .map(str::to_owned)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let line = "See https://x.com/onecrew.creator.get, e.g. x.json 0.6.0 v1.2 user@email.com \
        x.com/home email.trim() app.onecrew.creator.get `onecrew.creator.get`, then \
        onecrew.workflow.guide. and \"x.posts.search.advanced\"";
    assert_eq!(
        operation_id_literals(line, &prefixes),
        [
            "onecrew.creator.get",
            "onecrew.workflow.guide",
            "x.posts.search.advanced"
        ]
    );
}

#[test]
fn referenced_operation_ids_are_registered() {
    let registry = live_registry();
    let registered = registered_entry_ids(&registry)
        .iter()
        .map(|id| id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let prefixes = registry_prefixes(&registry);
    let mut resolved = 0_usize;
    let mut unregistered = Vec::new();
    for file in reference_inputs() {
        for (index, line) in file.text.lines().enumerate() {
            for literal in operation_id_literals(line, &prefixes) {
                let historical_result = matches!((file.path, literal),
                    ("apps/mcp-ui/src/test-support/dm-failure-fixture.ts", "onecrew.outreach.message.send")
                    | ("apps/mcp-ui/src/test-support/email-fixture.ts", "email.manual.send"));
                if historical_result {
                    assert!(onecrew_local_runtime::is_retired_operation(literal));
                    assert!(!registered.contains(literal));
                    assert!(registry.catalog().operation(literal).is_some());
                } else if registered.contains(literal) {
                    resolved += 1;
                } else {
                    unregistered.push(format!("{}:{}: `{literal}`", file.path, index + 1));
                }
            }
        }
    }
    assert!(
        unregistered.is_empty(),
        "literals with a registry prefix ({prefixes:?}) that are not a registered operation, \
         playbook, or toolkit-guide ID:\n{}",
        unregistered.join("\n")
    );
    assert!(
        resolved > 0,
        "the scanner resolved no registry reference in any input"
    );
}

#[test]
fn reference_inputs_carry_no_retired_tokens() {
    let retired = retired_tokens();
    let mut found = Vec::new();
    for file in reference_inputs() {
        let envelope = without_operation_arguments(file);
        for (index, (line, envelope_line)) in file.text.lines().zip(envelope.lines()).enumerate() {
            for token in &retired {
                let haystack = if token.plan_envelope_only {
                    envelope_line
                } else {
                    line
                };
                if names_retired(haystack, token) {
                    found.push(format!(
                        "{}:{}: `{}` ({})",
                        file.path,
                        index + 1,
                        token.token,
                        token.retired_as
                    ));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "retired tokens in drift inputs:\n{}",
        found.join("\n")
    );
}

#[test]
fn router_skill_introduces_live_meta_tools_in_surface_order() {
    // The stdio connection lists the bootstrap tools ahead of the business
    // surface, whose order is the one its live instructions introduce. The
    // Router's ordered Route section introduces tools in that same order.
    // The preceding workspace guidance can refer to bootstrap tools directly.
    let expected = bootstrap_tools()
        .into_iter()
        .map(str::to_owned)
        .chain(surface_tool_order())
        .collect::<Vec<_>>();
    let route = ROUTER_SKILL
        .text
        .split_once("## Route")
        .expect("Router Route section")
        .1;
    let introduced = first_occurrences(meta_tool_tokens(route));
    assert_eq!(
        introduced, expected,
        "{} Route introductions differ from the live bootstrap and surface order",
        ROUTER_SKILL.path
    );
}

#[test]
fn live_instructions_introduce_exactly_the_model_visible_meta_tools() {
    let (visible, app_only) = live_meta_tools();
    let introduced = surface_tool_order().into_iter().collect::<BTreeSet<_>>();
    assert_eq!(
        introduced, visible,
        "server/surface.rs instructions and the live full-surface tool router disagree"
    );
    assert!(
        introduced.is_disjoint(&app_only),
        "surface instructions name App-only tools {app_only:?}"
    );
}

#[test]
fn router_and_plugin_manifest_name_only_bootstrap_or_model_visible_tools() {
    let (visible, app_only) = live_meta_tools();
    let bootstrap = bootstrap_tools();
    let mut unknown = Vec::new();
    for file in router_files().chain([PLUGIN_MANIFEST]) {
        for (index, line) in file.text.lines().enumerate() {
            for token in meta_tool_tokens(line) {
                if !visible.contains(token) && !bootstrap.contains(&token) {
                    let kind = if app_only.contains(token) {
                        "App-only tool"
                    } else {
                        "not a live meta tool"
                    };
                    unknown.push(format!("{}:{}: `{token}` ({kind})", file.path, index + 1));
                }
            }
        }
    }
    assert!(unknown.is_empty(), "{}", unknown.join("\n"));
}

#[test]
fn web_runtime_change_scope_union_equals_owner_serde_names() {
    let source = WEB_RUNTIME_CHANGE.text;
    let union = &source[source
        .find("export type RuntimeChangeScope =")
        .unwrap_or_else(|| {
            panic!(
                "{} no longer exports the RuntimeChangeScope union",
                WEB_RUNTIME_CHANGE.path
            )
        })..];
    let union = &union[..union.find(';').expect("the union ends with a semicolon")];
    let members = union.split('"').skip(1).step_by(2).collect::<Vec<_>>();
    let web = members.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        web.len(),
        members.len(),
        "{} repeats a RuntimeChangeScope member: {members:?}",
        WEB_RUNTIME_CHANGE.path
    );
    let owner = CreatorChangeScope::ALL
        .into_iter()
        .map(serde_name)
        .chain(PlatformChangeScope::ALL.into_iter().map(serde_name))
        .chain(std::iter::once("all".to_owned()))
        .collect::<Vec<_>>();
    let owner = owner.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let missing = owner.difference(&web).collect::<Vec<_>>();
    let extra = web.difference(&owner).collect::<Vec<_>>();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{} RuntimeChangeScope union drifted from both owner serde names plus bridge all; missing \
         {missing:?}, not an owner scope {extra:?}",
        WEB_RUNTIME_CHANGE.path
    );
}

#[test]
fn drift_inputs_cover_every_fixture_and_preview_file() {
    fn directory_files(directory: &str, extensions: &[&str]) -> BTreeSet<String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(directory);
        std::fs::read_dir(&root)
            .unwrap_or_else(|error| panic!("read {directory}: {error}"))
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().unwrap().is_file())
            .map(|entry| entry.file_name().into_string().unwrap())
            .filter(|name| extensions.iter().any(|extension| name.ends_with(extension)))
            .map(|name| format!("{directory}/{name}"))
            .collect()
    }
    let listed = |files: &[ReferenceFile]| {
        files
            .iter()
            .map(|file| file.path.to_owned())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        listed(&MCP_UI_TEST_SUPPORT),
        directory_files(MCP_UI_TEST_SUPPORT_DIRECTORY, &[".ts", ".json"]),
        "list every {MCP_UI_TEST_SUPPORT_DIRECTORY} fixture in MCP_UI_TEST_SUPPORT"
    );
    assert_eq!(
        listed(&MCP_UI_PREVIEWS),
        directory_files(MCP_UI_PREVIEW_DIRECTORY, &[".json"]),
        "list every {MCP_UI_PREVIEW_DIRECTORY} export in MCP_UI_PREVIEWS"
    );
}
