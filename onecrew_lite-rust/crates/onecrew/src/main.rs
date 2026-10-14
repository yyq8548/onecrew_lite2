//! Thin onecrew_lite command-line adapter over the shared local service.
use onecrew_local_service::{
    ServiceClient, ServiceOptions, default_data_dir, ensure_service, run_service,
};
use serde_json::Value;
use std::{ffi::OsString, path::PathBuf, process::ExitCode, time::Duration};
use tokio::io::AsyncWriteExt;

mod assets {
    include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));
}
mod login_guidance;
mod plugin_commands;
mod update;
mod workspace;

const HELP: &str = "onecrew_lite — local Creator webpage and MCP\n\n\
Usage: onecrew_lite <command> [options]\n\n\
  login [--no-browser]  Sign in with the system browser, or print an authorization URL\n\
  status                Show service, data and validated login status\n\
  logout                Remove the local Access credential; preserve Creator data\n\
  web [--no-browser]    Open the local webpage, or print its address to open yourself\n\
  workspace [--no-browser]  Open workbench and bundled Library in two browser tabs\n\
  mcp --stdio           Serve MCP over stdin/stdout, sharing the local service\n\
  serve [--no-browser]  Start/reuse the service, open the webpage and show next actions\n\
  serve --foreground   Run the service in the foreground for diagnostics\n\
  stop                  Drain work and stop the shared service; preserve data\n\
  service host install  Register the Windows current-user service host; does not start it\n\
  service host remove   Remove that registration; does not stop the running service\n\
  update                Explicitly download, verify and replace this CLI; never installs plugins\n\
  data status           Inspect local data versions without upgrading them\n\
  opencli repair        Install/verify the managed compatible search runtime\n\
  plugin install        Explicitly install/update the self-contained Codex plugin\n\
  plugin status         Read last-verified cache; --refresh queries Codex explicitly\n\
  plugin package        Package a binary: --target <triple> --output <directory> [--binary <path>]\n\
  setup [--copy]        Print/copy standalone MCP JSON for other clients; never installs\n\n\
Options: --data-dir <path> (explicit isolated data), --json, --version, --help\n\
  --codex <path>        Drive this Codex CLI for plugin install/status --refresh\n\
                        instead of searching PATH; must be an existing file\n\n\
To choose the browser yourself, use web --no-browser and open the printed URL.\n\
Codex: install the plugin; Codex starts its bundled MCP. Other clients: use setup.\n\
Login prints the two connection choices and the optional webpage command.\n";

struct Arguments {
    command: String,
    data_dir: PathBuf,
    explicit_data_dir: bool,
    no_browser: bool,
    json: bool,
    foreground: bool,
    refresh: bool,
    copy: bool,
    package_binary: Option<PathBuf>,
    package_target: Option<String>,
    package_output: Option<PathBuf>,
    install_path: Option<PathBuf>,
    codex_cli: Option<PathBuf>,
    expected_version: Option<String>,
    expected_sha256: Option<String>,
    expected_size: Option<u64>,
}

fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Arguments, String> {
    let mut arguments = arguments.into_iter();
    let mut words = Vec::new();
    let mut data_dir = None;
    let mut no_browser = false;
    let mut json = false;
    let mut stdio = false;
    let mut foreground = false;
    let mut refresh = false;
    let mut copy = false;
    let mut package_binary = None;
    let mut package_target = None;
    let mut package_output = None;
    let mut install_path = None;
    let mut codex_cli = None;
    let mut expected_version = None;
    let mut expected_sha256 = None;
    let mut expected_size = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--install-path") => {
                install_path = Some(PathBuf::from(
                    arguments.next().ok_or("--install-path requires a path")?,
                ))
            }
            Some("--expected-version") => {
                expected_version = Some(
                    arguments
                        .next()
                        .ok_or("--expected-version requires a value")?
                        .into_string()
                        .map_err(|_| "invalid expected version")?,
                )
            }
            Some("--expected-sha256") => {
                expected_sha256 = Some(
                    arguments
                        .next()
                        .ok_or("--expected-sha256 requires a value")?
                        .into_string()
                        .map_err(|_| "invalid expected hash")?,
                )
            }
            Some("--expected-size") => {
                expected_size = Some(
                    arguments
                        .next()
                        .ok_or("--expected-size requires a value")?
                        .into_string()
                        .map_err(|_| "invalid expected size")?
                        .parse()
                        .map_err(|_| "invalid expected size")?,
                )
            }
            Some("--codex") => {
                codex_cli = Some(PathBuf::from(
                    arguments.next().ok_or("--codex requires a path")?,
                ))
            }
            Some("--data-dir") => {
                data_dir = Some(PathBuf::from(
                    arguments.next().ok_or("--data-dir requires a path")?,
                ));
            }
            Some("--no-browser") => no_browser = true,
            Some("--json") => json = true,
            Some("--stdio") => stdio = true,
            Some("--foreground") => foreground = true,
            Some("--refresh") => refresh = true,
            Some("--copy") => copy = true,
            Some("--binary") => {
                package_binary = Some(PathBuf::from(
                    arguments.next().ok_or("--binary requires a path")?,
                ))
            }
            Some("--target") => {
                package_target = Some(
                    arguments
                        .next()
                        .ok_or("--target requires a triple")?
                        .into_string()
                        .map_err(|_| "invalid target")?,
                )
            }
            Some("--output") => {
                package_output = Some(PathBuf::from(
                    arguments.next().ok_or("--output requires a path")?,
                ))
            }
            Some("--help" | "-h") => words = vec!["help".into()],
            Some("--version" | "-V") => words = vec!["version".into()],
            Some(value) if !value.starts_with('-') => words.push(value.to_owned()),
            _ => return Err("unknown command option; run onecrew_lite --help".into()),
        }
    }
    let command = if words.is_empty() {
        "help".into()
    } else {
        words.join(" ")
    };
    if ![
        "help",
        "version",
        "login",
        "logout",
        "status",
        "web",
        "workspace",
        "mcp",
        "serve",
        "stop",
        "update",
        "__bootstrap-update",
        "data status",
        "opencli repair",
        "plugin install",
        "plugin status",
        "plugin package",
        "setup",
        "__service",
        "__service-host",
        "__service-preflight",
        "service host install",
        "service host remove",
    ]
    .contains(&command.as_str())
    {
        return Err("unknown command; run onecrew_lite --help".into());
    }
    if (command == "mcp") != stdio {
        return Err("MCP requires exactly onecrew_lite mcp --stdio".into());
    }
    if no_browser && !["login", "web", "serve", "workspace"].contains(&command.as_str()) {
        return Err("--no-browser applies to login, web, serve or workspace".into());
    }
    if foreground && command != "serve" {
        return Err("--foreground applies to serve".into());
    }
    if refresh && command != "plugin status" {
        return Err("--refresh applies to plugin status".into());
    }
    if copy && command != "setup" {
        return Err("--copy applies to setup".into());
    }
    // A cached read never drives the CLI, so naming one there would be a
    // silently ignored argument.
    if codex_cli.is_some()
        && command != "plugin install"
        && !(command == "plugin status" && refresh)
    {
        return Err("--codex applies to plugin install or plugin status --refresh".into());
    }
    if command != "plugin package"
        && (package_binary.is_some() || package_target.is_some() || package_output.is_some())
    {
        return Err("--binary/--target/--output apply to plugin package".into());
    }
    if command != "__bootstrap-update"
        && (install_path.is_some()
            || expected_version.is_some()
            || expected_sha256.is_some()
            || expected_size.is_some())
    {
        return Err("bootstrap options require the authenticated installer handoff".into());
    }
    let explicit_data_dir = data_dir.is_some();
    let data_dir = data_dir.map(Ok).unwrap_or_else(default_data_dir)?;
    let data_dir = if data_dir.is_absolute() {
        data_dir
    } else {
        std::env::current_dir()
            .map_err(|_| "working directory unavailable")?
            .join(data_dir)
    };
    Ok(Arguments {
        command,
        data_dir,
        explicit_data_dir,
        no_browser,
        json,
        foreground,
        refresh,
        copy,
        codex_cli,
        package_binary,
        package_target,
        package_output,
        install_path,
        expected_version,
        expected_sha256,
        expected_size,
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("onecrew_lite: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let arguments = parse(std::env::args_os().skip(1))?;
    if arguments.command == "help" {
        print!("{HELP}");
        return Ok(());
    }
    if arguments.command == "version" {
        println!("onecrew_lite {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|_| "executable path unavailable")?;
    if arguments.command == "service host install" || arguments.command == "service host remove" {
        let action = if arguments.command.ends_with("install") {
            "install"
        } else {
            "remove"
        };
        onecrew_local_service::configure_service_host(&executable, &arguments.data_dir, action)?;
        return Ok(());
    }
    if arguments.command == "__service-preflight" {
        onecrew_local_service::service_start_preflight(&executable)?;
        print_json(&serde_json::json!({"independent_start": true}));
        return Ok(());
    }
    if arguments.command == "update" || arguments.command == "__bootstrap-update" {
        let outcome = update::execute(&arguments, &executable).await?;
        print_json(&serde_json::to_value(outcome).map_err(|_| "update_result_encoding_failed")?);
        return Ok(());
    }
    if arguments.command == "setup" || arguments.command.starts_with("plugin ") {
        return plugin_commands::execute(&arguments, &executable).await;
    }
    if arguments.command == "opencli repair" {
        onecrew_opencli::repair_managed_installation(true).await?;
        let cli = onecrew_opencli::OpenCli::inspect_system()
            .await
            .map_err(|_| "opencli_repair_readback_failed")?;
        let addons = cli
            .repair_bundled_addons(true)
            .await
            .map_err(|_| "opencli_addon_install_failed")?;
        let bridge = cli.recover_bridge().await;
        print_json(&serde_json::json!({
            "installed": true, "version": cli.inventory().version,
            "command_path": cli.command_path(), "addons": addons,
            "daemon_running": bridge.as_ref().is_ok_and(|value| value.daemon_running),
            "extension_connected": bridge.as_ref().is_ok_and(|value| value.extension_connected),
            "connection_check_passed": bridge.is_ok(),
        }));
        return Ok(());
    }
    if arguments.command == "__service-host" {
        return onecrew_local_service::run_service_host(&executable, &arguments.data_dir).await;
    }
    if arguments.command == "__service" || (arguments.command == "serve" && arguments.foreground) {
        return run_service(ServiceOptions {
            data_dir: arguments.data_dir,
            executable,
            web_assets: assets::WEB_ASSETS,
        })
        .await;
    }
    if arguments.command == "stop" {
        let client = ServiceClient::existing(&arguments.data_dir).await?;
        print_json(&client.request("stop").await?);
        return Ok(());
    }
    // Status can inspect an older running binary without silently replacing it.
    let client = if arguments.command == "status" {
        ServiceClient::existing(&arguments.data_dir).await?
    } else {
        ensure_service(arguments.data_dir.clone(), executable.clone()).await?
    };
    match arguments.command.as_str() {
        "workspace" => {
            let web = client.request("web").await?;
            let url = web
                .get("url")
                .and_then(Value::as_str)
                .ok_or("web URL unavailable")?;
            workspace::open(&executable, url, arguments.no_browser).await
        }
        "serve" => {
            let mut status = client.request("status").await?;
            let web = client.request("web").await?;
            let url = web
                .get("url")
                .and_then(Value::as_str)
                .ok_or("web URL unavailable")?;
            status["web_url"] = Value::from(url);
            print_json(&status);
            if !arguments.no_browser {
                open_browser(url.to_owned()).await?;
            }
            Ok(())
        }
        "mcp" => bridge_stdio(client).await,
        "login" => {
            login(
                client,
                arguments.no_browser,
                arguments.json,
                &executable,
                arguments
                    .explicit_data_dir
                    .then_some(arguments.data_dir.as_path()),
            )
            .await
        }
        "web" => {
            let value = client.request("web").await?;
            let url = value
                .get("url")
                .and_then(Value::as_str)
                .ok_or("web URL unavailable")?;
            if !arguments.no_browser {
                open_browser(url.to_owned()).await?;
            }
            if arguments.json {
                print_json(&value);
            } else {
                println!("{url}");
            }
            Ok(())
        }
        "logout" | "status" => {
            print_json(&client.request(&arguments.command).await?);
            Ok(())
        }
        "data status" => {
            print_json(&client.request("data-status").await?);
            Ok(())
        }
        _ => Err("unknown command".into()),
    }
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".into())
    );
}

async fn login(
    client: ServiceClient,
    no_browser: bool,
    json: bool,
    executable: &std::path::Path,
    data_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    let status = client.request("status").await?;
    if status
        .pointer("/login/status/state")
        .and_then(Value::as_str)
        == Some("active")
    {
        return print_login_complete(&client, json, executable, data_dir).await;
    }
    let begun = client.request("login").await?;
    let url = begun
        .get("authorize_url")
        .and_then(Value::as_str)
        .ok_or("authorization URL unavailable")?;
    let mut manual_browser = no_browser;
    if !no_browser {
        match open_browser(url.to_owned()).await {
            Ok(()) => {
                client.request("browser-opened").await?;
            }
            Err(error) => {
                eprintln!("onecrew_lite: {error}; open the URL below manually");
                manual_browser = true;
            }
        }
    }
    // Progress is not the result. In particular, `begun` contains the snapshot
    // from before browser launch and must never be returned as current state.
    // Keep stdout reserved for one terminal result in JSON mode.
    if manual_browser {
        eprintln!("{url}");
    }
    eprintln!("Waiting for browser login. Press Ctrl-C to cancel.");
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                let outcome = client.request("login-cancel").await?;
                if outcome.get("outcome").and_then(Value::as_str) == Some("completed_before_cancellation") {
                    return print_login_complete(&client, json, executable, data_dir).await;
                }
                print_json(&outcome);
                return Err("login cancelled".into());
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                let status = client.request("login-status").await?;
                match status.pointer("/status/state").and_then(Value::as_str) {
                    Some("active") => return print_login_complete(&client, json, executable, data_dir).await,
                    Some("pairing_pending" | "pairing_completing") => (),
                    _ => { print_json(&status); return Err("login did not complete; inspect the status above".into()); }
                }
            }
        }
    }
}

