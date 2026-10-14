//! Bounded native loopback callback transport for Creator Sync pairing.
//!
//! This owner binds one ephemeral IPv4 loopback listener, accepts one strict
//! HTTP request, and moves validated callback material directly into typed
//! Creator Sync values. Raw request bytes never cross this module.

use std::{
    io::ErrorKind,
    net::{IpAddr, Ipv4Addr},
    sync::Mutex,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use onecrew_creator_sync::{PairingCode, PairingState};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::{Instant, timeout, timeout_at},
};
use uuid::Uuid;
use zeroize::Zeroizing;

const CALLBACK_PATH: &str = "/creator-sync/pairing-callback";
const OVERALL_WAIT: Duration = Duration::from_secs(5 * 60);
const ACCEPT_POLL: Duration = Duration::from_secs(1);
const READ_TIMEOUT: Duration = Duration::from_secs(3);
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);
const JOIN_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REQUEST_LINE_BYTES: usize = 2_048;
const MAX_HEADER_LINE_BYTES: usize = 1_024;
const MAX_QUERY_BYTES: usize = 1_024;
const MAX_HEADER_BYTES: usize = 8_192;
const MAX_HEADER_COUNT: usize = 32;

const RESULT_PAGE_TEMPLATE: &str = include_str!("assets/pairing-result.html");
const BRAND_MARK: &[u8] = include_bytes!("assets/onecrew-ai-mark.png");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PairingLoopbackStartError {
    BindUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PairingLoopbackReceiveError {
    CallbackAlreadyTaken,
    Canceled,
    Rejected,
    TimedOut,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PairingLoopbackDisposition {
    Succeeded,
    Rejected,
    RateLimited,
    RemoteUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PairingLoopbackOutcome {
    Succeeded,
    Rejected,
    Canceled,
    TimedOut,
    TransportFailed,
    JoinFailed,
    AlreadyJoined,
}

/// Typed callback material. Deliberately no `Debug`, `Clone`, or Serde.
pub(crate) struct PairingLoopbackAccepted {
    code: PairingCode,
    state: PairingState,
    completion: PairingLoopbackCompletion,
}

impl PairingLoopbackAccepted {
    pub(crate) fn into_parts(self) -> (PairingCode, PairingState, PairingLoopbackCompletion) {
        (self.code, self.state, self.completion)
    }
}

/// One-shot response decision retained by the native controller.
/// Deliberately no `Debug`, `Clone`, or Serde.
pub(crate) struct PairingLoopbackCompletion {
    sender: Option<oneshot::Sender<PairingLoopbackDisposition>>,
}

impl PairingLoopbackCompletion {
    pub(crate) fn respond(mut self, disposition: PairingLoopbackDisposition) {
        if let Some(sender) = self.sender.take()
            && sender.send(disposition).is_err()
        {
            // The responder already left; there is no one to answer and, by
            // this module's no-log contract, nothing further to record.
        }
    }
}

struct PairingCallbackParts {
    code: PairingCode,
    state: PairingState,
}

#[derive(Clone, Copy)]
struct PairingLoopbackLimits {
    overall_wait: Duration,
    accept_poll: Duration,
    read_timeout: Duration,
    write_timeout: Duration,
    join_timeout: Duration,
}

impl Default for PairingLoopbackLimits {
    fn default() -> Self {
        Self {
            overall_wait: OVERALL_WAIT,
            accept_poll: ACCEPT_POLL,
            read_timeout: READ_TIMEOUT,
            write_timeout: WRITE_TIMEOUT,
            join_timeout: JOIN_TIMEOUT,
        }
    }
}

pub(crate) struct PairingLoopbackSupervisor {
    bound_port: u16,
    cancel: Mutex<Option<oneshot::Sender<()>>>,
    callback:
        Mutex<Option<oneshot::Receiver<Result<PairingCallbackParts, PairingLoopbackReceiveError>>>>,
    completion: Mutex<Option<oneshot::Sender<PairingLoopbackDisposition>>>,
    task: Mutex<Option<JoinHandle<PairingLoopbackOutcome>>>,
    join_timeout: Duration,
}

impl PairingLoopbackSupervisor {
    pub(crate) async fn begin() -> Result<Self, PairingLoopbackStartError> {
        Self::begin_with_limits(PairingLoopbackLimits::default()).await
    }

    async fn begin_with_limits(
        limits: PairingLoopbackLimits,
    ) -> Result<Self, PairingLoopbackStartError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| PairingLoopbackStartError::BindUnavailable)?;
        let address = listener
            .local_addr()
            .map_err(|_| PairingLoopbackStartError::BindUnavailable)?;
        if address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) || address.port() == 0 {
            return Err(PairingLoopbackStartError::BindUnavailable);
        }

        let (cancel_tx, cancel_rx) = oneshot::channel();
        let (callback_tx, callback_rx) = oneshot::channel();
        let (completion_tx, completion_rx) = oneshot::channel();
        let bound_port = address.port();
        let task = tokio::spawn(run_loopback_listener(
            listener,
            bound_port,
            limits,
            cancel_rx,
            callback_tx,
            completion_rx,
        ));
        Ok(Self {
            bound_port,
            cancel: Mutex::new(Some(cancel_tx)),
            callback: Mutex::new(Some(callback_rx)),
            completion: Mutex::new(Some(completion_tx)),
            task: Mutex::new(Some(task)),
            join_timeout: limits.join_timeout,
        })
    }

    pub(crate) fn bound_port(&self) -> u16 {
        self.bound_port
    }

    pub(crate) async fn receive_callback(
        &self,
    ) -> Result<PairingLoopbackAccepted, PairingLoopbackReceiveError> {
        let receiver = self
            .callback
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .ok_or(PairingLoopbackReceiveError::CallbackAlreadyTaken)?;
        let parts = receiver
            .await
            .map_err(|_| PairingLoopbackReceiveError::Unavailable)??;
        let sender = self
            .completion
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .ok_or(PairingLoopbackReceiveError::Unavailable)?;
        Ok(PairingLoopbackAccepted {
            code: parts.code,
            state: parts.state,
            completion: PairingLoopbackCompletion {
                sender: Some(sender),
            },
        })
    }

    pub(crate) async fn join_after_completion(&self) -> PairingLoopbackOutcome {
        self.join_task(false).await
    }

    pub(crate) async fn cancel_and_join(&self) -> PairingLoopbackOutcome {
        self.join_task(true).await
    }

    async fn join_task(&self, cancel: bool) -> PairingLoopbackOutcome {
        if cancel {
            if let Some(sender) = self
                .cancel
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
                && sender.send(()).is_err()
            {
                // The task already finished and dropped its receiver; the join
                // below observes that terminal state.
            }
            self.completion
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take();
        }
        let Some(mut task) = self
            .task
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        else {
            return PairingLoopbackOutcome::AlreadyJoined;
        };
        match timeout(self.join_timeout, &mut task).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => PairingLoopbackOutcome::JoinFailed,
            Err(_) => {
                task.abort();
                if task.await.is_err() {
                    // Abort is the expected way for the listener to end here; a
                    // panic payload is not examined because this module keeps
                    // no log sink, and either way the outcome is `JoinFailed`.
                }
                PairingLoopbackOutcome::JoinFailed
            }
        }
    }
}

