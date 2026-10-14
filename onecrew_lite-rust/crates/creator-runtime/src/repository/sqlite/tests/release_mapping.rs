//! Release-to-schema mapping (data-evolution matrix row "Reused application
//! version with a changed schema identity"): `CURRENT_SCHEMA_VERSION` must be
//! shipped by a distinct, recorded application release. The newest edge
//! module header (`//! CRM vN -> vN+1 (app X.Y.Z)`) is the frozen mapping,
//! `development/release/version.json` is the release record, and the two must
//! agree before packaging. A schema bump that reuses the release of the edge
//! before it, or that leaves the release record behind, fails here.

use std::path::{Path, PathBuf};

use super::schema;

type Release = (u64, u64, u64);

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two levels below the workspace root")
        .to_path_buf()
}

fn release(version: &str) -> Release {
    let parts: Vec<u64> = version
        .split('.')
        .map(|part| {
            part.parse()
                .unwrap_or_else(|_| panic!("{version}: release versions are exact X.Y.Z"))
        })
        .collect();
    let [major, minor, patch] = parts[..] else {
        panic!("{version}: release versions are exact X.Y.Z");
    };
    (major, minor, patch)
}

/// `(to_version, app release)` for every edge module whose header follows the
/// release-mapping convention, ordered by version. Edges that predate the
/// convention carry no header and stay frozen without one.
fn recorded_edge_releases() -> Vec<(i64, Release)> {
    let migrations =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/repository/sqlite/schema/migrations");
    let mut releases = Vec::new();
    for entry in std::fs::read_dir(&migrations).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((from, to)) = name
            .strip_suffix(".rs")
            .and_then(|stem| stem.strip_prefix('v'))
            .and_then(|stem| stem.split_once("_to_v"))
        else {
            continue;
        };
        let (Ok(from), Ok(to)) = (from.parse::<i64>(), to.parse::<i64>()) else {
            continue;
        };
        assert_eq!(to, from + 1, "{name}: an edge module is one adjacent step");
        let source = std::fs::read_to_string(entry.path()).unwrap();
        let header = source.lines().next().unwrap_or_default();
        let Some(rest) = header.strip_prefix("//! CRM v") else {
            continue;
        };
        let transition = format!("{from} -> v{to} (app ");
        let Some(version) = rest
            .strip_prefix(&transition)
            .and_then(|rest| rest.split_once(')'))
            .map(|(version, _)| version)
        else {
            panic!(
                "{name}: the header must read `//! CRM v{from} -> v{to} (app X.Y.Z)`, got `{header}`"
            );
        };
        releases.push((to, release(version)));
    }
    releases.sort_unstable();
    releases
}

#[test]
fn current_schema_version_is_shipped_by_a_distinct_recorded_release() {
    let record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace_root().join("development/release/version.json"))
            .unwrap(),
    )
    .unwrap();
    let application = release(record["cliPluginBase"].as_str().expect("cliPluginBase"));
    let schema_release = release(
        record["creatorCurrentSchemaIntroducedInRelease"]
            .as_str()
            .expect("creatorCurrentSchemaIntroducedInRelease"),
    );
    assert!(
        schema_release <= application,
        "the schema-introducing release {schema_release:?} cannot be newer than the distributed application {application:?}"
    );

    let edges = recorded_edge_releases();
    // Frozen released mappings prove the header parser reads real records.
    assert!(edges.contains(&(34, (0, 5, 22))), "{edges:?}");
    assert!(edges.contains(&(35, (0, 5, 23))), "{edges:?}");
    assert!(edges.contains(&(36, (0, 6, 0))), "{edges:?}");
    assert!(edges.contains(&(37, (0, 6, 1))), "{edges:?}");
    let (newest_version, newest_release) =
        *edges.last().expect("at least one edge records its release");
    assert_eq!(
        newest_version,
        schema::CURRENT_SCHEMA_VERSION,
        "the edge reaching CURRENT_SCHEMA_VERSION must record the application release that ships it"
    );
    assert_eq!(
        newest_release, schema_release,
        "development/release/version.json must name the release recorded by the newest edge"
    );
    for pair in edges.windows(2) {
        let (previous, current) = (pair[0], pair[1]);
        // Schemas 71 and 72 were already shipped together in 0.6.64. Keep
        // their released headers immutable; only this exact historical pair
        // is exempt. New edges (including 73) still require a newer release.
        if previous == (71, (0, 6, 64)) && current == (72, (0, 6, 64)) {
            continue;
        }
        assert!(
            previous.1 < current.1,
            "schema {} reuses application release {:?} of schema {}; a schema bump needs a new application version",
            current.0,
            current.1,
            previous.0
        );
    }
}
