//! Application-owned outbox loop. Closing a Web tab never owns its lifetime.
use onecrew_creator_runtime::CreatorRuntime;
use futures::FutureExt;
use tokio_util::sync::CancellationToken;

pub(crate) async fn run(runtime: CreatorRuntime, cancel: CancellationToken) {
    loop {
        tokio::select! {
            _=cancel.cancelled()=>break,
            _=tokio::time::sleep(std::time::Duration::from_secs(2))=>{}
        }
        // Do not cancel a provider attempt halfway through. Shutdown joins this task
        // with a bound; a forced termination leaves the durable attempt uncertain.
        match std::panic::AssertUnwindSafe(runtime.run_gmail_bulk_item()).catch_unwind().await {
            Ok(Ok(_))=>{},
            Ok(Err(error))=>{
                log::error!("gmail bulk item failed: {error}");
                if let Err(error)=runtime.interrupt_gmail_bulk().await { log::error!("gmail bulk recovery failed: {error}"); }
            },
            Err(_)=>{
                log::error!("gmail bulk item panicked; pausing queue");
                if let Err(error)=runtime.interrupt_gmail_bulk().await { log::error!("gmail bulk panic recovery failed: {error}"); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_stops_idle_worker_without_waiting_for_poll() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = CreatorRuntime::open(directory.path(), Default::default()).unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        tokio::time::timeout(std::time::Duration::from_millis(100), run(runtime, cancel))
            .await
            .unwrap();
    }
}