async fn print_login_complete(
    client: &ServiceClient,
    json: bool,
    executable: &std::path::Path,
    data_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    let status = client.request("status").await?;
    let guidance = login_guidance::response(&status, executable, data_dir)?;
    if json {
        print_json(&guidance);
    } else {
        login_guidance::print(&guidance);
    }
    Ok(())
}

async fn bridge_stdio(client: ServiceClient) -> Result<(), String> {
    let stream = client.connect_mcp().await?;
    let (mut read, mut write) = tokio::io::split(stream);
    // Tokio stdin uses an uncancellable blocking-pool read, which can prevent
    // runtime shutdown when the service closes while Codex keeps stdin open.
    // This bounded transport-only thread belongs to the proxy process; it
    // cannot own or outlive a business operation in the separate service.
    let (send, mut receive) = tokio::sync::mpsc::channel::<Result<Vec<u8>, &'static str>>(8);
    std::thread::Builder::new()
        .name("onecrew-stdin".into())
        .spawn(move || {
            use std::io::Read;
            let mut stdin = std::io::stdin().lock();
            loop {
                let mut buffer = vec![0_u8; 16 * 1024];
                match stdin.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        buffer.truncate(count);
                        if send.blocking_send(Ok(buffer)).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        // The failure reaches the receiver while one is still
                        // listening; the transfer stops either way.
                        if send
                            .blocking_send(Err("MCP stdin transfer failed"))
                            .is_err()
                        {
                            eprintln!(
                                "onecrew_lite: MCP stdin transfer failed after the transport closed"
                            );
                        }
                        break;
                    }
                }
            }
        })
        .map_err(|_| "MCP stdin reader unavailable")?;
    let input = async {
        while let Some(bytes) = receive.recv().await {
            write
                .write_all(&bytes?)
                .await
                .map_err(|_| "MCP input transfer failed")?;
        }
        write
            .shutdown()
            .await
            .map_err(|_| "MCP input shutdown failed")
    };
    let output = async {
        let mut stdout = tokio::io::stdout();
        tokio::io::copy(&mut read, &mut stdout)
            .await
            .map_err(|_| "MCP stdout transfer failed")?;
        stdout.flush().await.map_err(|_| "MCP stdout flush failed")
    };
    tokio::pin!(input, output);
    tokio::select! {
        result = &mut input => { result.map_err(str::to_owned)?; output.await.map_err(str::to_owned) },
        result = &mut output => result.map_err(str::to_owned),
    }
}

