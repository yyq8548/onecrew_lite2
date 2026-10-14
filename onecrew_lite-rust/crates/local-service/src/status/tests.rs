use super::*;

#[test]
fn normal_startup_summary_does_not_replay_historical_upgrade_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let current = onecrew_local_runtime::LocalPlatform::preflight_data_dir(directory.path());
    let mut historical = current.clone();
    historical.state = LocalDataState::UpgradeRequired;
    let report = crate::startup::DataStartupStatus {
        phase: crate::startup::StartupPhase::Ready,
        preflight: Some(historical),
        postflight: Some(current),
        upgrade: None,
        failure: None,
        diagnostic_code: Some("historical_diagnostic".into()),
    };
    let wire = serde_json::to_value(StartupSummary::from_report(&report)).unwrap();
    assert_eq!(wire, serde_json::json!({"phase":"ready"}));
    assert!(
        !serde_json::to_string(&current_data(&report))
            .unwrap()
            .contains("upgrade_required")
    );
    let failed = crate::startup::DataStartupStatus {
        phase: crate::startup::StartupPhase::Failed,
        diagnostic_code: Some("startup_failed".into()),
        ..report
    };
    let wire = serde_json::to_value(StartupSummary::from_report(&failed)).unwrap();
    assert_eq!(wire["diagnostic_code"], "startup_failed");
    assert!(current_data(&failed).is_none());
}

#[tokio::test]
async fn update_actions_are_instructions_for_a_verified_standalone_cli_only() {
    use crate::cli_update_status::{CliUpdateStatusOwner, UpdateIdentity};
    use onecrew_codex::{PluginStatus, Verification};
    use onecrew_creator_sync::CreatorSyncStatus;

    let login = crate::login::LoginOwner::default().projection(CreatorSyncStatus {
        state: CreatorSyncAdmissionState::Active,
        action: None,
        issue: None,
        installation_id: Some("test-installation".into()),
        credential_id: Some("test-credential".into()),
    });
    let cli_update = CliUpdateStatusOwner::default()
        .read(
            UpdateIdentity::eligible(true, Some(&login)),
            "0.5.22",
            || async { Ok("0.5.23".into()) },
        )
        .await;
    let update_wire = serde_json::to_string(&cli_update).unwrap();
    assert!(!update_wire.contains("credential"));
    assert!(!update_wire.contains("http"));
    let mut status = LocalServiceStatus {
        locale: onecrew_local_runtime::PresentationLocale::SimplifiedChinese,
        workspace_activity: None,
        workspace_history: Vec::new(),
        workspace_activity_error: None,
        version: "0.5.22".into(),
        pid: 1,
        running: true,
        transport: "stdio".into(),
        mcp_clients: 1,
        binary_sha256: "a".repeat(64),
        data: None,
        startup: StartupSummary {
            phase: crate::startup::StartupPhase::Ready,
            failure: None,
            diagnostic_code: None,
        },
        login: Some(login),
        login_error: None,
        plugin: PluginStatus {
            state: PluginState::Installed,
            verification: Verification::Cached,
            installed_version: Some("0.5.22+codex.fixture".into()),
            enabled: Some(true),
            last_verified_at: Some(1),
            diagnostic_code: None,
        },
        cli_update,
        next_actions: Vec::new(),
    };
    let source = Path::new("/home/someone/.local/share/onecrew/service-binaries/hash/onecrew");
    assert_eq!(serde_json::to_value(&status).unwrap()["locale"], "zh-CN");
    let data = Path::new("/tmp/isolated-onecrew-data");
    for context in [StatusContext::Human, StatusContext::Mcp] {
        let actions = serde_json::to_value(next_actions(
            &status,
            context,
            source,
            data,
            Some(Path::new("/verified/bin/onecrew")),
        ))
        .unwrap();
        let update = actions
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["kind"] == "update_cli")
            .unwrap();
        assert_eq!(update["command"]["executable"], "/verified/bin/onecrew");
        assert_eq!(update["command"]["args"][0], "update");
        assert!(
            update["tool"].is_null(),
            "MCP must never execute an update tool"
        );
        let instruction = update["instruction"].as_str().unwrap();
        assert!(instruction.contains("0.5.22 → 0.5.23"));
        assert!(instruction.contains("new OneCrew CLI version is available"));
        assert!(instruction.contains("inform the user"));
        assert!(instruction.contains("user explicitly requests"));
        assert!(instruction.contains("does not automatically install plugins"));
        let actions =
            serde_json::to_value(next_actions(&status, context, source, data, None)).unwrap();
        let update = actions
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["kind"] == "update_cli")
            .unwrap();
        assert!(update["command"].is_null());
        let instruction = update["instruction"].as_str().unwrap();
        assert!(instruction.contains("0.5.22 → 0.5.23"));
        assert!(instruction.contains("wait for their update preference"));
        assert!(
            update["instruction"]
                .as_str()
                .unwrap()
                .contains("Access account center")
        );
    }
    for (phase, expected) in [
        (crate::startup::StartupPhase::Inspecting, "await_startup"),
        (crate::startup::StartupPhase::Upgrading, "await_startup"),
        (crate::startup::StartupPhase::Failed, "inspect_data"),
    ] {
        status.startup.phase = phase;
        let actions = serde_json::to_value(next_actions(
            &status,
            StatusContext::Mcp,
            source,
            data,
            None,
        ))
        .unwrap();
        assert_eq!(actions.as_array().unwrap().len(), 1);
        assert_eq!(actions[0]["kind"], expected);
        assert!(
            !serde_json::to_string(&actions)
                .unwrap()
                .contains("data upgrade")
        );
    }
}

