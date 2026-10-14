//! MCP Apps UI resources: self-contained `ui://` components embedded at
//! compile time and served through the standard resources capability.
//! Workflow discovery stays in the stateless catalog (ADR 0006/0008); these
//! resources are UI templates only, reached from a tool's
//! `_meta.ui.resourceUri`. Each URI embeds a content hash, because hosts
//! treat the URI as a cache key: changed bytes must ship under a new URI.

use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use rmcp::model::{ReadResourceResult, Resource, ResourceContents};

/// MIME required by the MCP Apps spec (SEP-1865).
pub(crate) const APPS_RESOURCE_MIME: &str = "text/html;profile=mcp-app";

struct AppWidget {
    key: &'static str,
    title: &'static str,
    html: &'static str,
    uri: OnceLock<String>,
}

impl AppWidget {
    fn uri(&self) -> &str {
        self.uri.get_or_init(|| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            self.html.hash(&mut hasher);
            format!("ui://onecrew/{}/{:016x}.html", self.key, hasher.finish())
        })
    }
}

// Every MCP Apps Host mounts the one business-intent action card.
// Old feature-specific cards are intentionally not replay-compatible.
static WIDGETS: [AppWidget; 1] = [AppWidget {
    key: "operation-inspector",
    title: "onecrew_lite action card",
    html: include_str!("../assets/apps/operation-inspector.html"),
    uri: OnceLock::new(),
}];

const RETIRED_INSPECTOR_HTML: &str =
    include_str!("../assets/apps/retired-operation-inspector.html");

const PUBLIC_WIDGET_KEY: &str = "operation-inspector";

/// The raw self-contained HTML for a shipped widget, for hosts that embed
/// it directly instead of reading `ui://` over MCP — an external MCP Apps
/// Host loads it through its host integration (the host never calls the
/// loopback MCP endpoint through this helper).
pub fn widget_template(key: &str) -> Option<&'static str> {
    WIDGETS
        .iter()
        .find(|widget| widget.key == key)
        .map(|widget| widget.html)
}

/// The versioned URI for a shipped widget. Panics on an unknown key so a
/// tool cannot reference a template that does not ship — the same
/// fail-at-construction stance as the tool metadata registry.
pub(crate) fn app_resource_uri(key: &str) -> &'static str {
    WIDGETS
        .iter()
        .find(|widget| widget.key == key)
        .unwrap_or_else(|| panic!("unknown app widget {key}"))
        .uri()
}

/// List the action card with the Apps MIME. Feature-specific keys are
/// retired rather than served as compatibility aliases.
pub(crate) fn list_app_resources() -> Vec<Resource> {
    WIDGETS
        .iter()
        .filter(|widget| widget.key == PUBLIC_WIDGET_KEY)
        .map(|widget| {
            Resource::new(widget.uri(), widget.key)
                .with_title(widget.title)
                .with_mime_type(APPS_RESOURCE_MIME)
                .with_size(widget.html.len() as u64)
        })
        .collect()
}

/// Serve the current inspector bytes for its exact URI. A well-formed retired
/// inspector URI receives one inert version notice under the requested URI;
/// other schemes, widget keys, and malformed paths still fail closed.
pub(crate) fn read_app_resource(uri: &str) -> Option<ReadResourceResult> {
    if let Some(widget) = WIDGETS
        .iter()
        .find(|widget| widget.key == PUBLIC_WIDGET_KEY && widget.uri() == uri)
    {
        return Some(ReadResourceResult::new(vec![
            ResourceContents::text(widget.html, uri).with_mime_type(APPS_RESOURCE_MIME),
        ]));
    }
    is_retired_inspector_uri(uri).then(|| {
        ReadResourceResult::new(vec![
            ResourceContents::text(RETIRED_INSPECTOR_HTML, uri).with_mime_type(APPS_RESOURCE_MIME),
        ])
    })
}

