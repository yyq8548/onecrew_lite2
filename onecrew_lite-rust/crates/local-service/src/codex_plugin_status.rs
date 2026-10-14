//! Explicit, read-only Codex plugin check requested by a person in the Web UI.
//! This reads Codex's local plugin list; it never enables a plugin or grants
//! OneCrew access to the Codex account or model.

pub(crate) async fn refresh(
    state: &crate::ServiceState,
) -> Result<onecrew_codex::PluginStatus, String> {
    let executable = state.executable.clone();
    let data_dir = state.data_dir.clone();
    crate::blocking::run_web_blocking("codex_plugin_status_refresh", move || {
        let layout = crate::cli_installation::UserInstallLayout::current()?;
        let target = crate::cli_installation::current_cli_target()?;
        let mut expected = vec![onecrew_codex::expected_plugin_version(
            &executable,
            target,
            Some(&data_dir),
        )?];
        // Default and explicitly bound default data roots are equivalent; an
        // isolated service accepts only its actual bound root's generation.
        if data_dir == crate::paths::default_data_dir()? {
            expected.push(onecrew_codex::expected_plugin_version(
                &executable,
                target,
                None,
            )?);
        }
        onecrew_codex::refresh_plugin_status_for_generations(
            &layout.codex_plugin(),
            None,
            &expected,
        )
    })
    .await?
}
