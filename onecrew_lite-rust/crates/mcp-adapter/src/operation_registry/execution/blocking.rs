//! Owns blocking-pool spawn, join, cancellation, and panic classification.

/// Why a blocking-pool task finished without handing back the owner's result.
///
/// Produced only by [`run_blocking`] from tokio's `JoinError`; the panic
/// payload never crosses into a domain failure or the public boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockingTaskFailure {
    /// The owner call panicked inside the blocking pool.
    Panicked,
    /// The runtime shut down before the task ran; the owner call never
    /// started.
    Cancelled,
}

impl From<BlockingTaskFailure> for String {
    fn from(failure: BlockingTaskFailure) -> Self {
        match failure {
            BlockingTaskFailure::Panicked => "blocking owner call panicked".to_owned(),
            BlockingTaskFailure::Cancelled => "blocking owner call was cancelled".to_owned(),
        }
    }
}

/// The one blocking wrapper for this host (HOST-01, HOST-03).
///
/// An operation handler body only awaits; every synchronous Creator or platform
/// owner call (SQLite behind the repository lock, vault reads, filesystem
/// export) runs here on tokio's blocking pool so it never parks a runtime
/// worker. Design choice: wrap the synchronous segment inside each typed
/// binding rather than moving whole handler futures onto blocking threads,
/// because many bindings mix one synchronous owner call with a real network
/// `await` that must keep its worker-thread scheduling.
///
/// `JoinError` is mapped exactly once, through `E: From<BlockingTaskFailure>`:
/// `String` bindings receive a secret-free message, and each typed binding
/// error maps the failure to its own existing "runtime unavailable" variant
/// beside its classifier (ADAPTER-13). No new public failure code or shared
/// envelope is introduced.
pub(crate) async fn run_blocking<T, E, F>(call: F) -> Result<T, E>
where
    T: Send + 'static,
    E: From<BlockingTaskFailure> + Send + 'static,
    F: FnOnce() -> Result<T, E> + Send + 'static,
{
    match tokio::task::spawn_blocking(call).await {
        Ok(result) => result,
        Err(error) if error.is_panic() => Err(E::from(BlockingTaskFailure::Panicked)),
        Err(_) => Err(E::from(BlockingTaskFailure::Cancelled)),
    }
}

#[cfg(test)]
mod tests;