impl Drop for PairingLoopbackSupervisor {
    fn drop(&mut self) {
        if let Some(sender) = self
            .cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            && sender.send(()).is_err()
        {
            // The task already finished and dropped its receiver; drop has
            // nothing further to wait for.
        }
        self.completion
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(task) = self
            .task
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            task.abort();
        }
    }
}

async fn run_loopback_listener(
    listener: TcpListener,
    bound_port: u16,
    limits: PairingLoopbackLimits,
    mut cancel: oneshot::Receiver<()>,
    callback: oneshot::Sender<Result<PairingCallbackParts, PairingLoopbackReceiveError>>,
    completion: oneshot::Receiver<PairingLoopbackDisposition>,
) -> PairingLoopbackOutcome {
    let deadline = Instant::now() + limits.overall_wait;
    let mut callback = Some(callback);
    let accepted = loop {
        if Instant::now() >= deadline {
            notify_callback(&mut callback, Err(PairingLoopbackReceiveError::TimedOut));
            return PairingLoopbackOutcome::TimedOut;
        }
        let poll = limits
            .accept_poll
            .min(deadline.saturating_duration_since(Instant::now()));
        tokio::select! {
            _ = &mut cancel => {
                notify_callback(&mut callback, Err(PairingLoopbackReceiveError::Canceled));
                return PairingLoopbackOutcome::Canceled;
            }
            result = timeout(poll, listener.accept()) => {
                match result {
                    Ok(Ok((stream, _peer))) => break stream,
                    Ok(Err(_)) => {
                        notify_callback(
                            &mut callback,
                            Err(PairingLoopbackReceiveError::Unavailable),
                        );
                        return PairingLoopbackOutcome::TransportFailed;
                    }
                    Err(_) => continue,
                }
            }
        }
    };
    drop(listener);

    let parsed = tokio::select! {
        _ = &mut cancel => {
            notify_callback(&mut callback, Err(PairingLoopbackReceiveError::Canceled));
            reject_accepted(&accepted, limits.write_timeout).await;
            return PairingLoopbackOutcome::Canceled;
        }
        result = read_and_parse_request(&accepted, bound_port, deadline, limits.read_timeout) => result,
    };
    let parts = match parsed {
        Ok(parts) => parts,
        Err(RequestFailure::TimedOut) => {
            notify_callback(&mut callback, Err(PairingLoopbackReceiveError::TimedOut));
            reject_accepted(&accepted, limits.write_timeout).await;
            return PairingLoopbackOutcome::TimedOut;
        }
        Err(RequestFailure::Rejected) => {
            notify_callback(&mut callback, Err(PairingLoopbackReceiveError::Rejected));
            reject_accepted(&accepted, limits.write_timeout).await;
            return PairingLoopbackOutcome::Rejected;
        }
        Err(RequestFailure::Transport) => {
            notify_callback(&mut callback, Err(PairingLoopbackReceiveError::Unavailable));
            return PairingLoopbackOutcome::TransportFailed;
        }
    };
    if send_callback_result(&mut callback, Ok(parts)).is_err() {
        reject_accepted(&accepted, limits.write_timeout).await;
        return PairingLoopbackOutcome::Rejected;
    }

    let disposition = tokio::select! {
        biased;
        _ = &mut cancel => PairingLoopbackDisposition::Rejected,
        result = timeout_at(deadline, completion) => {
            match result {
                Ok(Ok(disposition)) => disposition,
                Ok(Err(_)) | Err(_) => PairingLoopbackDisposition::Rejected,
            }
        }
    };
    if write_response(&accepted, disposition, limits.write_timeout)
        .await
        .is_err()
    {
        return PairingLoopbackOutcome::TransportFailed;
    }
    if disposition == PairingLoopbackDisposition::Succeeded {
        PairingLoopbackOutcome::Succeeded
    } else {
        PairingLoopbackOutcome::Rejected
    }
}