fn is_retired_inspector_uri(uri: &str) -> bool {
    let Some(hash) = uri
        .strip_prefix("ui://onecrew/operation-inspector/")
        .and_then(|path| path.strip_suffix(".html"))
    else {
        return false;
    };
    hash.len() == 16
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_uris_are_hashed_ui_scheme_and_stable() {
        let uri = app_resource_uri("operation-inspector");
        assert!(
            uri.starts_with("ui://onecrew/operation-inspector/") && uri.ends_with(".html"),
            "{uri}"
        );
        assert_eq!(
            uri,
            app_resource_uri("operation-inspector"),
            "uri is stable per build"
        );
    }

    #[test]
    fn operation_inspector_is_a_shipped_self_contained_template() {
        let uri = app_resource_uri("operation-inspector");
        assert!(uri.starts_with("ui://onecrew/operation-inspector/"));
        let html = widget_template("operation-inspector").expect("inspector template ships");
        assert!(html.contains("onecrew_lite action card"));
        assert!(!html.contains("src=\"http") && !html.contains("href=\"http"));
    }

    #[test]
    fn listing_and_reading_agree_and_unknown_uris_fail_closed() {
        let listed = list_app_resources();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].uri.contains("/operation-inspector/"));
        assert_eq!(listed[0].title.as_deref(), Some("onecrew_lite action card"));
        for resource in &listed {
            assert_eq!(resource.mime_type.as_deref(), Some(APPS_RESOURCE_MIME));
            let read = read_app_resource(&resource.uri).expect("listed resource is readable");
            match &read.contents[0] {
                ResourceContents::TextResourceContents {
                    uri,
                    mime_type,
                    text,
                    ..
                } => {
                    assert_eq!(uri, &resource.uri);
                    assert_eq!(mime_type.as_deref(), Some(APPS_RESOURCE_MIME));
                    assert!(text.contains("<html"), "self-contained html");
                    assert!(
                        !text.contains("src=\"http") && !text.contains("href=\"http"),
                        "no external references"
                    );
                }
                other => panic!("expected text contents, got {other:?}"),
            }
        }
        assert!(read_app_resource("file:///etc/passwd").is_none());
    }

    #[test]
    fn retired_inspector_hash_returns_one_inert_version_notice() {
        let mut tombstone = None;
        for stale in [
            "ui://onecrew/operation-inspector/cd5a212247364733.html",
            "ui://onecrew/operation-inspector/ff5957e3adb8517b.html",
        ] {
            let read = read_app_resource(stale).expect("retired inspector tombstone");
            let ResourceContents::TextResourceContents { uri, text, .. } = &read.contents[0] else {
                panic!("expected text tombstone");
            };
            assert_eq!(uri, stale);
            assert!(text.contains("onecrew_lite card version notice"));
            assert!(text.contains("卡片版本不兼容"));
            assert!(!text.contains("onecrew_result_state"));
            assert!(!text.contains("onecrew_tool_execute"));
            if let Some(expected) = &tombstone {
                assert_eq!(text, expected, "all retired hashes share one tombstone");
            } else {
                tombstone = Some(text.clone());
            }
        }
    }

    #[test]
    fn unknown_keys_and_malformed_uris_fail_closed() {
        assert!(read_app_resource("ui://onecrew/unknown-widget/deadbeef.html").is_none());
        for key in ["task-setup", "user-choice", "hello"] {
            assert!(
                read_app_resource(&format!("ui://onecrew/{key}/deadbeef.html")).is_none(),
                "retired widget URI {key} must fail closed"
            );
        }
        assert!(read_app_resource("ui://onecrew/operation-inspector/deadbeef.txt").is_none());
        assert!(
            read_app_resource("ui://onecrew/operation-inspector/DEADBEEF00000000.html").is_none()
        );
        assert!(
            read_app_resource("ui://onecrew/operation-inspector/extra/deadbeef.html").is_none()
        );
        assert!(read_app_resource("ui://onecrew/operation-inspector").is_none());
        assert!(read_app_resource("ui://other-host/operation-inspector/deadbeef.html").is_none());
    }

    #[test]
    fn template_catalog_contains_only_the_generic_inspector() {
        assert!(widget_template("operation-inspector").is_some());
        for key in ["task-setup", "user-choice"] {
            assert!(
                widget_template(key).is_none(),
                "legacy template {key} survived"
            );
        }
        assert!(widget_template("hello").is_none());
    }

    #[test]
    fn creator_sync_control_plane_has_no_mcp_app_resource() {
        let forbidden = [
            "activation",
            "pairing",
            "enrollment",
            "creator_sync",
            "creator-sync",
            "snapshot_sync",
            "snapshot-sync",
            "creator-snapshots",
        ];
        let listed = list_app_resources();
        for resource in listed {
            let read = read_app_resource(&resource.uri).expect("listed resource is readable");
            let wire = format!("{resource:?} {read:?}").to_ascii_lowercase();
            for token in &forbidden {
                assert!(!wire.contains(token), "MCP App resource exposes {token}");
            }
        }
    }

    #[test]
    fn production_host_surface_has_no_retired_embedded_envoy_language() {
        for (path, source) in [
            ("lib.rs", include_str!("lib.rs")),
            ("apps.rs", include_str!("apps.rs")),
        ] {
            let production = source
                .split_once("#[cfg(test)]")
                .map_or(source, |(production, _)| production);
            for retired in [
                "Envoy",
                "envoy-widget",
                "chat_envoy",
                "local.envoy",
                "ENVOY_",
            ] {
                assert!(
                    !production.contains(retired),
                    "{path} retains retired embedded-host text: {retired}"
                );
            }
        }
        assert!(widget_template("operation-inspector").is_some());
        let listed = list_app_resources();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].uri.contains("/operation-inspector/"));
        let read = read_app_resource(&listed[0].uri).expect("external host resource is readable");
        assert!(format!("{read:?}").contains("operation-inspector"));
    }
}
