use onecrew_local_runtime::{ProxyConfig, ProxyProbe};

use crate::{ServiceState, blocking::run_web_blocking};

pub(super) async fn set_profile_proxy(
    state: &ServiceState,
    profile_id: String,
    proxy: ProxyConfig,
) -> Result<(), String> {
    state
        .platform()
        .await?
        .set_profile_proxy(&profile_id, proxy)
        .await
}

pub(super) async fn clear_profile_proxy(
    state: &ServiceState,
    profile_id: String,
) -> Result<(), String> {
    let platform = state.platform().await?;
    run_web_blocking("clear_profile_proxy", move || {
        platform.clear_profile_proxy(&profile_id)
    })
    .await?
}

/// Config-time check: probe an unsaved proxy and report its egress IP so the
/// user can confirm it works before hitting 保存.
pub(super) async fn probe_proxy(
    state: &ServiceState,
    proxy: ProxyConfig,
) -> Result<ProxyProbe, String> {
    state.platform().await?.probe_proxy(proxy).await
}
