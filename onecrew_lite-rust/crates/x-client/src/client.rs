use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use onecrew_x_transport::{JsonTransport, TransportError};
use thiserror::Error;

use crate::graphql::query_ids;

mod reads;
mod request;
mod resolve;
mod writes;

pub use reads::{
    POST_THREAD_DEFAULT_COUNT, POST_THREAD_MAX_COUNT, POST_THREAD_MIN_COUNT,
    accepts_post_thread_count,
};

#[derive(Debug, Error)]
pub enum XClientError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("unknown X GraphQL operation: {0}")]
    UnknownOperation(String),
    #[error("invalid X URL: {0}")]
    Url(String),
    #[error("invalid X response: {0}")]
    Response(String),
    #[error("invalid X input: {0}")]
    Input(String),
}

#[derive(Clone)]
pub struct XClient<T: JsonTransport> {
    transport: Arc<T>,
    /// Hardcoded fallback queryIds (same table twitter-cli ships) — the happy path.
    query_ids: BTreeMap<&'static str, &'static str>,
    /// Runtime-resolved queryIds (fallback, or a live value after a stale one was
    /// refreshed from the openapi / JS bundles). Shared across clones.
    resolved: Arc<Mutex<BTreeMap<String, String>>>,
    /// Whether x.com JS bundles have been scanned this session (one-shot).
    scanned: Arc<Mutex<bool>>,
    /// The operating account's own profile. Resolving it costs two requests
    /// and it cannot change while the session's cookies do not, so it is
    /// resolved once and shared across clones (a single DM sync used to ask
    /// for it four times).
    identity: Arc<tokio::sync::OnceCell<crate::models::User>>,
}

impl<T: JsonTransport> XClient<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport: Arc::new(transport),
            query_ids: query_ids(),
            resolved: Arc::new(Mutex::new(BTreeMap::new())),
            scanned: Arc::new(Mutex::new(false)),
            identity: Arc::new(tokio::sync::OnceCell::new()),
        }
    }
}
