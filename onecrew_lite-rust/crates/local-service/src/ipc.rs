//! Authenticated private process control and raw bidirectional MCP transport.
//! Discovery credentials are never returned by a control or business response.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use rmcp::ServiceExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::Arc,
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};
use zeroize::Zeroizing;

pub(crate) const PROTOCOL: u32 = 1;
pub(crate) const MAX_CONTROL_BYTES: u64 = 16 * 1024;

// Only serialized into the private discovery file. Deliberately no Debug.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Endpoint {
    pub(crate) protocol: u32,
    pub(crate) version: String,
    pub(crate) binary_id: String,
    pub(crate) pid: u32,
    pub(crate) port: u16,
    pub(crate) web_port: u16,
    pub(crate) token: String,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.token.zeroize();
    }
}

pub(crate) fn random_secret() -> String {
    let mut bytes = Zeroizing::new([0_u8; 32]);
    rand::rngs::OsRng.fill_bytes(bytes.as_mut());
    URL_SAFE_NO_PAD.encode(bytes.as_ref())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hello {
    pub(crate) protocol: u32,
    pub(crate) token: String,
    pub(crate) mode: String,
    pub(crate) command: Option<String>,
}

impl Drop for Hello {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.token.zeroize();
    }
}

pub(crate) async fn read_message<T: serde::de::DeserializeOwned>(
    stream: &mut BufReader<TcpStream>,
) -> Result<T, String> {
    read_bounded(stream, Duration::from_secs(5)).await
}

pub(crate) async fn read_reply<T: serde::de::DeserializeOwned>(
    stream: &mut BufReader<TcpStream>,
) -> Result<T, String> {
    read_bounded(stream, Duration::from_secs(120)).await
}

async fn read_bounded<T: serde::de::DeserializeOwned>(
    stream: &mut BufReader<TcpStream>,
    timeout: Duration,
) -> Result<T, String> {
    let mut line = Zeroizing::new(Vec::new());
    let count = tokio::time::timeout(
        timeout,
        stream
            .take(MAX_CONTROL_BYTES + 1)
            .read_until(b'\n', &mut line),
    )
    .await
    .map_err(|_| "control_timeout")?
    .map_err(|_| "control_unavailable")?;
    if count == 0 || count as u64 > MAX_CONTROL_BYTES || line.last() != Some(&b'\n') {
        return Err("invalid_control_message".into());
    }
    serde_json::from_slice(&line).map_err(|_| "invalid_control_message".into())
}

pub(crate) async fn write_message(
    stream: &mut TcpStream,
    value: &impl Serialize,
) -> Result<(), String> {
    let mut bytes =
        Zeroizing::new(serde_json::to_vec(value).map_err(|_| "control_encoding_failed")?);
    bytes.push(b'\n');
    stream
        .write_all(&bytes)
        .await
        .map_err(|_| "control_unavailable")?;
    stream
        .flush()
        .await
        .map_err(|_| "control_unavailable".into())
}

pub(crate) async fn connect(
    endpoint: &Endpoint,
    mode: &str,
    command: Option<&str>,
) -> Result<BufReader<TcpStream>, String> {
    if endpoint.protocol != PROTOCOL || endpoint.port == 0 {
        return Err("service_protocol_incompatible".into());
    }
    let mut stream = tokio::time::timeout(
        Duration::from_secs(2),
        TcpStream::connect(SocketAddrV4::new(Ipv4Addr::LOCALHOST, endpoint.port)),
    )
    .await
    .map_err(|_| "service_unavailable")?
    .map_err(|_| "service_unavailable")?;
    write_message(
        &mut stream,
        &Hello {
            protocol: PROTOCOL,
            token: endpoint.token.clone(),
            mode: mode.into(),
            command: command.map(str::to_owned),
        },
    )
    .await?;
    Ok(BufReader::new(stream))
}

pub(crate) async fn serve_connection(
    stream: TcpStream,
    endpoint: Arc<Endpoint>,
    state: Arc<crate::ServiceState>,
) -> Result<(), String> {
    if !stream
        .peer_addr()
        .map_err(|_| "peer_unavailable")?
        .ip()
        .is_loopback()
    {
        return Err("unauthorized_peer".into());
    }
    let mut stream = BufReader::new(stream);
    let hello: Hello = read_message(&mut stream).await?;
    if hello.protocol != PROTOCOL
        || !bool::from(hello.token.as_bytes().ct_eq(endpoint.token.as_bytes()))
    {
        return Err("unauthorized_control".into());
    }
    match hello.mode.as_str() {
        "mcp" if hello.command.is_none() => {
            write_message(stream.get_mut(), &json!({"ok":true})).await?;
            let shutdown = state.shutdown.clone();
            let running = tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                result = tokio::time::timeout(Duration::from_secs(10), crate::mcp::Session::new(state.clone()).serve(stream)) => {
                    result.map_err(|_| "mcp_initialize_timeout")?.map_err(|_| "mcp_initialize_failed")?
                }
            };
            let cancel = running.cancellation_token();
            state.mcp_connected().await;
            tokio::select! {
                result = running.waiting() => { result.map_err(|_| "mcp_session_failed")?; },
                _ = shutdown.cancelled() => { cancel.cancel(); },
            }
            if !shutdown.is_cancelled() {
                state.mcp_disconnected().await;
            }
            Ok(())
        }
        "control" => {
            let command = hello.command.as_deref().ok_or("missing_control_command")?;
            let result = if command == "stop" {
                control(&state, &endpoint, command).await
            } else {
                let owner = state.clone();
                let command = command.to_owned();
                state
                    .controls
                    .run(async move { control(&owner, &endpoint, &command).await })
                    .await
                    .unwrap_or_else(|error| Err(Value::from(error)))
            };
            let response = match result {
                Ok(value) => json!({"ok":true,"result":value}),
                Err(error) => json!({"ok":false,"error":error}),
            };
            write_message(stream.get_mut(), &response).await
        }
        _ => Err("unknown_control_mode".into()),
    }
}

async fn control(
    state: &Arc<crate::ServiceState>,
    endpoint: &Endpoint,
    command: &str,
) -> Result<Value, Value> {
    match command {
        "ping" => Ok(
            json!({"version":endpoint.version,"binary_id":endpoint.binary_id,"pid":endpoint.pid}),
        ),
        "status" => serde_json::to_value(
            state
                .service_status(crate::status::StatusContext::Human)
                .await
                .map_err(Value::from)?,
        )
        .map_err(|_| Value::from("status_unavailable")),
        "login" => state.begin_login().await.map_err(Value::from),
        "login-status" => serde_json::to_value(state.login_status().await.map_err(Value::from)?)
            .map_err(|_| Value::from("status_unavailable")),
        "login-cancel" => state.cancel_login().await.map_err(Value::from),
        "browser-opened" => {
            state.login.browser_opened();
            Ok(Value::Null)
        }
        "logout" => state.logout().await.map_err(Value::from),
        "web" => Ok(json!({"url":state.browser.launch_url().map_err(Value::from)?})),
        "data-status" => state.data_status().await.map_err(Value::from),
        "stop" => {
            state.stop().await.map_err(Value::from)?;
            Ok(json!({"stopping":true,"data_preserved":true}))
        }
        _ => Err(Value::from("unknown_control_command")),
    }
}