#[tokio::test]
async fn a_broken_marketplace_registration_offers_people_an_explicit_plugin_install() {
    use crate::cli_update_status::{CliUpdateStatusOwner, UpdateIdentity};
    use onecrew_codex::{PluginStatus, Verification};

    let cli_update = CliUpdateStatusOwner::default()
        .read(UpdateIdentity::eligible(false, None), "0.6.1", || async {
            Ok("0.6.1".into())
        })
        .await;
    let mut status = LocalServiceStatus {
        locale: onecrew_local_runtime::PresentationLocale::SimplifiedChinese,
        workspace_activity: None,
        workspace_history: Vec::new(),
        workspace_activity_error: None,
        version: "0.6.1".into(),
        pid: 1,
        running: true,
        transport: "stdio".into(),
        mcp_clients: 1,
        binary_sha256: "a".repeat(64),
        data: None,
        startup: StartupSummary {
            phase: crate::startup::StartupPhase::Ready,
            failure: None,
            diagnostic_code: None,
        },
        login: None,
        login_error: None,
        plugin: PluginStatus {
            state: PluginState::Installed,
            verification: Verification::Cached,
            installed_version: Some("0.6.1+codex.fixture".into()),
            enabled: Some(true),
            last_verified_at: Some(1),
            diagnostic_code: None,
        },
        cli_update,
        next_actions: Vec::new(),
    };
    let source = Path::new("/home/someone/.local/share/onecrew/service-binaries/hash/onecrew");
    let data = Path::new("/tmp/isolated-onecrew-data");
    let install_actions = |status: &LocalServiceStatus, context: StatusContext| {
        serde_json::to_value(next_actions(status, context, source, data, None))
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .filter(|action| action["kind"] == "install_plugin")
            .cloned()
            .collect::<Vec<_>>()
    };
    for context in [StatusContext::Human, StatusContext::Mcp] {
        assert!(install_actions(&status, context).is_empty());
    }
    for code in [
        "codex_marketplace_source_missing",
        "codex_marketplace_source_relocated",
    ] {
        status.plugin = PluginStatus {
            state: PluginState::Unavailable,
            verification: Verification::Cached,
            installed_version: None,
            enabled: None,
            last_verified_at: Some(1),
            diagnostic_code: Some(code.into()),
        };
        let actions = install_actions(&status, StatusContext::Human);
        assert_eq!(actions.len(), 1, "{code}");
        assert_eq!(
            actions[0]["command"]["args"],
            serde_json::json!([
                "plugin",
                "install",
                "--data-dir",
                "/tmp/isolated-onecrew-data"
            ])
        );
        let instruction = actions[0]["instruction"].as_str().unwrap();
        assert!(instruction.contains("restart Codex"), "{instruction}");
        assert!(
            instruction.contains("only when the user chooses"),
            "{instruction}"
        );
        assert!(instruction.contains("not authorization"), "{instruction}");
        // The Codex router never installs or refreshes the plugin, so a
        // connected MCP client reads plugin.diagnostic_code, not an install prompt.
        assert!(
            install_actions(&status, StatusContext::Mcp).is_empty(),
            "{code}"
        );
    }
    // Other unavailable receipts keep the prompt for people only, without guidance.
    status.plugin.diagnostic_code = Some("plugin_receipt_invalid".into());
    assert!(install_actions(&status, StatusContext::Mcp).is_empty());
    let human = install_actions(&status, StatusContext::Human);
    assert_eq!(human.len(), 1);
    assert!(human[0].get("instruction").is_none());
}
