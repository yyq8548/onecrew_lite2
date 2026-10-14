//! Fixed production origins and typed Creator Sync endpoint construction.

#[cfg(test)]
use url::Host;
use url::Url;
use uuid::Uuid;

struct ProductionEndpoints {
    access_origin: &'static str,
    sync_origin: &'static str,
}

const PRODUCTION_ENDPOINTS: ProductionEndpoints = ProductionEndpoints {
    access_origin: "https://access.onecrewai.io",
    sync_origin: "https://api.onecrewai.io",
};

/// Canonical Access origin shared by enrollment and device-authenticated integrations.
pub(crate) const fn production_access_origin() -> &'static str {
    PRODUCTION_ENDPOINTS.access_origin
}
const AUTHORIZE_QUERY_KEYS: [&str; 7] = [
    "app_version",
    "callback_port",
    "code_challenge",
    "code_challenge_method",
    "label",
    "platform",
    "state",
];

#[derive(Clone)]
pub(crate) struct CreatorSyncEndpoints {
    access_origin: Url,
    sync_origin: Url,
}

pub(crate) struct PairingAuthorizeRequest<'a> {
    pub(crate) app_version: &'a str,
    pub(crate) callback_port: u16,
    pub(crate) code_challenge: &'a str,
    pub(crate) platform: &'a str,
    pub(crate) state: &'a str,
    pub(crate) label: &'a crate::DeviceLabel,
}

impl CreatorSyncEndpoints {
    pub(crate) fn production() -> Self {
        Self {
            access_origin: Url::parse(PRODUCTION_ENDPOINTS.access_origin)
                .expect("the compiled Access origin must be a valid URL"),
            sync_origin: Url::parse(PRODUCTION_ENDPOINTS.sync_origin)
                .expect("the compiled Creator Sync origin must be a valid URL"),
        }
    }

    pub(crate) fn pairing_authorize_url(
        &self,
        request: PairingAuthorizeRequest<'_>,
    ) -> Result<String, ()> {
        let mut url = self.access_path(&["pairing", "authorize"]);
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("app_version", request.app_version);
            query.append_pair("callback_port", &request.callback_port.to_string());
            query.append_pair("code_challenge", request.code_challenge);
            query.append_pair("code_challenge_method", "S256");
            query.append_pair("label", request.label.as_str());
            query.append_pair("platform", request.platform);
            query.append_pair("state", request.state);
        }
        authorize_query_is_exact(&url).then(|| url.into()).ok_or(())
    }

    pub(crate) fn installations(&self) -> Url {
        self.sync_path(&["installations"])
    }

    pub(crate) fn installation(&self, installation_id: Uuid) -> Url {
        let installation_id = installation_id.to_string();
        self.sync_path(&["installations", &installation_id])
    }

    pub(crate) fn credential_probe(&self) -> Url {
        self.sync_path(&["creator-sync", "credential"])
    }

    pub(crate) fn snapshots(&self) -> Url {
        self.sync_path(&["creator-snapshots"])
    }

    pub(crate) fn creator_backups_plan(&self) -> Url {
        self.sync_path(&["creator-backups", "plan"])
    }

    pub(crate) fn snapshot(&self, snapshot_id: Uuid) -> Url {
        let snapshot_id = snapshot_id.to_string();
        self.sync_path(&["creator-snapshots", &snapshot_id])
    }

    pub(crate) fn snapshot_chunk(&self, snapshot_id: Uuid, chunk_index: usize) -> Url {
        let snapshot_id = snapshot_id.to_string();
        let chunk_index = chunk_index.to_string();
        self.sync_path(&["creator-snapshots", &snapshot_id, "chunks", &chunk_index])
    }

    pub(crate) fn snapshot_complete(&self, snapshot_id: Uuid) -> Url {
        let snapshot_id = snapshot_id.to_string();
        self.sync_path(&["creator-snapshots", &snapshot_id, "complete"])
    }

    fn access_path(&self, segments: &[&str]) -> Url {
        endpoint_path(&self.access_origin, segments)
    }

    pub(crate) fn table_sync(&self, segments: &[&str]) -> Url {
        self.sync_path(segments)
    }

    fn sync_path(&self, segments: &[&str]) -> Url {
        endpoint_path(&self.sync_origin, segments)
    }

    #[cfg(test)]
    pub(crate) fn with_sync_origin_for_test(value: &str, allow_loopback: bool) -> Result<Self, ()> {
        let sync_origin = parse_test_origin(value, allow_loopback)?;
        Ok(Self {
            access_origin: Url::parse(PRODUCTION_ENDPOINTS.access_origin).map_err(|_| ())?,
            sync_origin,
        })
    }
}

