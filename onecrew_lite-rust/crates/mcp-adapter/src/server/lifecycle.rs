use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use onecrew_local_runtime::LocalPlatform;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{operation_registry::OperationRegistryHandle, qr_images};

use super::setup::{McpSetupPrompt, SetupCapabilityState, SetupState, protect_setup, setup};
use super::surface::{McpAdapterServer, McpSurface};

#[derive(Clone)]
pub struct McpAdapterOptions {
    pub port: u16,
    pub access_token: String,
}

impl fmt::Debug for McpAdapterOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpAdapterOptions")
            .field("port", &self.port)
            .field("access_token", &"<redacted>")
            .finish()
    }
}

impl Default for McpAdapterOptions {
    fn default() -> Self {
        Self {
            port: 39393,
            access_token: String::new(),
        }
    }
}

pub struct McpAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAdapterTaskFailureKind {
    Serve,
    UnexpectedExit,
    TaskPanicked,
    TaskCancelled,
    ShutdownTimeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAdapterTaskCompletion {
    Running,
    Stopped,
    Failed(McpAdapterTaskFailureKind),
}

#[derive(Clone)]
pub struct McpAdapterCompletionObserver {
    receiver: watch::Receiver<McpAdapterTaskCompletion>,
}

impl McpAdapterCompletionObserver {
    pub fn current(&self) -> McpAdapterTaskCompletion {
        *self.receiver.borrow()
    }

    pub async fn changed(&mut self) -> McpAdapterTaskCompletion {
        // A dropped sender leaves the last published completion in place.
        if let Err(error) = self.receiver.changed().await {
            log::debug!("MCP adapter completion sender is gone: {error}");
        }
        self.current()
    }
}

pub struct McpAdapterHandle {
    pub endpoint: String,
    setup_state: SetupState,
    cancellation: CancellationToken,
    task: Option<JoinHandle<Result<(), String>>>,
    serve_abort: tokio::task::AbortHandle,
    completion_sender: watch::Sender<McpAdapterTaskCompletion>,
    completion: McpAdapterCompletionObserver,
    update_control: super::McpAdapterUpdateControl,
}

impl McpAdapterHandle {
    /// Return the shared tool-call owner without borrowing a host state lock
    /// across the asynchronous update pause/drain operation.
    pub fn update_control(&self) -> super::McpAdapterUpdateControl {
        self.update_control.clone()
    }
    pub fn completion(&self) -> McpAdapterCompletionObserver {
        self.completion.clone()
    }

    pub fn issue_setup_prompt(&self) -> McpSetupPrompt {
        self.setup_state.issue()
    }

    pub async fn stop(mut self) -> Result<(), String> {
        self.cancellation.cancel();
        let mut task = self
            .task
            .take()
            .ok_or_else(|| "MCP adapter task is unavailable".to_string())?;
        let join_result = tokio::time::timeout(Duration::from_secs(3), &mut task).await;
        match join_result {
            Ok(Ok(_)) => {}
            Ok(Err(_)) => {}
            Err(_) => {
                self.serve_abort.abort();
                task.abort();
                // Abort is the expected way for the serve task to end here.
                if let Err(error) = task.await
                    && !error.is_cancelled()
                {
                    log::warn!("MCP adapter serve task ended abnormally: {error}");
                }
                self.completion_sender
                    .send_replace(McpAdapterTaskCompletion::Failed(
                        McpAdapterTaskFailureKind::ShutdownTimeout,
                    ));
            }
        }
        match self.completion.current() {
            McpAdapterTaskCompletion::Running | McpAdapterTaskCompletion::Stopped => Ok(()),
            McpAdapterTaskCompletion::Failed(_) => {
                Err("MCP adapter stopped after a server failure".to_string())
            }
        }
    }
}

impl Drop for McpAdapterHandle {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(task) = self.task.take() {
            self.serve_abort.abort();
            task.abort();
        }
        if self.completion.current() == McpAdapterTaskCompletion::Running {
            self.completion_sender
                .send_replace(McpAdapterTaskCompletion::Stopped);
        }
    }
}

