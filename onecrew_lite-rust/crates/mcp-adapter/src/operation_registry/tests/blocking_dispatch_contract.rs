use std::{
    fs,
    path::{Path, PathBuf},
};

const RECEIVERS: [&str; 6] = ["creators", "owner", "owners", "platform", "o", "self"];
const ASYNC_BLOCK_OPENERS: [&str; 3] = ["async move {", "async move{", "async {"];

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
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

fn is_test_file(path: &Path) -> bool {
    let name = path.file_name().unwrap().to_str().unwrap();
    name == "tests.rs"
        || name == "test_support.rs"
        || name.ends_with("_tests.rs")
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

/// Production text of one file: everything before its `#[cfg(test)] mod`.
fn production_source(path: &Path) -> String {
    let source = fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    match source.find("#[cfg(test)]\nmod ") {
        Some(offset) => source[..offset].to_owned(),
        None => source,
    }
}

fn production_sources(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    rust_files(root, &mut files);
    files
        .into_iter()
        .filter(|path| !is_test_file(path))
        .map(|path| {
            let source = production_source(&path);
            (path, source)
        })
        .collect()
}

/// Handler-bearing production sources: every operation domain plus the
/// operator account owner the connection lifecycle handlers await.
fn handler_sources() -> Vec<(PathBuf, String)> {
    let mut sources = production_sources(&src_dir().join("operation_registry/domains"));
    let accounts = src_dir().join("accounts.rs");
    sources.push((accounts.clone(), production_source(&accounts)));
    sources
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn matching_close(text: &[u8], open: usize, open_byte: u8, close_byte: u8) -> usize {
    let mut depth = 0usize;
    for (offset, byte) in text[open..].iter().enumerate() {
        if *byte == open_byte {
            depth += 1;
        } else if *byte == close_byte {
            depth -= 1;
            if depth == 0 {
                return open + offset;
            }
        }
    }
    panic!("unbalanced delimiters after offset {open}");
}

fn skip_whitespace(text: &[u8], mut at: usize) -> usize {
    while at < text.len() && text[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

/// Byte ranges of every `async` body in the source: block bodies after
/// the openers above and `async fn` bodies (declarations without a body
/// are skipped).
fn async_bodies(source: &str) -> Vec<(usize, usize)> {
    let text = source.as_bytes();
    let mut bodies = Vec::new();
    for opener in ASYNC_BLOCK_OPENERS {
        for (offset, _) in source.match_indices(opener) {
            let open = offset + opener.len() - 1;
            bodies.push((open, matching_close(text, open, b'{', b'}')));
        }
    }
    for (offset, _) in source.match_indices("async fn") {
        let rest = &source[offset..];
        let body = rest.find('{');
        let declaration = rest.find(';');
        if let Some(body) = body
            && declaration.is_none_or(|declaration| body < declaration)
        {
            let open = offset + body;
            bodies.push((open, matching_close(text, open, b'{', b'}')));
        }
    }
    bodies
}

fn run_blocking_ranges(body: &str) -> Vec<(usize, usize)> {
    body.match_indices("run_blocking(")
        .map(|(offset, needle)| {
            let open = offset + needle.len() - 1;
            (open, matching_close(body.as_bytes(), open, b'(', b')'))
        })
        .collect()
}

/// Owner-receiver method calls inside one async body that are neither
/// awaited nor wrapped in `run_blocking`.
fn unwrapped_synchronous_calls(body: &str) -> Vec<String> {
    let text = body.as_bytes();
    let wrapped = run_blocking_ranges(body);
    let mut findings = Vec::new();
    let mut at = 0;
    while at < text.len() {
        if !is_ident(text[at]) || (at > 0 && is_ident(text[at - 1])) {
            at += 1;
            continue;
        }
        let start = at;
        while at < text.len() && is_ident(text[at]) {
            at += 1;
        }
        if !RECEIVERS.contains(&&body[start..at]) {
            continue;
        }
        let mut cursor = at;
        let call = loop {
            cursor = skip_whitespace(text, cursor);
            if cursor >= text.len() || text[cursor] != b'.' {
                break None;
            }
            cursor = skip_whitespace(text, cursor + 1);
            let segment = cursor;
            while cursor < text.len() && is_ident(text[cursor]) {
                cursor += 1;
            }
            if segment == cursor {
                break None;
            }
            let after = skip_whitespace(text, cursor);
            if after < text.len() && text[after] == b'(' {
                break Some(after);
            }
            cursor = after;
        };
        let Some(open) = call else {
            continue;
        };
        let close = matching_close(text, open, b'(', b')');
        let after_call = skip_whitespace(text, close + 1);
        let awaited = body[after_call..].starts_with(".await");
        let inside_wrapper = wrapped
            .iter()
            .any(|(wrapper_open, wrapper_close)| *wrapper_open < start && close < *wrapper_close);
        if !awaited && !inside_wrapper {
            findings.push(body[start..=close].to_owned());
        }
        at = close + 1;
    }
    findings
}

#[test]
fn every_handler_body_only_awaits_and_wraps_synchronous_owner_calls() {
    let mut findings = Vec::new();
    for (path, source) in handler_sources() {
        for (open, close) in async_bodies(&source) {
            for call in unwrapped_synchronous_calls(&source[open..=close]) {
                findings.push(format!("{}: {}", path.display(), call));
            }
        }
    }
    assert!(
        findings.is_empty(),
        "synchronous owner calls on a runtime worker:\n{}",
        findings.join("\n")
    );
}

#[test]
fn the_forbidden_inline_shapes_are_absent_from_every_domain() {
    for (path, source) in handler_sources() {
        for forbidden in [
            "async move { creators.",
            "async move { owner.",
            "async move { platform.",
        ] {
            for (offset, _) in source.match_indices(forbidden) {
                let line_end = source[offset..]
                    .find('\n')
                    .map_or(source.len(), |end| offset + end);
                let line = &source[offset..line_end];
                assert!(
                    line.contains(".await"),
                    "{}: `{line}` runs an owner call inline",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn contract_scanner_recognizes_the_forbidden_and_allowed_shapes() {
    let forbidden = "async move { creators.list_creators(input) }";
    let (open, close) = async_bodies(forbidden)[0];
    assert_eq!(
        unwrapped_synchronous_calls(&forbidden[open..=close]),
        ["creators.list_creators(input)"]
    );

    let multi_line = "async move {\n    let id = operator_id(context)?;\n    platform\n        .platform_x_get(id)\n        .map_err(Owner)\n}";
    let (open, close) = async_bodies(multi_line)[0];
    assert_eq!(
        unwrapped_synchronous_calls(&multi_line[open..=close]),
        ["platform\n        .platform_x_get(id)"]
    );

    let async_fn = "async fn list(&self) -> Result<Vec<Item>, String> {\n    self.platform.operator_account_list()\n}";
    let (open, close) = async_bodies(async_fn)[0];
    assert_eq!(
        unwrapped_synchronous_calls(&async_fn[open..=close]),
        ["self.platform.operator_account_list()"]
    );

    for allowed in [
        "async move { run_blocking(move || creators.list_creators(input)).await }",
        "async move { creators.send_email(input).await }",
        "async move {\n    let owner = Arc::clone(&owner);\n    run_blocking(move || resolve(owner.as_ref(), &context)).await?;\n    owner.profile(input).await.map_err(Owner)\n}",
        "async fn list(&self) -> Result<Vec<Item>, String> {\n    let platform = Arc::clone(&self.platform);\n    run_blocking(move || platform.operator_account_list()).await\n}",
    ] {
        let (open, close) = async_bodies(allowed)[0];
        assert!(
            unwrapped_synchronous_calls(&allowed[open..=close]).is_empty(),
            "{allowed}"
        );
    }
    assert!(async_bodies("async fn declared(&self) -> Result<(), String>;").is_empty());
}

#[test]
fn no_runtime_thread_blocks_and_only_the_wrapper_spawns_blocking_work() {
    for (path, source) in production_sources(&src_dir()) {
        for forbidden in ["block_on(", "block_in_place("] {
            assert!(
                !source.contains(forbidden),
                "{}: `{forbidden}` outside test code",
                path.display()
            );
        }
        let is_wrapper = path.ends_with("operation_registry/execution/blocking.rs");
        assert_eq!(
            source.contains("spawn_blocking("),
            is_wrapper,
            "{}: spawn_blocking must live only in the run_blocking wrapper",
            path.display()
        );
    }
}

#[test]
fn contract_scanner_recognizes_nested_test_directories() {
    for path in [
        "operation_registry/tests.rs",
        "operation_registry/schema_tests.rs",
        "operation_registry/domains/onecrew_writes/creators/test_support.rs",
        "operation_registry/tests/blocking_dispatch_contract.rs",
        "operation_registry/domains/platform_reads/tests/mappings.rs",
        "operation_registry/domains/task/tests/continuation/mod.rs",
    ] {
        assert!(is_test_file(Path::new(path)), "{path}");
    }
    for path in [
        "operation_registry/domains/platform_reads/mappings.rs",
        "operation_registry/execution/blocking.rs",
        "operation_registry/domains/latest.rs",
        "operation_registry/domains/test_support_runtime.rs",
        "operation_registry/domains/runtime_test_support.rs",
        "operation_registry/domains/test_support/runtime.rs",
    ] {
        assert!(!is_test_file(Path::new(path)), "{path}");
    }
}
