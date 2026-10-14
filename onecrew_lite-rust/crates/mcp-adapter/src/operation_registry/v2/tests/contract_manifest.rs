use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use super::super::{
    contract_manifest::{app_presentation_kinds, result_contract_schema_manifest},
    projection::AdapterPresentationKind,
};

#[test]
fn mcp_app_contract_manifest_is_exact_and_exportable() {
    let manifest = result_contract_schema_manifest().expect("contract manifest");
    assert_eq!(manifest.target, "mcp-app");
    assert_eq!(manifest.roots.len(), 9);

    let input = root_schema(&manifest, "ExactResultStateInput");
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(input)
        .expect("exact input validator");
    let binding = binding_fixture();
    assert!(validator.is_valid(&json!({"action":"confirm","binding":binding})));
    assert!(!validator.is_valid(&json!({
        "action":"confirm",
        "input":{"binding":binding,"instruction":"wrong branch"}
    })));

    assert!(!validator.is_valid(&json!({"action":"confirm_creator_save","binding":binding,"confirmed_route_ids":["route-id"]})));
    assert!(validator.is_valid(&json!({"action":"confirm_creator_save_batch","binding":binding,"selections":[{"row_id":"row-1","confirmed_route_ids":["route-id"]}]})));

    let snapshot = root_schema(&manifest, "ResultSnapshot");
    let kinds =
        find_string_enum(snapshot, "search_plan_editor").expect("closed presentation kinds");
    assert_eq!(kinds.len(), 15);
    assert_eq!(
        kinds
            .iter()
            .map(|kind| kind.as_str().expect("string presentation kind"))
            .collect::<Vec<_>>(),
        app_presentation_kinds(),
        "the exported vocabulary must mirror the closed enum this schema carries"
    );
    for kind in [
        "creator_save_batch_outcome",
        "creator_batch_confirmation",
        "creator_batch_outcome",
        "outreach_confirmation",
        "outreach_outcome",
        "email_confirmation",
        "email_outcome",
    ] {
        assert!(
            kinds.iter().any(|value| value == kind),
            "runtime owner presentation {kind}"
        );
    }
    assert!(!kinds.iter().any(|kind| kind == "task_editor"));
    assert!(
        kinds
            .iter()
            .any(|kind| kind == "creator_save_batch_confirmation")
    );
    assert!(!kinds.iter().any(|kind| kind == "creator_save_confirmation"));
    assert!(kinds.iter().any(|kind| kind == "search_plan_editor"));
    assert!(all_u64_schemas_are_js_safe(snapshot));
    let runtime_projection = serde_json::to_value(schemars::schema_for!(
        onecrew_local_runtime::PresentationProjection
    ))
    .expect("runtime projection schema");
    let runtime_limit = &runtime_projection["properties"]["sections"]["maxItems"];
    // The exported bound is the owner batch projection, well above the
    // generic 32-section limit that ordinary projections are validated against.
    assert!(runtime_limit.as_u64().is_some_and(|limit| limit > 32));
    let mut exported_limits = Vec::new();
    collect_section_limits(snapshot, &mut exported_limits);
    assert!(
        !exported_limits.is_empty(),
        "the exported projection has a sections bound"
    );
    assert!(
        exported_limits.iter().all(|limit| *limit == runtime_limit),
        "the MCP schema must not shrink the runtime's supported batch projection"
    );

    if let Ok(path) = env::var("MCP_APP_CONTRACT_MANIFEST_OUT") {
        write_manifest(Path::new(&path), &manifest).expect("write contract manifest");
    }
}

