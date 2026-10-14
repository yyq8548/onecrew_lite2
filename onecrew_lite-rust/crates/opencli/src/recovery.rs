//! Serialized local daemon recovery, independent of platform login.
use super::{OpenCli, OpenCliBridgeStatus, OpenCliError, PROBE_TIMEOUT, run};
use std::time::Duration;

static RECOVERY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) async fn recover(cli: &OpenCli) -> Result<OpenCliBridgeStatus, OpenCliError> {
    tokio::time::timeout(Duration::from_secs(25), recover_bounded(cli))
        .await
        .map_err(|_| OpenCliError::Timeout)?
}

async fn recover_bounded(cli: &OpenCli) -> Result<OpenCliBridgeStatus, OpenCliError> {
    // The async guard spans the bounded restart/readback so two windows cannot
    // restart a daemon that the other window has just started. No data lock is held.
    let _guard = RECOVERY.lock().await;
    cli.require_version().await?;
    let mut status = cli.bridge_status().await?;
    if status.daemon_running {
        return Ok(status);
    }
    run(
        &cli.executable,
        cli.script.as_deref(),
        &["daemon", "restart"],
        PROBE_TIMEOUT,
    )
    .await?;
    // Extension reconnection is asynchronous. Only the actual status may prove
    // success; the restart command's output is not a readiness receipt.
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        status = cli.bridge_status().await?;
        if status.daemon_running && status.extension_connected {
            break;
        }
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    #[ignore = "requires Node.js; fake local daemon, no provider reads"]
    async fn recovery_starts_once_and_never_claims_an_extension_or_restarts_healthy_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("bridge.cjs");
        std::fs::write(&script, r#"
const fs = require('fs');
const marker = __filename + '.started';
const args = process.argv.slice(2).join(' ');
if (args === '--version') console.log('1.8.8');
else if (args === 'list -f json') console.log(JSON.stringify([{command:'youtube/search',site:'youtube',name:'search',access:'read',browser:true}]));
else if (args === 'daemon status') console.log(fs.existsSync(marker) ? 'Daemon: running\nExtension: disconnected' : 'Daemon: not running');
else if (args === 'daemon restart') {
  if (fs.existsSync(marker)) process.exit(91);
  fs.writeFileSync(marker, '1');
  console.log('Daemon started');
} else process.exit(90);
"#).unwrap();
        let cli = OpenCli::inspect_with_script(PathBuf::from("node"), Some(script))
            .await
            .unwrap();
        let (first, second) = tokio::join!(cli.recover_bridge(), cli.recover_bridge());
        for result in [first, second] {
            let status = result.unwrap();
            assert!(status.daemon_running);
            assert!(!status.extension_connected);
        }
    }
}
