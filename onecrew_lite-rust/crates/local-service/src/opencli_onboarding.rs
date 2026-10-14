//! Secret-free OpenCLI checks and explicitly approved bundled add-on setup.
use crate::ServiceState;
use onecrew_local_runtime::{
    ChromeSetupPage, OpenCliAddonStatus, OpenCliAuthObservation, OpenCliSetupReport,
};
use serde_json::{Value, json};

pub(crate) async fn auth_observations(state: &ServiceState, profile: String) -> Result<Vec<OpenCliAuthObservation>, String> {
    state.platform().await?.opencli_auth_observations(&profile)
}

pub(crate) async fn status(state: &ServiceState) -> Result<OpenCliSetupReport, String> {
    Ok(state.platform().await?.opencli_setup_report().await)
}

pub(crate) async fn recover(state: &ServiceState) -> Result<OpenCliSetupReport, String> {
    state
        .platform()
        .await?
        .recover_opencli_bridge()
        .await
        .map_err(|_| "opencli_bridge_recovery_failed".to_owned())
}

pub(crate) async fn repair(
    state: &ServiceState,
    approved: bool,
) -> Result<OpenCliSetupReport, String> {
    state
        .platform()
        .await?
        .repair_opencli_installation(approved)
        .await
}

pub(crate) async fn open_setup_page(
    state: &ServiceState,
    page: ChromeSetupPage,
) -> Result<Value, String> {
    state
        .platform()
        .await?
        .open_opencli_setup_page(page)
        .await?;
    Ok(json!({"page": page, "browser": "chrome", "launch_requested": true}))
}

pub(crate) async fn open_login_page(
    state: &ServiceState,
    profile: String,
    platform: String,
) -> Result<Value, String> {
    state
        .platform()
        .await?
        .open_opencli_login_page(&profile, &platform)
        .await
        .map_err(|_| "opencli_login_page_failed".to_owned())?;
    Ok(json!({"profile": profile, "platform": platform, "opened": true}))
}

pub(crate) async fn install_addons(
    state: &ServiceState,
    approved: bool,
) -> Result<Vec<OpenCliAddonStatus>, String> {
    state
        .platform()
        .await?
        .install_opencli_addons(approved)
        .await
        .map_err(|_| "opencli_addon_install_failed".to_owned())
}

pub(crate) async fn check_profile(state: &ServiceState, profile: String) -> Result<Value, String> {
    state
        .platform()
        .await?
        .check_opencli_browser_profile(&profile)
        .await
        .map_err(|_| "opencli_profile_check_failed".to_owned())?;
    Ok(json!({"profile": profile, "preflight_passed": true}))
}

pub(crate) async fn check_site_auth(
    state: &ServiceState,
    profile: String,
    platform: String,
) -> Result<OpenCliAuthObservation, String> {
    state
        .platform()
        .await?
        .check_opencli_site_auth(&profile, &platform)
        .await
}