fn endpoint_path(origin: &Url, segments: &[&str]) -> Url {
    let mut url = origin.clone();
    let mut path = url
        .path_segments_mut()
        .expect("compiled HTTP origins must support path segments");
    path.clear();
    path.extend(segments.iter().copied());
    drop(path);
    url
}

fn authorize_query_is_exact(url: &Url) -> bool {
    let pairs = url.query_pairs().collect::<Vec<_>>();
    pairs.len() == AUTHORIZE_QUERY_KEYS.len()
        && pairs
            .iter()
            .map(|(key, _)| key.as_ref())
            .eq(AUTHORIZE_QUERY_KEYS)
        && pairs
            .iter()
            .find(|(key, _)| key == "code_challenge_method")
            .is_some_and(|(_, value)| value == "S256")
        && pairs
            .iter()
            .find(|(key, _)| key == "callback_port")
            .and_then(|(_, value)| value.parse::<u16>().ok())
            .is_some_and(|port| port >= 1024)
        && !pairs.iter().any(|(key, _)| key == "redirect_uri")
}

#[cfg(test)]
fn parse_test_origin(value: &str, allow_loopback: bool) -> Result<Url, ()> {
    if value.is_empty() || value.trim() != value {
        return Err(());
    }
    let origin = Url::parse(value).map_err(|_| ())?;
    let test_http = allow_loopback && origin.scheme() == "http";
    if origin.scheme() != "https" && !test_http {
        return Err(());
    }
    if !origin.username().is_empty()
        || origin.password().is_some()
        || origin.query().is_some()
        || origin.fragment().is_some()
        || origin.path() != "/"
        || (!allow_loopback && has_explicit_port(value))
    {
        return Err(());
    }
    let loopback = match origin.host() {
        Some(Host::Domain(domain)) => {
            domain.eq_ignore_ascii_case("localhost")
                || domain.to_ascii_lowercase().ends_with(".localhost")
        }
        Some(Host::Ipv4(address)) => address.is_loopback() || address.is_unspecified(),
        Some(Host::Ipv6(address)) => address.is_loopback() || address.is_unspecified(),
        None => true,
    };
    if loopback && !allow_loopback {
        return Err(());
    }
    Ok(origin)
}