async fn open_browser(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = tokio::process::Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = tokio::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler").arg(url);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut command = tokio::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    let status = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|_| "system browser could not be opened")?;
    if status.success() {
        Ok(())
    } else {
        Err("system browser could not be opened".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    #[test]
    fn protocol_mode_and_cli_only_switches_are_explicit() {
        assert!(parse(args(&["mcp"])).is_err());
        assert!(parse(args(&["status", "--stdio"])).is_err());
        assert!(parse(args(&["mcp", "--stdio", "--no-browser"])).is_err());
        assert!(parse(args(&["mcp", "--stdio"])).is_ok());
        assert!(parse(args(&["data", "upgrade"])).is_err());
        assert!(
            parse(args(&[
                "login",
                "--no-browser",
                "--data-dir",
                "/tmp/onecrew_lite test"
            ]))
            .unwrap()
            .no_browser
        );
    }

    #[test]
    fn naming_the_codex_cli_is_only_accepted_where_it_is_driven() {
        let install = parse(args(&["plugin", "install", "--codex", "/opt/codex"])).unwrap();
        assert_eq!(install.codex_cli, Some(PathBuf::from("/opt/codex")));
        assert!(
            parse(args(&[
                "plugin",
                "status",
                "--refresh",
                "--codex",
                "/opt/codex"
            ]))
            .is_ok()
        );
        // A cached read never starts Codex, so the argument would be ignored.
        assert!(parse(args(&["plugin", "status", "--codex", "/opt/codex"])).is_err());
        assert!(parse(args(&["setup", "--codex", "/opt/codex"])).is_err());
        assert!(parse(args(&["plugin", "install", "--codex"])).is_err());
        assert_eq!(parse(args(&["plugin", "install"])).unwrap().codex_cli, None);
    }
}
