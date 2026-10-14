//! Guards this host's presentation-kind mints against the closed vocabulary the
//! MCP App can render.
//!
//! The edge runs one way: `local-service` depends on `onecrew-mcp-adapter`, so
//! the vocabulary is imported here and the sources scanned are this crate's
//! own. The mirror image — `mcp-adapter` walking this crate's source tree —
//! would make the depended-upon crate track a directory layout it does not own.
//! Nothing at the Cargo level would object, because the coupling is to source
//! text rather than to a `use`; the cost lands later, when this crate moves a
//! module and that scan either fails for a reason belonging to neither crate
//! or, worse, walks an empty tree and reports a pass.
//!
//! No production path in this crate mints a presentation kind today: both mints
//! under `src` are `#[cfg(test)]` fixtures. That makes the membership assertion
//! true over an empty set, which is exactly what a broken scan would also
//! produce — so `the_presentation_kind_scanner_really_reads_this_crates_sources`
//! proves the walk works rather than trusting the empty result.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use onecrew_mcp_adapter::app_presentation_kinds;

#[test]
fn the_app_vocabulary_covers_every_presentation_kind_this_host_mints() {
    let vocabulary = app_presentation_kinds();
    assert!(
        !vocabulary.is_empty(),
        "onecrew-mcp-adapter exported an empty presentation vocabulary; every kind on earth \
         would satisfy a membership test against it"
    );
    let mut unregistered = Vec::new();
    for (path, source) in production_sources() {
        for kind in minted_presentation_kinds(&path, &source) {
            if !vocabulary.contains(&kind.as_str()) {
                unregistered.push(format!("{}: {kind}", path.display()));
            }
        }
    }
    assert!(
        unregistered.is_empty(),
        "this host mints presentation kinds the MCP App cannot render; register each in \
         onecrew-mcp-adapter's closed vocabulary and give it a view, or project through an \
         owner that already mints a registered kind:\n{}",
        unregistered.join("\n")
    );
}

/// Guards the scan above against the one failure that would make it useless: a
/// pass earned by reading nothing.
#[test]
fn the_presentation_kind_scanner_really_reads_this_crates_sources() {
    // The walk reaches this crate's real files and the parser reads real mints
    // out of them. Both of this crate's mints sit in `#[cfg(test)]` fixtures,
    // so this control scans every source rather than the production subset.
    let every_mint = rust_sources()
        .into_iter()
        .flat_map(|path| {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            minted_presentation_kinds(&path, &source)
        })
        .collect::<BTreeSet<_>>();
    for kind in ["outreach_draft", "record_preview"] {
        assert!(
            every_mint.contains(kind),
            "the scan no longer reads {kind} from this crate; aim this control at a mint that \
             still exists rather than deleting it"
        );
    }

    // Truncation keeps production text and drops the fixture, in a real file of
    // this crate: `work.rs` mints `outreach_draft` inside `#[cfg(test)]`, below
    // the `host_busy` rejection, which is production.
    let (path, production) = production_sources()
        .into_iter()
        .find(|(path, _)| path.ends_with("work.rs"))
        .expect("work.rs is scanned as a production source");
    assert!(
        production.contains("host_busy"),
        "{}: production text was truncated away",
        path.display()
    );
    assert!(
        !production.contains("outreach_draft"),
        "{}: a #[cfg(test)] fixture leaked into the production text",
        path.display()
    );

    // The parser and the truncation rule themselves.
    assert_eq!(
        minted_presentation_kinds(Path::new("sample.rs"), MINT_SAMPLE),
        ["a_kind"]
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

/// Spelled in halves so this file never contains the needle itself: the control
/// above scans every source under `src`, this one included.
const PRESENTATION_KIND_MINT: &str = concat!("ResultPresentationKind::", "new(");
const MINT_SAMPLE: &str = concat!("kind: ResultPresentationKind::", "new(\"a_kind\")?,");

/// Every presentation kind minted in one source. A mint that is not a plain
/// string literal fails here instead of escaping the scan unseen.
fn minted_presentation_kinds(path: &Path, source: &str) -> Vec<String> {
    source
        .match_indices(PRESENTATION_KIND_MINT)
        .map(|(offset, needle)| {
            let rest = source[offset + needle.len()..].trim_start();
            let literal = rest.strip_prefix('"').unwrap_or_else(|| {
                panic!(
                    "{}: keep every presentation-kind mint a string literal so this scan can \
                     read it, found `{}`",
                    path.display(),
                    &rest[..rest.len().min(40)]
                )
            });
            let end = literal
                .find('"')
                .unwrap_or_else(|| panic!("{}: unterminated presentation kind", path.display()));
            literal[..end].to_owned()
        })
        .collect()
}

/// Production text of one source: everything before an inline
/// `#[cfg(test)] mod name { … }`. A `#[cfg(test)] mod name;` declaration only
/// names a separate file that `is_test_path` already excludes, so truncating
/// there would silently drop every production mint that follows it.
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

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every Rust source under this crate's `src`. A missing or empty tree fails
/// here rather than yielding a scan that passes because it read nothing.
fn rust_sources() -> Vec<PathBuf> {
    let root = source_root();
    assert!(
        root.is_dir(),
        "no source tree at {}; a scan over a missing directory proves nothing",
        root.display()
    );
    let mut files = Vec::new();
    collect_rust_files(&root, &mut files);
    assert!(
        !files.is_empty(),
        "no Rust source under {}; a scan over an empty tree proves nothing",
        root.display()
    );
    files
}

/// Production sources of this crate, test-only modules excluded by path.
fn production_sources() -> Vec<(PathBuf, String)> {
    let sources = rust_sources()
        .into_iter()
        .filter(|path| !is_test_path(path))
        .map(|path| {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
                .replace("\r\n", "\n");
            let production = production_source(&source).to_owned();
            (path, production)
        })
        .collect::<Vec<_>>();
    assert!(
        !sources.is_empty(),
        "every source under {} was filtered out as test-only",
        source_root().display()
    );
    sources
}

fn collect_rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(root)
        .unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rust_files(&path, out);
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
