//! Thin Tokio boundary for synchronous Creator repository segments.

use std::{fmt, sync::Arc, time::Duration};

use tokio::sync::Semaphore;

use crate::repository::SQLITE_READ_POOL_SIZE;

const REPOSITORY_ADMISSION_WAIT: Duration = Duration::from_millis(100);
const REPOSITORY_WRITER_CAPACITY: usize = 1;
const CPU_SEGMENT_CAPACITY: usize = 2;

#[cfg(test)]
mod typed_usecase_tests {
    use super::*;
    use crate::{CreatorGateways, CreatorRuntime, ManualCreatorCreateInput};

    fn input(name: &str) -> ManualCreatorCreateInput {
        ManualCreatorCreateInput {
            display_name: name.into(),
            notes: None,
            tags: None,
            contact_routes: vec![],
        }
    }

    #[tokio::test]
    async fn typed_creator_usecases_admit_reads_writes_and_cpu_before_dispatch() {
        let directory = tempfile::tempdir().unwrap();
        let owner = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
        let read_permit = owner
            .repository_dispatch
            .reads
            .clone()
            .acquire_many_owned(SQLITE_READ_POOL_SIZE)
            .await
            .unwrap();
        assert!(matches!(
            owner
                .get_creator_async(crate::CreatorId::new("missing"))
                .await,
            Err(CreatorRepositoryCallError::Dispatch(
                CreatorRepositoryDispatchFailure::Overloaded
            ))
        ));
        drop(read_permit);
        assert!(matches!(
            owner
                .get_creator_async(crate::CreatorId::new("missing"))
                .await,
            Err(CreatorRepositoryCallError::Owner(_))
        ));

        let write_permit = owner
            .repository_dispatch
            .writes
            .clone()
            .acquire_owned()
            .await
            .unwrap();
        assert!(matches!(
            owner
                .prepare_creator_contact_route_delete_async(
                    crate::PrepareCreatorContactRouteDeleteInput {
                        creator_id: crate::CreatorId::new("missing"),
                        contact_route_id: crate::CreatorContactRouteId::new("missing"),
                    }
                )
                .await,
            Err(CreatorRepositoryCallError::Dispatch(
                CreatorRepositoryDispatchFailure::Overloaded
            ))
        ));
        assert!(matches!(
            owner
                .create_manual_creator_async(input("not created"))
                .await,
            Err(CreatorRepositoryCallError::Dispatch(
                CreatorRepositoryDispatchFailure::Overloaded
            ))
        ));
        assert_eq!(
            owner
                .list_creators_async(crate::ListCreatorsInput::default())
                .await
                .unwrap()
                .items
                .len(),
            0
        );
        drop(write_permit);
        let created = owner
            .create_manual_creator_async(input("created"))
            .await
            .unwrap();
        assert_eq!(
            owner
                .get_creator_async(created.creator_id)
                .await
                .unwrap()
                .creator
                .display_name,
            "created"
        );

        let cpu_permit = owner
            .repository_dispatch
            .cpu
            .clone()
            .acquire_many_owned(CPU_SEGMENT_CAPACITY as u32)
            .await
            .unwrap();
        assert!(matches!(
            owner.export_creator_workbook_xlsx_async().await,
            Err(CreatorRepositoryCallError::Dispatch(
                CreatorRepositoryDispatchFailure::Overloaded
            ))
        ));
        assert_eq!(
            owner.repository_dispatch.reads.available_permits(),
            SQLITE_READ_POOL_SIZE as usize
        );
        drop(cpu_permit);
        assert!(
            !owner
                .export_creator_workbook_xlsx_async()
                .await
                .unwrap()
                .bytes
                .is_empty()
        );

        owner.close_repository_intake();
        assert!(matches!(
            owner
                .get_creator_async(crate::CreatorId::new("missing"))
                .await,
            Err(CreatorRepositoryCallError::Dispatch(
                CreatorRepositoryDispatchFailure::Closed
            ))
        ));
        assert!(matches!(
            owner.create_manual_creator_async(input("closed")).await,
            Err(CreatorRepositoryCallError::Dispatch(
                CreatorRepositoryDispatchFailure::Closed
            ))
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn typed_creator_started_write_retains_admission_after_caller_cancellation() {
        let directory = tempfile::tempdir().unwrap();
        let owner = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
        let (entered, release) = owner.install_next_repository_write_barrier();
        let worker = owner.clone();
        let caller = tokio::spawn(async move {
            worker
                .create_manual_creator_async(input("completed after cancellation"))
                .await
        });
        entered.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert_eq!(owner.repository_dispatch.writes.available_permits(), 0);
        assert!(matches!(
            owner.create_manual_creator_async(input("still full")).await,
            Err(CreatorRepositoryCallError::Dispatch(
                CreatorRepositoryDispatchFailure::Overloaded
            ))
        ));
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let page = owner
                    .list_creators_async(crate::ListCreatorsInput::default())
                    .await
                    .unwrap();
                if page
                    .items
                    .iter()
                    .any(|row| row.display_name == "completed after cancellation")
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

/// Failure to dispatch or recover the result of one synchronous repository
/// segment through the bounded Tokio boundary.
///
/// The pre-dispatch variants are known to have no repository effect.
/// [`Self::OutcomeUnknown`] means blocking work may have run: read-only owners
/// may retry, while write owners must use their durable idempotency/readback
/// contract before deciding whether another write is safe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatorRepositoryDispatchFailure {
    /// Runtime shutdown closed admission before this call was accepted.
    Closed,
    /// Bounded admission stayed full for the complete admission window.
    Overloaded,
    /// Tokio cancelled the blocking task before its closure began.
    CancelledBeforeDispatch,
    /// Blocking work started, but panic or join loss removed its result.
    OutcomeUnknown,
}

impl CreatorRepositoryDispatchFailure {
    /// Whether the repository closure is known not to have started.
    pub const fn known_no_effect(self) -> bool {
        matches!(
            self,
            Self::Closed | Self::Overloaded | Self::CancelledBeforeDispatch
        )
    }
}

impl fmt::Display for CreatorRepositoryDispatchFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Closed => "Creator repository intake is closed",
            Self::Overloaded => "Creator repository capacity is temporarily full",
            Self::CancelledBeforeDispatch => {
                "Creator repository work was cancelled before dispatch"
            }
            Self::OutcomeUnknown => "Creator repository work started but its outcome is unknown",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CreatorRepositoryDispatchFailure {}

/// Error returned by a typed async repository wrapper.
#[derive(Debug)]
pub enum CreatorRepositoryCallError<E> {
    /// The repository owner ran and returned its own domain error.
    Owner(E),
    /// The bounded dispatch boundary failed before or after owner execution.
    Dispatch(CreatorRepositoryDispatchFailure),
}

impl<E: fmt::Display> fmt::Display for CreatorRepositoryCallError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owner(error) => error.fmt(formatter),
            Self::Dispatch(error) => error.fmt(formatter),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for CreatorRepositoryCallError<E> {}

#[derive(Clone)]
pub(crate) struct RepositoryDispatch {
    reads: Arc<Semaphore>,
    writes: Arc<Semaphore>,
    cpu: Arc<Semaphore>,
}

impl RepositoryDispatch {
    pub(crate) fn new() -> Self {
        Self {
            reads: Arc::new(Semaphore::new(SQLITE_READ_POOL_SIZE as usize)),
            writes: Arc::new(Semaphore::new(REPOSITORY_WRITER_CAPACITY)),
            cpu: Arc::new(Semaphore::new(CPU_SEGMENT_CAPACITY)),
        }
    }

    pub(crate) async fn read<T, F>(&self, work: F) -> Result<T, CreatorRepositoryDispatchFailure>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        Self::run(Arc::clone(&self.reads), work).await
    }

    pub(crate) async fn write<T, F>(&self, work: F) -> Result<T, CreatorRepositoryDispatchFailure>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        Self::run(Arc::clone(&self.writes), work).await
    }

    pub(crate) async fn cpu<T, F>(&self, work: F) -> Result<T, CreatorRepositoryDispatchFailure>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        Self::run(Arc::clone(&self.cpu), work).await
    }

    pub(crate) fn close(&self) {
        self.reads.close();
        self.writes.close();
        self.cpu.close();
    }

    async fn run<T, F>(
        admission: Arc<Semaphore>,
        work: F,
    ) -> Result<T, CreatorRepositoryDispatchFailure>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        // Capacity is acquired before spawn_blocking, so overload never fills
        // Tokio's blocking queue with work that only waits for a DB slot.
        let permit = match tokio::time::timeout(
            REPOSITORY_ADMISSION_WAIT,
            Arc::clone(&admission).acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => return Err(CreatorRepositoryDispatchFailure::Closed),
            Err(_) => return Err(CreatorRepositoryDispatchFailure::Overloaded),
        };
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work()
        })
        .await
        .map_err(|error| {
            if error.is_cancelled() {
                CreatorRepositoryDispatchFailure::CancelledBeforeDispatch
            } else {
                // Tokio cannot abort a running blocking closure. A panic or
                // lost join therefore cannot be called no-effect; durable
                // idempotency/readback is the operation owner's recovery.
                CreatorRepositoryDispatchFailure::OutcomeUnknown
            }
        })
    }
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct RepositorySegmentTestHooks {
    next_read: std::sync::Mutex<
        std::collections::VecDeque<(
            tokio::sync::oneshot::Sender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
    next_write: std::sync::Mutex<
        std::collections::VecDeque<(
            tokio::sync::oneshot::Sender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
    next_cpu: std::sync::Mutex<
        std::collections::VecDeque<(
            tokio::sync::oneshot::Sender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
}

#[cfg(test)]
impl RepositorySegmentTestHooks {
    fn before_read(&self) {
        let barrier = self.next_read.lock().unwrap().pop_front();
        if let Some((entered, release)) = barrier {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
    }

    fn before_write(&self) {
        let barrier = self.next_write.lock().unwrap().pop_front();
        if let Some((entered, release)) = barrier {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
    }

    fn before_cpu(&self) {
        let barrier = self.next_cpu.lock().unwrap().pop_front();
        if let Some((entered, release)) = barrier {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
    }
}

impl crate::CreatorRuntime {
    /// Reject new async Creator repository segments. Already-dispatched blocking
    /// work continues under Tokio ownership and remains recoverable through
    /// its business idempotency/readback path.
    pub fn close_repository_intake(&self) {
        self.repository_dispatch.close();
    }

    pub(crate) async fn repository_read_segment<T, E, F>(
        &self,
        work: F,
    ) -> Result<T, CreatorRepositoryCallError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        #[cfg(test)]
        let work = {
            let hooks = Arc::clone(&self.repository_segment_test_hooks);
            move || {
                hooks.before_read();
                work()
            }
        };
        self.repository_dispatch
            .read(work)
            .await
            .map_err(CreatorRepositoryCallError::Dispatch)?
            .map_err(CreatorRepositoryCallError::Owner)
    }

    #[cfg(test)]
    pub(super) fn install_next_repository_read_barrier(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        self.repository_segment_test_hooks
            .next_read
            .lock()
            .unwrap()
            .push_back((entered_tx, release_rx));
        (entered_rx, release_tx)
    }

    pub(crate) async fn repository_write_segment<T, E, F>(
        &self,
        work: F,
    ) -> Result<T, CreatorRepositoryCallError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        self.supervised_repository_writer(work)
            .await
            .map_err(CreatorRepositoryCallError::Dispatch)?
            .map_err(CreatorRepositoryCallError::Owner)
    }

    pub(crate) async fn repository_cpu_segment<T, E, F>(
        &self,
        work: F,
    ) -> Result<T, CreatorRepositoryCallError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        #[cfg(test)]
        let work = {
            let hooks = Arc::clone(&self.repository_segment_test_hooks);
            move || {
                hooks.before_cpu();
                work()
            }
        };
        self.repository_dispatch
            .cpu(work)
            .await
            .map_err(CreatorRepositoryCallError::Dispatch)?
            .map_err(CreatorRepositoryCallError::Owner)
    }

    /// Run one synchronous repository writer closure under bounded admission.
    /// Once the closure starts it remains Tokio-owned even if its caller is
    /// dropped, so effect-coupled work belongs inside `work`, not after this
    /// future's await point.
    pub(super) async fn supervised_repository_writer<T, F>(
        &self,
        work: F,
    ) -> Result<T, CreatorRepositoryDispatchFailure>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        #[cfg(test)]
        let work = {
            let hooks = Arc::clone(&self.repository_segment_test_hooks);
            move || {
                hooks.before_write();
                work()
            }
        };
        self.repository_dispatch.write(work).await
    }

    #[cfg(test)]
    pub(super) fn install_next_repository_write_barrier(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        self.repository_segment_test_hooks
            .next_write
            .lock()
            .unwrap()
            .push_back((entered_tx, release_rx));
        (entered_rx, release_tx)
    }

    #[cfg(test)]
    pub(super) fn install_next_repository_cpu_barrier(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        self.repository_segment_test_hooks
            .next_cpu
            .lock()
            .unwrap()
            .push_back((entered_tx, release_rx));
        (entered_rx, release_tx)
    }
}
