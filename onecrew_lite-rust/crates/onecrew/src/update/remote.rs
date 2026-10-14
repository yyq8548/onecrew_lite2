//! Explicit updater downloads; release metadata is shared with read-only service status.
use onecrew_access_client::CliReleaseClient;
pub(super) use onecrew_access_client::{
    CliRelease as Release, CliReleaseAsset as Asset, parse_cli_version as version,
    valid_cli_digest as valid_digest,
};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use tokio::io::AsyncWriteExt;
use url::Url;

pub(super) struct Remote {
    client: Client,
    metadata: CliReleaseClient,
}
pub(super) enum Download {
    Complete,
    ReleaseChanged,
}

impl Remote {
    pub(super) fn production() -> Result<Self, String> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|_| "update_transport_unavailable")?;
        let metadata = CliReleaseClient::new(onecrew_creator_sync::production_access_origin())
            .map_err(String::from)?;
        Ok(Self { client, metadata })
    }

    pub(super) async fn latest(&self, bearer: &str, target: &str) -> Result<Release, String> {
        self.metadata
            .latest(bearer, target, Duration::from_secs(30))
            .await
            .map_err(String::from)
    }

    fn asset_url(&self, version: &str, asset: &Asset) -> Result<Url, String> {
        self.metadata
            .asset_url(version, asset)
            .map_err(String::from)
    }

    pub(super) async fn download(
        &self,
        release: &Release,
        bearer: &str,
        path: &Path,
    ) -> Result<Download, String> {
        let mut response = self
            .client
            .get(self.asset_url(&release.version, &release.asset)?)
            .bearer_auth(bearer)
            .header("Cache-Control", "no-store")
            .send()
            .await
            .map_err(|_| "update_download_network_failed")?;
        if response.status() == StatusCode::CONFLICT {
            return Ok(Download::ReleaseChanged);
        }
        check_status(response.status())?;
        if response
            .content_length()
            .is_some_and(|size| size != release.asset.size)
        {
            return Err("update_size_mismatch".into());
        }
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|_| "update_staging_failed")?;
        let mut size = 0u64;
        let mut hash = Sha256::new();
        while let Some(bytes) = response
            .chunk()
            .await
            .map_err(|_| "update_download_network_failed")?
        {
            size = size
                .checked_add(bytes.len() as u64)
                .ok_or("update_size_mismatch")?;
            if size > release.asset.size {
                return Err("update_size_mismatch".into());
            }
            hash.update(&bytes);
            file.write_all(&bytes)
                .await
                .map_err(|_| "update_staging_failed")?;
        }
        if size != release.asset.size {
            return Err("update_size_mismatch".into());
        }
        if format!("{:x}", hash.finalize()) != release.asset.sha256 {
            return Err("update_hash_mismatch".into());
        }
        file.sync_all().await.map_err(|_| "update_staging_failed")?;
        Ok(Download::Complete)
    }
}
fn check_status(status: StatusCode) -> Result<(), String> {
    match status {
        StatusCode::OK => Ok(()),
        StatusCode::UNAUTHORIZED => Err("update_login_required".into()),
        StatusCode::FORBIDDEN => Err("update_access_denied".into()),
        _ => Err("update_http_rejected".into()),
    }
}

#[cfg(test)]
mod tests;