#[cfg(test)]
fn has_explicit_port(value: &str) -> bool {
    let Some((_, remainder)) = value.split_once("://") else {
        return false;
    };
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    if let Some(bracket) = authority.rfind(']') {
        authority[bracket + 1..].starts_with(':')
    } else {
        authority.contains(':')
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use url::Url;
    use uuid::Uuid;

    use super::{CreatorSyncEndpoints, PRODUCTION_ENDPOINTS, authorize_query_is_exact};

    #[test]
    fn production_endpoints_derive_every_path_from_two_origins() {
        let endpoints = CreatorSyncEndpoints::production();
        assert_eq!(
            endpoints.installations().origin().ascii_serialization(),
            PRODUCTION_ENDPOINTS.sync_origin
        );
        assert_eq!(endpoints.installations().path(), "/installations");
        let installation = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
        let snapshot = Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap();
        assert_eq!(
            endpoints.installation(installation).path(),
            "/installations/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
        );
        assert_eq!(
            endpoints.credential_probe().path(),
            "/creator-sync/credential"
        );
        assert_eq!(endpoints.snapshots().path(), "/creator-snapshots");
        assert_eq!(
            endpoints.creator_backups_plan().path(),
            "/creator-backups/plan"
        );
        assert_eq!(
            endpoints.snapshot(snapshot).path(),
            "/creator-snapshots/bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
        );
        assert_eq!(
            endpoints.snapshot_chunk(snapshot, 7).path(),
            "/creator-snapshots/bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb/chunks/7"
        );
        assert_eq!(
            endpoints.snapshot_complete(snapshot).path(),
            "/creator-snapshots/bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb/complete"
        );
    }

    #[test]
    fn internal_test_origin_parser_keeps_loopback_explicit() {
        assert!(
            CreatorSyncEndpoints::with_sync_origin_for_test(
                PRODUCTION_ENDPOINTS.sync_origin,
                false,
            )
            .is_ok()
        );
        assert!(
            CreatorSyncEndpoints::with_sync_origin_for_test("http://127.0.0.1:39123/", true,)
                .is_ok()
        );
        assert!(
            CreatorSyncEndpoints::with_sync_origin_for_test("http://127.0.0.1:39123/", false,)
                .is_err()
        );
        assert!(
            CreatorSyncEndpoints::with_sync_origin_for_test("https://user@example.com/", false,)
                .is_err()
        );
        assert!(
            CreatorSyncEndpoints::with_sync_origin_for_test("https://example.com/v1", false)
                .is_err()
        );
        assert!(CreatorSyncEndpoints::with_sync_origin_for_test("https://[::1]/", false).is_err());
        assert!(
            CreatorSyncEndpoints::with_sync_origin_for_test("https://example.com:443/", false,)
                .is_err()
        );
    }

    #[test]
    fn authorize_builder_has_exactly_seven_unique_keys_and_no_redirect() {
        let endpoints = CreatorSyncEndpoints::production();
        assert!(
            endpoints
                .pairing_authorize_url(super::PairingAuthorizeRequest {
                    app_version: "0.5.11",
                    callback_port: 1023,
                    code_challenge: "challenge",
                    platform: "macos",
                    state: "state",
                    label: &crate::DeviceLabel::new("小凡的 Mac 🖥️".into()).unwrap()
                })
                .is_err()
        );
        assert!(
            endpoints
                .pairing_authorize_url(super::PairingAuthorizeRequest {
                    app_version: "0.5.11",
                    callback_port: 1024,
                    code_challenge: "challenge",
                    platform: "macos",
                    state: "state",
                    label: &crate::DeviceLabel::new("小凡的 Mac 🖥️".into()).unwrap()
                })
                .is_ok()
        );
        let url = endpoints
            .pairing_authorize_url(super::PairingAuthorizeRequest {
                app_version: "0.5.11",
                callback_port: 49152,
                code_challenge: "challenge",
                platform: "macos",
                state: "state",
                label: &crate::DeviceLabel::new("小凡的 Mac 🖥️".into()).unwrap(),
            })
            .unwrap();
        let parsed = Url::parse(&url).unwrap();
        assert_eq!(
            parsed.origin().ascii_serialization(),
            PRODUCTION_ENDPOINTS.access_origin
        );
        assert_eq!(parsed.path(), "/pairing/authorize");
        let pairs = parsed.query_pairs().collect::<Vec<_>>();
        assert_eq!(pairs.len(), 7);
        assert_eq!(
            pairs.iter().find(|(key, _)| key == "label").unwrap().1,
            "小凡的 Mac 🖥️"
        );
        assert!(authorize_query_is_exact(&parsed));
        assert_eq!(
            pairs
                .iter()
                .filter(|(key, _)| key == "code_challenge_method")
                .map(|(_, value)| value.as_ref())
                .collect::<Vec<_>>(),
            vec!["S256"]
        );
        assert!(!pairs.iter().any(|(key, _)| key == "redirect_uri"));

        let mut missing = parsed.clone();
        missing
            .query_pairs_mut()
            .clear()
            .extend_pairs(pairs.iter().filter(|(key, _)| key != "state").cloned());
        assert!(!authorize_query_is_exact(&missing));

        let mut duplicate = parsed.clone();
        duplicate
            .query_pairs_mut()
            .append_pair("state", "duplicate");
        assert!(!authorize_query_is_exact(&duplicate));

        let mut extra = parsed.clone();
        extra.query_pairs_mut().append_pair("extra", "value");
        assert!(!authorize_query_is_exact(&extra));

        let mut redirect = parsed;
        redirect
            .query_pairs_mut()
            .append_pair("redirect_uri", "http://127.0.0.1/");
        assert!(!authorize_query_is_exact(&redirect));
    }

    #[test]
    fn web_cli_service_and_platform_sources_do_not_duplicate_pairing_origins() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap();
        for relative in [
            "apps/web/src",
            "crates/onecrew/src",
            "crates/local-service/src",
            "crates/platform-runtime/src",
        ] {
            assert_no_origin_literals(&workspace.join(relative));
        }
    }

    fn assert_no_origin_literals(path: &Path) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                assert_no_origin_literals(&path);
                continue;
            }
            let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if !matches!(extension, "rs" | "ts" | "tsx" | "json" | "toml") {
                continue;
            }
            let source = fs::read_to_string(&path).unwrap();
            assert!(!source.contains(PRODUCTION_ENDPOINTS.access_origin));
            assert!(!source.contains(PRODUCTION_ENDPOINTS.sync_origin));
            assert!(!source.contains("PRODUCTION_ENDPOINTS"));
        }
    }
}
