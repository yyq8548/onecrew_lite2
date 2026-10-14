//! Shared, fresh, authenticated Access CLI release metadata. No download or installation.
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;
use url::Url;

pub mod mail;

const MAX_MANIFEST_BYTES: usize = 16 * 1024;
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliReleaseAsset {
    pub target: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    product: String,
    version: String,
    assets: Vec<CliReleaseAsset>,
}

pub struct CliRelease {
    pub version: String,
    pub asset: CliReleaseAsset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliReleaseError {
    TransportUnavailable,
    VersionInvalid,
    AssetInvalid,
    AssetUrlInvalid,
    TargetUnavailable,
    NetworkFailed,
    Timeout,
    ManifestTooLarge,
    ManifestInvalid,
    LoginRequired,
    AccessDenied,
    HttpRejected,
}

impl CliReleaseError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::TransportUnavailable => "update_transport_unavailable",
            Self::VersionInvalid => "update_version_invalid",
            Self::AssetInvalid => "update_asset_invalid",
            Self::AssetUrlInvalid => "update_asset_url_invalid",
            Self::TargetUnavailable => "update_target_unavailable",
            Self::NetworkFailed => "update_manifest_network_failed",
            Self::Timeout => "update_manifest_timeout",
            Self::ManifestTooLarge => "update_manifest_too_large",
            Self::ManifestInvalid => "update_manifest_invalid",
            Self::LoginRequired => "update_login_required",
            Self::AccessDenied => "update_access_denied",
            Self::HttpRejected => "update_http_rejected",
        }
    }
}

impl From<CliReleaseError> for String {
    fn from(error: CliReleaseError) -> Self {
        error.code().to_owned()
    }
}

pub fn parse_cli_version(value: &str) -> Result<[u64; 3], CliReleaseError> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || !part.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(CliReleaseError::VersionInvalid);
    }
    Ok([
        parts[0]
            .parse()
            .map_err(|_| CliReleaseError::VersionInvalid)?,
        parts[1]
            .parse()
            .map_err(|_| CliReleaseError::VersionInvalid)?,
        parts[2]
            .parse()
            .map_err(|_| CliReleaseError::VersionInvalid)?,
    ])
}

pub fn valid_cli_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub struct CliReleaseClient {
    client: Client,
    origin: Url,
}

impl CliReleaseClient {
    pub fn new(origin: &str) -> Result<Self, CliReleaseError> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| CliReleaseError::TransportUnavailable)?;
        let origin = Url::parse(origin).map_err(|_| CliReleaseError::TransportUnavailable)?;
        if origin.scheme() != "https"
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(CliReleaseError::TransportUnavailable);
        }
        Ok(Self { client, origin })
    }

    /// Always fetches fresh metadata. Callers own any explicit status-only cache.
    pub async fn latest(
        &self,
        bearer: &str,
        target: &str,
        timeout: Duration,
    ) -> Result<CliRelease, CliReleaseError> {
        let endpoint = self
            .origin
            .join("/api/cli/update")
            .map_err(|_| CliReleaseError::TransportUnavailable)?;
        let mut response = self
            .client
            .get(endpoint)
            .bearer_auth(bearer)
            .header("Cache-Control", "no-store")
            .header("Accept", "application/json")
            .timeout(timeout)
            .send()
            .await
            .map_err(network_failure)?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED => return Err(CliReleaseError::LoginRequired),
            StatusCode::FORBIDDEN => return Err(CliReleaseError::AccessDenied),
            _ => return Err(CliReleaseError::HttpRejected),
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_MANIFEST_BYTES as u64)
        {
            return Err(CliReleaseError::ManifestTooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(network_failure)? {
            if bytes.len() + chunk.len() > MAX_MANIFEST_BYTES {
                return Err(CliReleaseError::ManifestTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        self.decode(&bytes, target)
    }

    fn decode(&self, bytes: &[u8], target: &str) -> Result<CliRelease, CliReleaseError> {
        let manifest: Manifest =
            serde_json::from_slice(bytes).map_err(|_| CliReleaseError::ManifestInvalid)?;
        if manifest.schema_version != 1
            || manifest.product != "onecrew_lite-cli"
            || manifest.assets.is_empty()
            || manifest.assets.len() > 3
        {
            return Err(CliReleaseError::ManifestInvalid);
        }
        parse_cli_version(&manifest.version)?;
        let mut targets = std::collections::BTreeSet::new();
        for asset in &manifest.assets {
            if !targets.insert(&asset.target)
                || !matches!(
                    asset.target.as_str(),
                    "aarch64-apple-darwin" | "x86_64-apple-darwin" | "x86_64-pc-windows-msvc"
                )
                || asset.size == 0
                || asset.size > MAX_BINARY_BYTES
                || !valid_cli_digest(&asset.sha256)
            {
                return Err(CliReleaseError::AssetInvalid);
            }
            self.asset_url(&manifest.version, asset)?;
        }
        let asset = manifest
            .assets
            .into_iter()
            .find(|asset| asset.target == target)
            .ok_or(CliReleaseError::TargetUnavailable)?;
        Ok(CliRelease {
            version: manifest.version,
            asset,
        })
    }

    pub fn asset_url(
        &self,
        version: &str,
        asset: &CliReleaseAsset,
    ) -> Result<Url, CliReleaseError> {
        let url = Url::parse(&asset.url).map_err(|_| CliReleaseError::AssetUrlInvalid)?;
        if url.origin() != self.origin.origin()
            || url.scheme() != self.origin.scheme()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != format!("/api/cli/update/{version}/files/{}", asset.target)
        {
            return Err(CliReleaseError::AssetUrlInvalid);
        }
        Ok(url)
    }
}

fn network_failure(error: reqwest::Error) -> CliReleaseError {
    if error.is_timeout() {
        CliReleaseError::Timeout
    } else {
        CliReleaseError::NetworkFailed
    }
}

#[cfg(test)]
mod tests;