/// The App's closed vocabulary must cover every presentation kind minted below
/// it. `ResultPresentationKind::new` validates identifier shape only, so a kind
/// minted outside this vocabulary passes every Rust lane and the App's own
/// drift test — both sides agree on a word neither of them contains — and first
/// fails in the App, which judges the whole snapshot invalid rather than the one
/// unknown field.
///
/// Two source trees are readable from here without inverting an edge: this
/// crate's own, and `platform-runtime`, which this crate depends on. A crate
/// that depends on *this* one checks itself from its own tests instead — see
/// `onecrew-local-service` — because a scan pointed at a dependent's layout
/// would go silently empty the day that layout moved.
#[test]
fn the_closed_vocabulary_covers_every_presentation_kind_minted_below_the_app() {
    let vocabulary = app_presentation_kinds();
    let mut unregistered = Vec::new();
    let mut typed_sites = Vec::new();
    for (path, source) in scanned_production_sources() {
        for mint in minted_presentation_kinds(&path, &source) {
            match mint {
                PresentationKindMint::Literal(kind) => {
                    if !vocabulary.contains(&kind.as_str()) {
                        unregistered.push(format!("{}: {kind}", path.display()));
                    }
                }
                PresentationKindMint::Typed => typed_sites.push(path.clone()),
            }
        }
    }
    assert!(
        unregistered.is_empty(),
        "these mint presentation kinds this App cannot render; add each to \
         AdapterPresentationKindContract and give it a view:\n{}",
        unregistered.join("\n")
    );

    // The single mint that hands over a closed enum's own string rather than a
    // literal. Its kinds are covered by type just below, so keep it single: a
    // second such site would be a hole this scan reads straight past.
    assert_eq!(
        typed_sites.len(),
        1,
        "the closed adapter enum is meant to be the only non-literal \
         presentation-kind mint; found {typed_sites:?}"
    );
    assert_eq!(
        typed_sites[0].file_name().and_then(|name| name.to_str()),
        Some("call.rs"),
        "the non-literal mint moved; aim the typed coverage below at its enum"
    );
    for kind in AdapterPresentationKind::ALL.map(AdapterPresentationKind::as_str) {
        assert!(
            vocabulary.contains(&kind),
            "this adapter's own closed kind {kind} is outside the App vocabulary"
        );
    }
}

/// Guards the scan itself: it must really read the mints of both scanned
/// crates, and it must keep ignoring the arbitrary fixture kinds of test-only
/// modules.
#[test]
fn the_presentation_kind_scanner_reads_mints_and_skips_test_fixtures() {
    let minted = scanned_production_sources()
        .iter()
        .flat_map(|(path, source)| minted_presentation_kinds(path, source))
        .filter_map(|mint| match mint {
            PresentationKindMint::Literal(kind) => Some(kind),
            PresentationKindMint::Typed => None,
        })
        .collect::<BTreeSet<_>>();
    for kind in [
        "email_confirmation",
        "email_outcome",
        "outreach_confirmation",
        "search_plan_editor",
    ] {
        assert!(
            minted.contains(kind),
            "the scan reads {kind} from the runtime"
        );
    }
    assert!(
        minted.contains("batch_result"),
        "the scan reads this crate's own mints too, not only the runtime's"
    );
    for fixture in [
        "generic_confirmation",
        "creator_discovery",
        "creator_result",
    ] {
        assert!(
            !minted.contains(fixture),
            "{fixture} is a test fixture that never reaches the App"
        );
    }
    assert_eq!(
        minted_presentation_kinds(
            Path::new("sample.rs"),
            "kind: ResultPresentationKind::new(\"a_kind\")?,"
        ),
        [PresentationKindMint::Literal("a_kind".to_owned())]
    );
    assert_eq!(
        minted_presentation_kinds(
            Path::new("sample.rs"),
            "ResultPresentationKind::new(self.as_str())"
        ),
        [PresentationKindMint::Typed]
    );
    // A `mod tests;` declaration keeps the production code that follows it; an
    // inline `mod tests { … }` body is where the file's test code starts.
    let declaration = "mod a;\n#[cfg(test)]\nmod tests;\nmod b;\nfn live() {}\n";
    assert_eq!(production_source(declaration), declaration);
    assert_eq!(
        production_source("fn live() {}\n\n#[cfg(test)]\nmod tests {\n    fn fixture() {}\n}\n"),
        "fn live() {}\n\n"
    );
}

const PRESENTATION_KIND_MINT: &str = "ResultPresentationKind::new(";
/// The one non-literal mint form this scan accepts: a closed Rust enum handing
/// over its own wire string. Those kinds are asserted by type, so they never
/// have to be recovered from source text.
const TYPED_MINT_ARGUMENT: &str = "self.as_str()";

#[derive(Debug, PartialEq, Eq)]
enum PresentationKindMint {
    Literal(String),
    Typed,
}

/// Every presentation kind minted in one production source. A mint that is
/// neither a plain string literal nor a closed enum's own conversion fails here
/// instead of escaping the scan unseen.
fn minted_presentation_kinds(path: &Path, source: &str) -> Vec<PresentationKindMint> {
    source
        .match_indices(PRESENTATION_KIND_MINT)
        .map(|(offset, needle)| {
            let rest = source[offset + needle.len()..].trim_start();
            if rest.starts_with(TYPED_MINT_ARGUMENT) {
                return PresentationKindMint::Typed;
            }
            let literal = rest.strip_prefix('"').unwrap_or_else(|| {
                panic!(
                    "{}: mint every presentation kind from a string literal, or from a closed \
                     enum's `{TYPED_MINT_ARGUMENT}`, so this scan can read it; found `{}`",
                    path.display(),
                    &rest[..rest.len().min(40)]
                )
            });
            let end = literal
                .find('"')
                .unwrap_or_else(|| panic!("{}: unterminated presentation kind", path.display()));
            PresentationKindMint::Literal(literal[..end].to_owned())
        })
        .collect()
}