impl McpAdapter {
    pub async fn start(
        platform: Arc<LocalPlatform>,
        registry: &OperationRegistryHandle,
        options: McpAdapterOptions,
    ) -> Result<McpAdapterHandle, String> {
        if options.access_token.trim().is_empty() {
            return Err("MCP access token must not be empty".into());
        }
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), options.port);
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|error| format!("failed to bind MCP adapter on {address}: {error}"))?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let cancellation = CancellationToken::new();
        qr_images::cleanup_stale_files();
        let server = McpAdapterServer::new(platform, registry);
        let update_control = server.update_control.clone();
        let base_server = server.clone();
        let service = StreamableHttpService::new(
            move || Ok(server.clone()),
            LocalSessionManager::default().into(),
            {
                let mut config = StreamableHttpServerConfig::default();
                // Modern 2026-07-28 requests are always stateless; this also
                // disables legacy per-session state so business continuity
                // stays in runtime flow/operation IDs for every client.
                config.legacy_session_mode = false;
                config.json_response = true;
                config.cancellation_token = cancellation.child_token();
                config
            },
        );
        let token = options.access_token;
        let endpoint = format!("http://{address}/mcp");
        // Exact routes only: unknown /mcp sub-paths fall through to 404
        // instead of being served as MCP protocol requests.
        let mut protected_mcp = Router::new().route_service("/mcp", service);
        for surface in McpSurface::ALL {
            if surface == McpSurface::Full {
                continue;
            }
            let surface_server = base_server.for_surface(surface);
            protected_mcp = protected_mcp.route_service(
                surface.path(),
                StreamableHttpService::new(
                    move || Ok(surface_server.clone()),
                    LocalSessionManager::default().into(),
                    {
                        let mut config = StreamableHttpServerConfig::default();
                        config.legacy_session_mode = false;
                        config.json_response = true;
                        config.cancellation_token = cancellation.child_token();
                        config
                    },
                ),
            );
        }
        let protected_mcp =
            protected_mcp.route_layer(middleware::from_fn_with_state(token.clone(), authorize));
        let setup_state = SetupState {
            capability: Arc::new(Mutex::new(SetupCapabilityState::Unissued)),
            allowed_hosts: Arc::new([address.to_string(), format!("localhost:{}", address.port())]),
        };
        let setup_router = Router::new()
            .route("/setup/{capability}", get(setup))
            .route_layer(middleware::from_fn_with_state(
                setup_state.clone(),
                protect_setup,
            ))
            .with_state(setup_state.clone());
        let router = setup_router.merge(protected_mcp);
        let shutdown = cancellation.clone();
        let observed_cancellation = cancellation.clone();
        let (completion_sender, completion_receiver) =
            watch::channel(McpAdapterTaskCompletion::Running);
        let task_completion_sender = completion_sender.clone();
        let serve_task = tokio::spawn(async move {
            let serve_result = axum::serve(listener, router)
                .with_graceful_shutdown(async move { shutdown.cancelled_owned().await })
                .await;
            match serve_result {
                Ok(()) if observed_cancellation.is_cancelled() => Ok(()),
                Ok(()) => Err(McpAdapterTaskFailureKind::UnexpectedExit),
                Err(_) => Err(McpAdapterTaskFailureKind::Serve),
            }
        });
        let serve_abort = serve_task.abort_handle();
        let task = tokio::spawn(supervise_task(serve_task, task_completion_sender));
        Ok(McpAdapterHandle {
            update_control,
            endpoint,
            setup_state,
            cancellation,
            task: Some(task),
            serve_abort,
            completion_sender,
            completion: McpAdapterCompletionObserver {
                receiver: completion_receiver,
            },
        })
    }
}

async fn supervise_task(
    task: JoinHandle<Result<(), McpAdapterTaskFailureKind>>,
    completion_sender: watch::Sender<McpAdapterTaskCompletion>,
) -> Result<(), String> {
    let completion = completion_from_join_result(task.await);
    completion_sender.send_replace(completion);
    match completion {
        McpAdapterTaskCompletion::Running | McpAdapterTaskCompletion::Stopped => Ok(()),
        McpAdapterTaskCompletion::Failed(_) => Err("MCP adapter server task failed".to_string()),
    }
}

fn completion_from_join_result(
    result: Result<Result<(), McpAdapterTaskFailureKind>, tokio::task::JoinError>,
) -> McpAdapterTaskCompletion {
    match result {
        Ok(Ok(())) => McpAdapterTaskCompletion::Stopped,
        Ok(Err(kind)) => McpAdapterTaskCompletion::Failed(kind),
        Err(error) if error.is_panic() => {
            McpAdapterTaskCompletion::Failed(McpAdapterTaskFailureKind::TaskPanicked)
        }
        Err(_) => McpAdapterTaskCompletion::Failed(McpAdapterTaskFailureKind::TaskCancelled),
    }
}

async fn authorize(
    State(token): State<String>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let expected = format!("Bearer {token}");
    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if provided != Some(expected.as_str()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use crate::server::v2_adapter_tests::test_options;

    use super::*;

    #[test]
    fn adapter_options_debug_redacts_access_token() {
        let secret = "debug-must-not-contain-this-secret";
        let rendered = format!("{:?}", test_options(secret));
        assert!(!rendered.contains(secret));
        assert!(rendered.contains("<redacted>"));
    }

    #[tokio::test]
    async fn supervisor_reports_panics_and_cancellation_without_hanging_observers() {
        let (panic_sender, panic_receiver) = watch::channel(McpAdapterTaskCompletion::Running);
        let mut panic_observer = McpAdapterCompletionObserver {
            receiver: panic_receiver,
        };
        let panicking = tokio::spawn(async {
            panic!("synthetic serve panic without secrets");
            #[expect(
                unreachable_code,
                reason = "the typed tail fixes the spawned task output after the intentional test panic"
            )]
            Ok::<(), McpAdapterTaskFailureKind>(())
        });
        let panic_supervisor = tokio::spawn(supervise_task(panicking, panic_sender));
        let panic_completion =
            tokio::time::timeout(Duration::from_secs(1), panic_observer.changed())
                .await
                .expect("panic observer must become terminal");
        assert_eq!(
            panic_completion,
            McpAdapterTaskCompletion::Failed(McpAdapterTaskFailureKind::TaskPanicked)
        );
        assert!(panic_supervisor.await.unwrap().is_err());

        let (cancel_sender, cancel_receiver) = watch::channel(McpAdapterTaskCompletion::Running);
        let mut cancel_observer = McpAdapterCompletionObserver {
            receiver: cancel_receiver,
        };
        let cancelled = tokio::spawn(async {
            std::future::pending::<()>().await;
            Ok::<(), McpAdapterTaskFailureKind>(())
        });
        cancelled.abort();
        let cancel_supervisor = tokio::spawn(supervise_task(cancelled, cancel_sender));
        let cancel_completion =
            tokio::time::timeout(Duration::from_secs(1), cancel_observer.changed())
                .await
                .expect("cancel observer must become terminal");
        assert_eq!(
            cancel_completion,
            McpAdapterTaskCompletion::Failed(McpAdapterTaskFailureKind::TaskCancelled)
        );
        assert!(cancel_supervisor.await.unwrap().is_err());
    }
}