fn send_callback_result(
    callback: &mut Option<
        oneshot::Sender<Result<PairingCallbackParts, PairingLoopbackReceiveError>>,
    >,
    result: Result<PairingCallbackParts, PairingLoopbackReceiveError>,
) -> Result<(), ()> {
    callback.take().ok_or(())?.send(result).map_err(|_| ())
}

/// Deliver a terminal callback result where the waiter may already be gone:
/// an already-consumed callback or a dropped receiver is never an error of
/// the loopback itself. This module logs nothing: callback material must not
/// reach a log sink (see `exact_request_yields_only_non_debug_owner_types`).
fn notify_callback(
    callback: &mut Option<
        oneshot::Sender<Result<PairingCallbackParts, PairingLoopbackReceiveError>>,
    >,
    result: Result<PairingCallbackParts, PairingLoopbackReceiveError>,
) {
    if send_callback_result(callback, result).is_err() {
        // A consumed callback or a gone receiver: the caller returns the
        // loopback outcome, and this module keeps no log sink.
    }
}

/// Answer an accepted connection with the rejection page. The peer may have
/// closed already; the loopback outcome is decided by the caller, not by
/// whether this courtesy response was delivered.
async fn reject_accepted(accepted: &TcpStream, write_timeout: Duration) {
    if write_response(
        accepted,
        PairingLoopbackDisposition::Rejected,
        write_timeout,
    )
    .await
    .is_err()
    {
        // The peer closed before the courtesy page; the outcome is decided by
        // the caller, and this module keeps no log sink.
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestFailure {
    Rejected,
    TimedOut,
    Transport,
}

async fn read_and_parse_request(
    stream: &TcpStream,
    bound_port: u16,
    overall_deadline: Instant,
    read_timeout: Duration,
) -> Result<PairingCallbackParts, RequestFailure> {
    let read_deadline = (Instant::now() + read_timeout).min(overall_deadline);
    let mut buffer = Zeroizing::new(Vec::with_capacity(MAX_HEADER_BYTES + 1));
    let mut chunk = Zeroizing::new([0_u8; 1_024]);
    let header_end = loop {
        timeout_at(read_deadline, stream.readable())
            .await
            .map_err(|_| RequestFailure::TimedOut)?
            .map_err(|_| RequestFailure::Transport)?;
        match stream.try_read(&mut chunk[..]) {
            Ok(0) => return Err(RequestFailure::Rejected),
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                chunk[..read].fill(0);
                if buffer.len() > MAX_HEADER_BYTES {
                    return Err(RequestFailure::Rejected);
                }
                if let Some(end) = find_header_end(&buffer) {
                    break end;
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => continue,
            Err(_) => return Err(RequestFailure::Transport),
        }
    };
    if header_end != buffer.len() {
        return Err(RequestFailure::Rejected);
    }
    parse_request(&buffer[..header_end - 4], bound_port)
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn parse_request(request: &[u8], bound_port: u16) -> Result<PairingCallbackParts, RequestFailure> {
    if request.len() > MAX_HEADER_BYTES {
        return Err(RequestFailure::Rejected);
    }
    let request = std::str::from_utf8(request).map_err(|_| RequestFailure::Rejected)?;
    let mut lines = request.split("\r\n");
    let request_line = lines.next().ok_or(RequestFailure::Rejected)?;
    if request_line.len() > MAX_REQUEST_LINE_BYTES {
        return Err(RequestFailure::Rejected);
    }
    let mut parts = request_line.split(' ');
    let method = parts.next().ok_or(RequestFailure::Rejected)?;
    let target = parts.next().ok_or(RequestFailure::Rejected)?;
    let version = parts.next().ok_or(RequestFailure::Rejected)?;
    if parts.next().is_some() || method != "GET" || version != "HTTP/1.1" {
        return Err(RequestFailure::Rejected);
    }
    if target.contains('#') || !target.starts_with('/') {
        return Err(RequestFailure::Rejected);
    }
    let (path, query) = target.split_once('?').ok_or(RequestFailure::Rejected)?;
    if path != CALLBACK_PATH || query.is_empty() || query.len() > MAX_QUERY_BYTES {
        return Err(RequestFailure::Rejected);
    }

    let mut host = None;
    let mut content_length = None;
    let mut header_count = 0_usize;
    for line in lines {
        if line.is_empty() || line.len() > MAX_HEADER_LINE_BYTES {
            return Err(RequestFailure::Rejected);
        }
        header_count += 1;
        if header_count > MAX_HEADER_COUNT {
            return Err(RequestFailure::Rejected);
        }
        let (name, value) = line.split_once(':').ok_or(RequestFailure::Rejected)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(RequestFailure::Rejected);
        }
        let value = value.trim_matches([' ', '\t']);
        if name.eq_ignore_ascii_case("host") {
            if host.replace(value).is_some() {
                return Err(RequestFailure::Rejected);
            }
            continue;
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(RequestFailure::Rejected);
        }
        if name.eq_ignore_ascii_case("content-length") && content_length.replace(value).is_some() {
            return Err(RequestFailure::Rejected);
        }
    }
    let expected_host = format!("127.0.0.1:{bound_port}");
    if host != Some(expected_host.as_str()) {
        return Err(RequestFailure::Rejected);
    }
    if let Some(content_length) = content_length
        && content_length.parse::<usize>().ok() != Some(0)
    {
        return Err(RequestFailure::Rejected);
    }

    let fields = query.split('&').collect::<Vec<_>>();
    if fields.len() != 2 {
        return Err(RequestFailure::Rejected);
    }
    let mut code = None;
    let mut state = None;
    for field in fields {
        let (name, value) = field.split_once('=').ok_or(RequestFailure::Rejected)?;
        if value.is_empty() || value.contains('%') {
            return Err(RequestFailure::Rejected);
        }
        match name {
            "code" if code.is_none() => code = Some(value),
            "state" if state.is_none() => state = Some(value),
            _ => return Err(RequestFailure::Rejected),
        }
    }
    let code = PairingCode::parse(code.ok_or(RequestFailure::Rejected)?.to_owned())
        .map_err(|_| RequestFailure::Rejected)?;
    let state = PairingState::parse(state.ok_or(RequestFailure::Rejected)?.to_owned())
        .map_err(|_| RequestFailure::Rejected)?;
    Ok(PairingCallbackParts { code, state })
}

async fn write_response(
    stream: &TcpStream,
    disposition: PairingLoopbackDisposition,
    write_timeout: Duration,
) -> Result<(), ()> {
    let (status, page) = match disposition {
        PairingLoopbackDisposition::Succeeded => (
            "200 OK",
            PairingResultPage {
                result: "success",
                icon: "✓",
                title: "绑定完成",
                message: "OneCrew 已完成账户绑定与凭据验证。",
                status: "已安全连接",
                hint: "可以关闭此标签页并返回桌面应用。",
            },
        ),
        PairingLoopbackDisposition::RemoteUnavailable => (
            "503 Service Unavailable",
            PairingResultPage {
                result: "rejected",
                icon: "!",
                title: "网络连接失败",
                message: "桌面应用无法连接 OneCrew 同步服务。",
                status: "请检查网络或代理",
                hint: "请返回 OneCrew，网络恢复后重新检测并开始绑定。",
            },
        ),
        PairingLoopbackDisposition::RateLimited => (
            "429 Too Many Requests",
            PairingResultPage {
                result: "rejected",
                icon: "!",
                title: "操作过于频繁",
                message: "当前来源的绑定尝试已达到短时上限。",
                status: "请稍后再试",
                hint: "请返回 OneCrew；冷却结束后重新检测，不需要重复网页授权。",
            },
        ),
        PairingLoopbackDisposition::Rejected => (
            "400 Bad Request",
            PairingResultPage {
                result: "rejected",
                icon: "!",
                title: "绑定未完成",
                message: "本次请求未完成或已经失效。",
                status: "需要重新开始",
                hint: "请返回 OneCrew，取消当前流程后重新开始。",
            },
        ),
    };
    let nonce = Uuid::new_v4().simple().to_string();
    let body = render_result_page(page, &nonce);
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Language: zh-CN\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nCross-Origin-Opener-Policy: same-origin\r\nCross-Origin-Resource-Policy: same-origin\r\nPermissions-Policy: camera=(), geolocation=(), microphone=(), payment=()\r\nContent-Security-Policy: default-src 'none'; img-src data:; style-src 'nonce-{nonce}'; script-src 'nonce-{nonce}'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let deadline = Instant::now() + write_timeout;
    let mut written = 0_usize;
    while written < response.len() {
        timeout_at(deadline, stream.writable())
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;
        match stream.try_write(&response.as_bytes()[written..]) {
            Ok(0) => return Err(()),
            Ok(count) => written += count,
            Err(error) if error.kind() == ErrorKind::WouldBlock => continue,
            Err(_) => return Err(()),
        }
    }
    Ok(())
}

struct PairingResultPage {
    result: &'static str,
    icon: &'static str,
    title: &'static str,
    message: &'static str,
    status: &'static str,
    hint: &'static str,
}

fn render_result_page(page: PairingResultPage, nonce: &str) -> String {
    RESULT_PAGE_TEMPLATE
        .replace("__BRAND_MARK__", &STANDARD.encode(BRAND_MARK))
        .replace("__NONCE__", nonce)
        .replace("__RESULT__", page.result)
        .replace("__ICON__", page.icon)
        .replace("__TITLE__", page.title)
        .replace("__MESSAGE__", page.message)
        .replace("__STATUS__", page.status)
        .replace("__HINT__", page.hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "reacher_pairing_v1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const STATE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    fn request(port: u16) -> String {
        format!(
            "GET {CALLBACK_PATH}?code={CODE}&state={STATE} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        )
    }

    fn parse_text(value: &str, port: u16) -> Result<PairingCallbackParts, RequestFailure> {
        let value = value.strip_suffix("\r\n\r\n").unwrap_or(value);
        parse_request(value.as_bytes(), port)
    }

    fn assert_rejected(result: Result<PairingCallbackParts, RequestFailure>) {
        assert!(matches!(result, Err(RequestFailure::Rejected)));
    }

    fn assert_branded_secret_free_response(response: &str) {
        let encoded = response
            .split("data:image/png;base64,")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert_eq!(STANDARD.decode(encoded).unwrap(), BRAND_MARK);
        assert!(response.contains("img-src data:"));
        for required in [
            "OneCrew",
            "data:image/png;base64,",
            "#f4f5ef",
            "#ffffff",
            "#cfd7ce",
            "#101712",
            "#58655d",
            "#26734a",
            "此页面不会显示或保存授权信息。",
        ] {
            assert!(response.contains(required));
        }
        for forbidden in ["Agent Platform", "Reacher", CODE, STATE] {
            assert!(!response.contains(forbidden));
        }
    }

    async fn write_all(stream: &TcpStream, bytes: &[u8]) {
        let mut written = 0_usize;
        while written < bytes.len() {
            stream.writable().await.unwrap();
            match stream.try_write(&bytes[written..]) {
                Ok(count) => written += count,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => panic!("write failed: {error}"),
            }
        }
    }

    async fn read_to_close(stream: &TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 1_024];
        loop {
            stream.readable().await.unwrap();
            match stream.try_read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => bytes.extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => panic!("read failed: {error}"),
            }
        }
        String::from_utf8(bytes).unwrap()
    }

    fn short_limits() -> PairingLoopbackLimits {
        PairingLoopbackLimits {
            overall_wait: Duration::from_millis(120),
            accept_poll: Duration::from_millis(10),
            read_timeout: Duration::from_millis(40),
            write_timeout: Duration::from_millis(40),
            join_timeout: Duration::from_millis(100),
        }
    }

    #[test]
    fn exact_request_yields_only_non_debug_owner_types() {
        let parts = parse_text(&request(49152), 49152).unwrap();
        fn accepts_owner_types(_code: PairingCode, _state: PairingState) {}
        accepts_owner_types(parts.code, parts.state);

        let source = include_str!("pairing_loopback.rs");
        let declaration_start = source.find("/// Typed callback material").unwrap();
        let declaration_end = source.find("impl PairingLoopbackAccepted").unwrap();
        assert!(!source[declaration_start..declaration_end].contains("#[derive"));
        let production = &source[..source.find("#[cfg(test)]").unwrap()];
        for forbidden in [
            "log::",
            "println!",
            "#[tauri::command]",
            ".emit(",
            "Serialize",
        ] {
            assert!(!production.contains(forbidden));
        }
        assert!(production.contains("pub(crate) async fn cancel_and_join"));
        assert!(production.contains("impl Drop for PairingLoopbackSupervisor"));
        assert!(production.contains("task.abort()"));
    }

    #[test]
    fn method_version_path_target_and_query_fail_closed() {
        let port = 49152;
        for invalid in [
            request(port).replacen("GET ", "POST ", 1),
            request(port).replacen("HTTP/1.1", "HTTP/1.0", 1),
            request(port).replace(CALLBACK_PATH, "/creator-sync/other"),
            request(port).replace(
                CALLBACK_PATH,
                "http://127.0.0.1/creator-sync/pairing-callback",
            ),
            request(port).replace("?code=", "#fragment?code="),
            request(port).replace("&state=", "&extra=x&state="),
            request(port).replace("&state=", "&code=x&state="),
            request(port).replace("code=", "code=&ignored="),
            request(port).replace(STATE, &format!("{STATE}%41")),
        ] {
            assert_rejected(parse_text(&invalid, port));
        }
    }

    #[test]
    fn host_body_and_header_constraints_fail_closed() {
        let port = 49152;
        for invalid in [
            request(port).replace("127.0.0.1", "localhost"),
            request(port).replace("127.0.0.1", "[::1]"),
            request(port).replace(&port.to_string(), "49153"),
            request(port).replace("Host:", "Host: 127.0.0.1:49152\r\nHost:"),
            request(port).replace("Host:", "Transfer-Encoding: chunked\r\nHost:"),
            request(port).replace("Host:", "Content-Length: 1\r\nHost:"),
            request(port).replace("Host:", "Content-Length: nope\r\nHost:"),
            request(port).replace("Host:", "Content-Length: 0\r\nContent-Length: 0\r\nHost:"),
            request(port).replace("\r\n\r\n", "\r\n\r\nx"),
        ] {
            let without_terminator = invalid.strip_suffix("\r\n\r\n").unwrap_or(&invalid);
            assert_rejected(parse_request(without_terminator.as_bytes(), port));
        }
    }

    #[test]
    fn request_limits_are_enforced() {
        let port = 49152;
        let long_query = format!(
            "GET {CALLBACK_PATH}?code={CODE}&state={} HTTP/1.1\r\nHost: 127.0.0.1:{port}",
            "A".repeat(MAX_QUERY_BYTES)
        );
        assert_rejected(parse_request(long_query.as_bytes(), port));
        let long_header = format!(
            "{}\r\nX-Test: {}",
            request(port).trim_end(),
            "a".repeat(MAX_HEADER_LINE_BYTES)
        );
        assert_rejected(parse_request(long_header.as_bytes(), port));
        let many_headers = format!(
            "{}{}",
            request(port).trim_end(),
            (0..MAX_HEADER_COUNT)
                .map(|index| format!("\r\nX-{index}: a"))
                .collect::<String>()
        );
        assert_rejected(parse_request(many_headers.as_bytes(), port));
        let oversized_total = format!(
            "{}{}",
            request(port).trim_end(),
            (0..9)
                .map(|index| format!("\r\nX-{index}: {}", "a".repeat(1_000)))
                .collect::<String>()
        );
        assert!(oversized_total.len() > MAX_HEADER_BYTES);
        assert_rejected(parse_request(oversized_total.as_bytes(), port));
    }

    #[tokio::test]
    async fn listener_is_ready_single_use_and_response_is_secret_free() {
        let supervisor = PairingLoopbackSupervisor::begin_with_limits(short_limits())
            .await
            .unwrap();
        let port = supervisor.bound_port();
        assert_ne!(port, 0);
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        let raw_request = request(port);
        write_all(&stream, raw_request.as_bytes()).await;
        let accepted = supervisor.receive_callback().await.unwrap();
        let (_code, _state, completion) = accepted.into_parts();
        completion.respond(PairingLoopbackDisposition::Succeeded);
        let response = read_to_close(&stream).await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_branded_secret_free_response(&response);
        for required in [
            "Cache-Control: no-store",
            "Referrer-Policy: no-referrer",
            "X-Content-Type-Options: nosniff",
            "X-Frame-Options: DENY",
            "Cross-Origin-Opener-Policy: same-origin",
            "Permissions-Policy: camera=(), geolocation=(), microphone=(), payment=()",
            "OneCrew",
            "data:image/png;base64,",
            "绑定完成",
            "已安全连接",
            "history.replaceState",
            "/creator-sync/pairing-complete",
            "Connection: close",
        ] {
            assert!(response.contains(required));
        }
        assert!(response.contains("data-result=\"success\""));
        assert!(response.contains("script-src 'nonce-"));
        assert!(response.contains("style-src 'nonce-"));
        let nonce = response
            .split("script-src 'nonce-")
            .nth(1)
            .and_then(|value| value.split('\'').next())
            .unwrap();
        assert_eq!(nonce.len(), 32);
        assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(response.matches(&format!("nonce=\"{nonce}\"")).count(), 2);
        for placeholder in [
            "__NONCE__",
            "__RESULT__",
            "__ICON__",
            "__TITLE__",
            "__MESSAGE__",
            "__STATUS__",
            "__HINT__",
        ] {
            assert!(!response.contains(placeholder));
        }
        assert!(!response.contains("'unsafe-inline'"));
        for forbidden in [
            "location.search",
            "URLSearchParams",
            "href=",
            "src=\"http",
            "http://",
            "https://",
            "codex://",
        ] {
            assert!(!response.contains(forbidden));
        }
        assert_eq!(
            supervisor.join_after_completion().await,
            PairingLoopbackOutcome::Succeeded
        );
        assert!(
            TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn remote_unavailable_response_names_the_network_failure_without_secrets() {
        let supervisor = PairingLoopbackSupervisor::begin_with_limits(short_limits())
            .await
            .unwrap();
        let port = supervisor.bound_port();
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        write_all(&stream, request(port).as_bytes()).await;
        let accepted = supervisor.receive_callback().await.unwrap();
        let (_code, _state, completion) = accepted.into_parts();
        completion.respond(PairingLoopbackDisposition::RemoteUnavailable);
        let response = read_to_close(&stream).await;
        assert!(response.starts_with("HTTP/1.1 503 Service Unavailable"));
        assert_branded_secret_free_response(&response);
        assert!(response.contains("网络连接失败"));
        assert!(response.contains("无法连接 OneCrew 同步服务"));
        assert!(response.contains("请检查网络或代理"));
        assert!(response.contains("/creator-sync/pairing-failed"));
        assert_eq!(
            supervisor.join_after_completion().await,
            PairingLoopbackOutcome::Rejected
        );
    }

    #[tokio::test]
    async fn rate_limited_response_is_retryable_and_does_not_call_it_incomplete() {
        let supervisor = PairingLoopbackSupervisor::begin_with_limits(short_limits())
            .await
            .unwrap();
        let port = supervisor.bound_port();
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        write_all(&stream, request(port).as_bytes()).await;
        let accepted = supervisor.receive_callback().await.unwrap();
        let (_code, _state, completion) = accepted.into_parts();
        completion.respond(PairingLoopbackDisposition::RateLimited);
        let response = read_to_close(&stream).await;
        assert!(response.starts_with("HTTP/1.1 429 Too Many Requests"));
        assert_branded_secret_free_response(&response);
        assert!(response.contains("操作过于频繁"));
        assert!(response.contains("请稍后再试"));
        assert!(!response.contains("绑定未完成"));
        assert_eq!(
            supervisor.join_after_completion().await,
            PairingLoopbackOutcome::Rejected
        );
    }

    #[tokio::test]
    async fn invalid_request_is_rejected_and_consumes_the_only_connection() {
        let supervisor = PairingLoopbackSupervisor::begin_with_limits(short_limits())
            .await
            .unwrap();
        let port = supervisor.bound_port();
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        write_all(
            &stream,
            request(port).replace("127.0.0.1", "localhost").as_bytes(),
        )
        .await;
        let received = supervisor.receive_callback().await;
        assert!(matches!(
            received,
            Err(PairingLoopbackReceiveError::Rejected)
        ));
        let response = read_to_close(&stream).await;
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert_branded_secret_free_response(&response);
        assert!(response.contains("Cache-Control: no-store"));
        assert!(response.contains("Referrer-Policy: no-referrer"));
        assert!(response.contains("data-result=\"rejected\""));
        assert!(response.contains("绑定未完成"));
        assert!(response.contains("/creator-sync/pairing-failed"));
        assert!(response.contains("history.replaceState"));
        assert_eq!(
            supervisor.join_after_completion().await,
            PairingLoopbackOutcome::Rejected
        );
        assert!(
            TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn timeout_and_cancel_are_terminal_and_joined() {
        let timed_out = PairingLoopbackSupervisor::begin_with_limits(short_limits())
            .await
            .unwrap();
        let received = timed_out.receive_callback().await;
        assert!(matches!(
            received,
            Err(PairingLoopbackReceiveError::TimedOut)
        ));
        assert_eq!(
            timed_out.join_after_completion().await,
            PairingLoopbackOutcome::TimedOut
        );

        let canceled = PairingLoopbackSupervisor::begin_with_limits(short_limits())
            .await
            .unwrap();
        assert_eq!(
            canceled.cancel_and_join().await,
            PairingLoopbackOutcome::Canceled
        );
        let received = canceled.receive_callback().await;
        assert!(matches!(
            received,
            Err(PairingLoopbackReceiveError::Canceled)
        ));
        assert_eq!(
            canceled.cancel_and_join().await,
            PairingLoopbackOutcome::AlreadyJoined
        );
    }

    #[tokio::test]
    async fn connected_client_must_finish_headers_within_the_short_read_timeout() {
        let supervisor = PairingLoopbackSupervisor::begin_with_limits(short_limits())
            .await
            .unwrap();
        let port = supervisor.bound_port();
        let _stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        let received = supervisor.receive_callback().await;
        assert!(matches!(
            received,
            Err(PairingLoopbackReceiveError::TimedOut)
        ));
        assert_eq!(
            supervisor.join_after_completion().await,
            PairingLoopbackOutcome::TimedOut
        );
    }
}