/// Every source tree this scan may read: the runtime owner this crate depends
/// on, and this crate itself.
fn scanned_production_sources() -> Vec<(PathBuf, String)> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources = production_sources(&manifest.join("../platform-runtime/src"));
    sources.extend(production_sources(&manifest.join("src")));
    sources
}

/// Production sources under one crate's `src`. Test-only modules are excluded
/// by path: a `tests.rs` file and a `tests` directory are wholly test code, so
/// their fixture kinds are arbitrary strings that never reach the App.
///
/// A missing or empty tree fails here. Left unguarded, a scan that read nothing
/// would report every assertion above as satisfied.
fn production_sources(root: &Path) -> Vec<(PathBuf, String)> {
    assert!(
        root.is_dir(),
        "no source tree at {}; a scan over a missing directory proves nothing",
        root.display()
    );
    let mut files = Vec::new();
    rust_files(root, &mut files);
    assert!(!files.is_empty(), "no source under {}", root.display());
    let sources = files
        .into_iter()
        .filter(|path| !is_test_path(path))
        .map(|path| {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let production = production_source(&source).to_owned();
            (path, production)
        })
        .collect::<Vec<_>>();
    assert!(
        !sources.is_empty(),
        "every source under {} was filtered out as test-only",
        root.display()
    );
    sources
}

/// Production text of one scanned source: everything before an inline
/// `#[cfg(test)] mod name { … }`. A `#[cfg(test)] mod name;` declaration only
/// names a separate file that `is_test_path` already excludes, so truncating
/// there would silently drop every production mint that follows the
/// declaration — where the runtime happens to mint `search_plan_editor`.
fn production_source(source: &str) -> &str {
    const TEST_MODULE: &str = "#[cfg(test)]\nmod ";
    let mut at = 0;
    while let Some(found) = source[at..].find(TEST_MODULE) {
        let start = at + found;
        let rest = &source[start + TEST_MODULE.len()..];
        let name_end = rest
            .find(|character: char| !character.is_alphanumeric() && character != '_')
            .unwrap_or(rest.len());
        if rest[name_end..].trim_start().starts_with('{') {
            return &source[..start];
        }
        at = start + TEST_MODULE.len();
    }
    source
}

fn rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(root)
        .unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

fn is_test_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name == "tests.rs"
        || name == "test_support.rs"
        || name.ends_with("_tests.rs")
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

fn collect_section_limits<'a>(value: &'a Value, limits: &mut Vec<&'a Value>) {
    match value {
        Value::Object(object) => {
            if let Some(limit) = value.pointer("/properties/sections/maxItems") {
                limits.push(limit);
            }
            for child in object.values() {
                collect_section_limits(child, limits);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_section_limits(child, limits);
            }
        }
        _ => {}
    }
}

fn root_schema<'a>(manifest: &'a onecrew_schema_manifest::SchemaManifest, name: &str) -> &'a Value {
    &manifest
        .roots
        .iter()
        .find(|root| root.name == name)
        .expect("schema root")
        .schema
}

fn find_string_enum<'a>(value: &'a Value, member: &str) -> Option<&'a Vec<Value>> {
    match value {
        Value::Object(object) => object
            .get("enum")
            .and_then(Value::as_array)
            .filter(|values| values.iter().any(|value| value == member))
            .or_else(|| {
                object
                    .values()
                    .find_map(|child| find_string_enum(child, member))
            }),
        Value::Array(values) => values
            .iter()
            .find_map(|child| find_string_enum(child, member)),
        _ => None,
    }
}

fn all_u64_schemas_are_js_safe(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            let safe = object.get("format") != Some(&Value::String("uint64".into()))
                || object.get("maximum") == Some(&Value::from(9_007_199_254_740_991_u64));
            safe && object.values().all(all_u64_schemas_are_js_safe)
        }
        Value::Array(values) => values.iter().all(all_u64_schemas_are_js_safe),
        _ => true,
    }
}

fn write_manifest(
    path: &Path,
    manifest: &onecrew_schema_manifest::SchemaManifest,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(manifest).expect("manifest json"),
    )
}

fn binding_fixture() -> Value {
    json!({
        "result_ref":format!("rr_{}", "1".repeat(32)),
        "view":{"identity":format!("rv_{}", "2".repeat(32)),"kind":"confirmation"},
        "origin":"mcp:full",
        "fingerprint":format!("sha256:{}", "3".repeat(64)),
        "revision":2
    })
}
