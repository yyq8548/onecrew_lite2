//! Process-shared Resend quota coordination.
//!
//! Resend's documented limit is 10 requests/second **per team**, shared by
//! every API key of that team — and another computer may hold a key for the
//! same team. The provider exposes no safe team identity to partition on
//! (deriving one from key material is forbidden), so one conservative
//! process-wide pool covers every Resend operation in this process: list,
//! detail, attachment, credential validation, and user-initiated sends.

use std::sync::Mutex;

use tokio::sync::{Notify, Semaphore, SemaphorePermit};
use tokio::time::{Duration, Instant};

use onecrew_email_contracts::ProviderRateLimitStatus;

/// Conservative fallback when a response does not carry rate-limit headers.
///
/// The documented default is 10 requests/second per team. We retain a
/// five-request/second fallback so an independent process using the same team
/// still has room. Once the provider tells us its current window, pacing is
/// derived from those headers rather than assuming this value (or a guessed
/// 100 ms interval).
const REQUEST_INTERVAL: Duration = Duration::from_millis(200);
/// Maximum provider requests in flight at once.
const MAX_IN_FLIGHT: usize = 4;
/// A background acquire defers to waiting user operations at most this many
/// times before it takes the next slot regardless — priority without
/// starvation.
const MAX_BACKGROUND_DEFERRALS: u32 = 8;
/// Ceiling for pause windows derived from provider headers.
const MAX_PAUSE: Duration = Duration::from_secs(300);

/// User-initiated operations (send, readback, credential probes, attachment
/// opens) jump ahead of historical backfill; backfill still progresses via
/// the deferral bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResendOpPriority {
    User,
    Background,
}

#[derive(Debug)]
struct QuotaState {
    next_slot: Option<Instant>,
    pause_until: Option<Instant>,
    user_waiting: usize,
    /// Minimum spacing between starts in the current provider window. This is
    /// a capacity-one token bucket: concurrent callers can never turn a newly
    /// learned rate into a request burst.
    interval: Duration,
    /// Number of slot reservations handed to callers but not started yet.
    /// While non-zero, a response cannot move `next_slot` backwards ahead of
    /// already queued work.
    reservations: usize,
}

/// One shared coordinator per process. The internal mutex is synchronous and
/// never held across an await.
#[derive(Debug)]
pub(crate) struct ResendQuota {
    state: Mutex<QuotaState>,
    in_flight: Semaphore,
    user_done: Notify,
}

impl Default for ResendQuota {
    fn default() -> Self {
        Self {
            state: Mutex::new(QuotaState {
                next_slot: None,
                pause_until: None,
                user_waiting: 0,
                interval: REQUEST_INTERVAL,
                reservations: 0,
            }),
            in_flight: Semaphore::new(MAX_IN_FLIGHT),
            user_done: Notify::new(),
        }
    }
}

/// Holding this permit bounds in-flight provider work; drop it when the
/// response has been fully consumed.
pub struct ResendQuotaPermit<'a> {
    _in_flight: SemaphorePermit<'a>,
}

/// Reclaims a pacing reservation when an acquire future is cancelled while it
/// sleeps. Without this guard, one cancelled detail could leave the quota
/// believing a slot was permanently queued and unnecessarily suppress later
/// work.
struct SlotReservation<'a> {
    quota: &'a ResendQuota,
    armed: bool,
}

impl Drop for SlotReservation<'_> {
    fn drop(&mut self) {
        if self.armed {
            let mut state = self.quota.state.lock().expect("quota mutex poisoned");
            state.reservations = state.reservations.saturating_sub(1);
        }
    }
}

struct UserWaitingGuard<'a> {
    quota: &'a ResendQuota,
    armed: bool,
}

impl Drop for UserWaitingGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let mut state = self.quota.state.lock().expect("quota mutex poisoned");
            state.user_waiting = state.user_waiting.saturating_sub(1);
            drop(state);
            self.quota.user_done.notify_waiters();
        }
    }
}

impl ResendQuota {
    /// Waits for an in-flight slot and the next request slot. Cancellation
    /// safe: dropping the future releases the in-flight permit and any
    /// user-waiting registration.
    pub(crate) async fn acquire(&self, priority: ResendOpPriority) -> ResendQuotaPermit<'_> {
        // Register foreground work before waiting for the semaphore. A
        // background detail wave that already occupies all permits may still
        // drain, but newly queued background work now observes the foreground
        // waiter and cannot continuously refill the queue ahead of it.
        let mut user_guard = UserWaitingGuard {
            quota: self,
            armed: false,
        };
        if priority == ResendOpPriority::User {
            self.state
                .lock()
                .expect("quota mutex poisoned")
                .user_waiting += 1;
            user_guard.armed = true;
        }
        let in_flight = self
            .in_flight
            .acquire()
            .await
            .expect("quota semaphore closed");

        let mut deferrals: u32 = 0;
        loop {
            let decision = {
                let mut state = self.state.lock().expect("quota mutex poisoned");
                if priority == ResendOpPriority::Background
                    && state.user_waiting > 0
                    && deferrals < MAX_BACKGROUND_DEFERRALS
                {
                    None
                } else {
                    let now = Instant::now();
                    let slot = [state.next_slot, state.pause_until]
                        .into_iter()
                        .flatten()
                        .fold(now, Instant::max);
                    state.next_slot = Some(slot + state.interval);
                    state.reservations += 1;
                    Some(slot)
                }
            };
            match decision {
                Some(slot) => {
                    let mut reservation = SlotReservation {
                        quota: self,
                        armed: true,
                    };
                    tokio::time::sleep_until(slot).await;
                    reservation.armed = false;
                    {
                        let mut state = self.state.lock().expect("quota mutex poisoned");
                        state.reservations = state.reservations.saturating_sub(1);
                    }
                    drop(user_guard);
                    return ResendQuotaPermit {
                        _in_flight: in_flight,
                    };
                }
                None => {
                    deferrals += 1;
                    // A missed notification only costs one interval of sleep,
                    // never a hang.
                    let interval = self.state.lock().expect("quota mutex poisoned").interval;
                    tokio::select! {
                        () = self.user_done.notified() => {}
                        () = tokio::time::sleep(interval) => {}
                    }
                }
            }
        }
    }

    /// Feeds response headers back into pacing. Resend's remaining budget is
    /// spread over the reset window, while the advertised limit is used when
    /// remaining is absent. This is intentionally a capacity-one bucket:
    /// learning a faster window never releases several queued requests at
    /// once. An exhausted window pauses new requests until its reset.
    pub(crate) fn observe(&self, rate_limit: ProviderRateLimitStatus) {
        let interval = interval_from_headers(rate_limit);
        {
            let mut state = self.state.lock().expect("quota mutex poisoned");
            state.interval = interval;
            // If no callers are queued behind a slot, the response we just
            // consumed is the pacing reference. Rebase to the newly learned
            // interval so both faster and slower provider windows take effect
            // on the next request. Existing reservations remain untouched.
            if state.reservations == 0 {
                state.next_slot = Some(Instant::now() + interval);
            }
        }
        if rate_limit.remaining == Some(0) {
            let reset = rate_limit
                .reset_seconds
                .unwrap_or(1)
                .clamp(1, MAX_PAUSE.as_secs());
            self.pause_for(Duration::from_secs(reset));
        }
    }

    /// A transient 429 pauses the whole pool for the provider-directed
    /// window (`retry-after`, else reset, else one interval), bounded.
    pub(crate) fn report_rate_limited(&self, rate_limit: ProviderRateLimitStatus) -> Duration {
        let interval = interval_from_headers(rate_limit);
        {
            let mut state = self.state.lock().expect("quota mutex poisoned");
            state.interval = interval;
            if state.reservations == 0 {
                state.next_slot = Some(Instant::now() + interval);
            }
        }
        let seconds = rate_limit
            .retry_after_seconds
            .or(rate_limit.reset_seconds)
            .unwrap_or(1);
        let pause = Duration::from_secs(seconds.clamp(1, MAX_PAUSE.as_secs())).min(MAX_PAUSE);
        self.pause_for(pause);
        pause
    }

    fn pause_for(&self, pause: Duration) {
        let until = Instant::now() + pause;
        let mut state = self.state.lock().expect("quota mutex poisoned");
        if state.pause_until.is_none_or(|current| current < until) {
            state.pause_until = Some(until);
        }
    }

    #[cfg(test)]
    fn user_waiting(&self) -> usize {
        self.state
            .lock()
            .expect("quota mutex poisoned")
            .user_waiting
    }

    #[cfg(test)]
    fn reservations(&self) -> usize {
        self.state
            .lock()
            .expect("quota mutex poisoned")
            .reservations
    }
}

/// Calculate one-token spacing from the provider's current window.
///
/// `ratelimit-reset` is the number of seconds until the window resets. If
/// `ratelimit-remaining` is present, include the request that just completed
/// (`remaining + 1`) so the requests still available are distributed through
/// the rest of the window. Otherwise use the advertised limit as the window
/// capacity. Missing or invalid combinations retain the conservative
/// fallback. The arithmetic is saturating and bounded so malformed headers
/// cannot create a zero-duration (bursting) or unreasonably large sleep.
fn interval_from_headers(rate_limit: ProviderRateLimitStatus) -> Duration {
    const MIN_INTERVAL: Duration = Duration::from_millis(1);
    const MAX_INTERVAL: Duration = Duration::from_secs(300);

    let slots = rate_limit
        .remaining
        .map(|remaining| remaining.saturating_add(1))
        .or(rate_limit.limit)
        .filter(|slots| *slots > 0);
    let reset_seconds = rate_limit.reset_seconds.filter(|seconds| *seconds > 0);
    let Some((slots, reset_seconds)) = slots.zip(reset_seconds) else {
        // A limit without a reset is still useful when providers use a
        // conventional one-second window (the documented Resend default).
        return rate_limit
            .limit
            .filter(|limit| *limit > 0)
            .map(|limit| duration_for_ratio(1, limit))
            .unwrap_or(REQUEST_INTERVAL)
            .clamp(MIN_INTERVAL, MAX_INTERVAL);
    };

    let slots = rate_limit
        .limit
        .filter(|limit| *limit > 0)
        .map_or(slots, |limit| slots.min(limit));
    duration_for_ratio(reset_seconds, slots).clamp(MIN_INTERVAL, MAX_INTERVAL)
}

fn duration_for_ratio(seconds: u64, slots: u64) -> Duration {
    // Keep sub-second windows precise enough for high, but valid, provider
    // limits without ever allowing an immediate second acquire.
    let nanos = u128::from(seconds)
        .saturating_mul(1_000_000_000)
        .saturating_add(u128::from(slots).saturating_sub(1))
        / u128::from(slots.max(1));
    let nanos = nanos.min(u128::from(u64::MAX));
    Duration::from_nanos(nanos as u64)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn no_headers() -> ProviderRateLimitStatus {
        ProviderRateLimitStatus {
            limit: None,
            remaining: None,
            reset_seconds: None,
            retry_after_seconds: None,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn requests_are_spaced_by_the_configured_interval() {
        let quota = ResendQuota::default();
        let started = Instant::now();
        for _ in 0..3 {
            drop(quota.acquire(ResendOpPriority::Background).await);
        }
        assert_eq!(started.elapsed(), REQUEST_INTERVAL * 2);
    }

    #[tokio::test(start_paused = true)]
    async fn response_headers_adapt_spacing_without_releasing_a_burst() {
        let quota = ResendQuota::default();

        // A 20-request window produces 50 ms spacing; this is learned from
        // the provider, not a hard-coded replacement for the fallback.
        quota.observe(ProviderRateLimitStatus {
            limit: Some(20),
            remaining: Some(19),
            reset_seconds: Some(1),
            retry_after_seconds: None,
        });
        drop(quota.acquire(ResendOpPriority::User).await);
        let first = Instant::now();
        drop(quota.acquire(ResendOpPriority::User).await);
        let second = Instant::now();
        assert_eq!(second.duration_since(first), Duration::from_millis(50));

        // A slower five-request window takes effect on the next request and
        // keeps every subsequent acquire at least one full interval apart.
        quota.observe(ProviderRateLimitStatus {
            limit: Some(5),
            remaining: Some(4),
            reset_seconds: Some(1),
            retry_after_seconds: None,
        });
        drop(quota.acquire(ResendOpPriority::User).await);
        let third = Instant::now();
        assert_eq!(third.duration_since(second), Duration::from_millis(200));
        drop(quota.acquire(ResendOpPriority::User).await);
        let fourth = Instant::now();
        assert_eq!(fourth.duration_since(third), Duration::from_millis(200));
    }

    #[tokio::test(start_paused = true)]
    async fn cancelling_a_queued_slot_reclaims_its_reservation() {
        let quota = Arc::new(ResendQuota::default());
        quota.observe(ProviderRateLimitStatus {
            limit: Some(5),
            remaining: Some(4),
            reset_seconds: Some(1),
            retry_after_seconds: None,
        });

        let waiter = tokio::spawn({
            let quota = Arc::clone(&quota);
            async move {
                drop(quota.acquire(ResendOpPriority::Background).await);
            }
        });
        tokio::task::yield_now().await;
        assert_eq!(quota.reservations(), 1);
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        assert_eq!(quota.reservations(), 0);

        // With the cancelled 200 ms reservation gone, the newly learned
        // 20-request window applies to the next acquire immediately.
        quota.observe(ProviderRateLimitStatus {
            limit: Some(20),
            remaining: Some(19),
            reset_seconds: Some(1),
            retry_after_seconds: None,
        });
        let started = Instant::now();
        drop(quota.acquire(ResendOpPriority::User).await);
        assert_eq!(started.elapsed(), Duration::from_millis(50));
    }

    #[test]
    fn header_interval_uses_remaining_budget_and_safe_fallbacks() {
        assert_eq!(
            interval_from_headers(ProviderRateLimitStatus {
                limit: Some(5),
                remaining: Some(4),
                reset_seconds: Some(1),
                retry_after_seconds: None,
            }),
            Duration::from_millis(200)
        );
        assert_eq!(
            interval_from_headers(ProviderRateLimitStatus {
                limit: Some(20),
                remaining: Some(19),
                reset_seconds: Some(1),
                retry_after_seconds: None,
            }),
            Duration::from_millis(50)
        );
        assert_eq!(
            interval_from_headers(ProviderRateLimitStatus {
                limit: None,
                remaining: None,
                reset_seconds: None,
                retry_after_seconds: None,
            }),
            REQUEST_INTERVAL
        );
        // A zero/negative equivalent is not a valid window and must not
        // produce a zero-duration acquire that bursts requests.
        assert!(
            interval_from_headers(ProviderRateLimitStatus {
                limit: Some(1),
                remaining: Some(0),
                reset_seconds: Some(0),
                retry_after_seconds: None,
            }) >= Duration::from_millis(1)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_rate_limit_report_pauses_the_pool_for_retry_after() {
        let quota = ResendQuota::default();
        drop(quota.acquire(ResendOpPriority::User).await);
        let pause = quota.report_rate_limited(ProviderRateLimitStatus {
            retry_after_seconds: Some(2),
            ..no_headers()
        });
        assert_eq!(pause, Duration::from_secs(2));
        let started = Instant::now();
        drop(quota.acquire(ResendOpPriority::User).await);
        assert!(started.elapsed() >= Duration::from_secs(2));
    }

    #[tokio::test(start_paused = true)]
    async fn an_exhausted_window_pauses_until_reset() {
        let quota = ResendQuota::default();
        quota.observe(ProviderRateLimitStatus {
            remaining: Some(0),
            reset_seconds: Some(3),
            ..no_headers()
        });
        let started = Instant::now();
        drop(quota.acquire(ResendOpPriority::Background).await);
        assert!(started.elapsed() >= Duration::from_secs(3));
    }

    #[tokio::test(start_paused = true)]
    async fn background_defers_to_waiting_user_work_but_is_never_starved() {
        let quota = Arc::new(ResendQuota::default());
        let order = Arc::new(Mutex::new(Vec::new()));

        // Push every slot behind a pause so both contenders must wait, with
        // the user registered first.
        quota.pause_for(Duration::from_secs(2));
        let user = tokio::spawn({
            let quota = Arc::clone(&quota);
            let order = Arc::clone(&order);
            async move {
                drop(quota.acquire(ResendOpPriority::User).await);
                order.lock().unwrap().push("user");
            }
        });
        tokio::task::yield_now().await;
        assert_eq!(quota.user_waiting(), 1);

        let background = tokio::spawn({
            let quota = Arc::clone(&quota);
            let order = Arc::clone(&order);
            async move {
                drop(quota.acquire(ResendOpPriority::Background).await);
                order.lock().unwrap().push("background");
            }
        });

        user.await.unwrap();
        background.await.unwrap();
        assert_eq!(*order.lock().unwrap(), vec!["user", "background"]);
        assert_eq!(quota.user_waiting(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn background_progresses_under_sustained_user_load() {
        let quota = Arc::new(ResendQuota::default());
        let user_grants = Arc::new(AtomicUsize::new(0));

        let background = tokio::spawn({
            let quota = Arc::clone(&quota);
            async move {
                drop(quota.acquire(ResendOpPriority::Background).await);
                Instant::now()
            }
        });
        let users = tokio::spawn({
            let quota = Arc::clone(&quota);
            let user_grants = Arc::clone(&user_grants);
            async move {
                for _ in 0..32 {
                    drop(quota.acquire(ResendOpPriority::User).await);
                    user_grants.fetch_add(1, Ordering::SeqCst);
                }
            }
        });

        let finished = background.await.unwrap();
        assert!(
            finished.elapsed() < Duration::from_secs(60),
            "background acquire must complete despite continuous user work"
        );
        assert!(
            user_grants.load(Ordering::SeqCst) < 32,
            "the deferral bound admits background before user work drains"
        );
        users.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn cancelling_a_user_acquire_releases_its_priority_claim() {
        let quota = Arc::new(ResendQuota::default());
        // Exhaust in-flight permits so the user acquire blocks on the
        // semaphore while registered... it registers only after the permit,
        // so block on the slot instead: take the slot far into the future.
        quota.pause_for(Duration::from_secs(100));
        let waiter = tokio::spawn({
            let quota = Arc::clone(&quota);
            async move {
                drop(quota.acquire(ResendOpPriority::User).await);
            }
        });
        tokio::task::yield_now().await;
        assert_eq!(quota.user_waiting(), 1);
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        assert_eq!(
            quota.user_waiting(),
            0,
            "an aborted user acquire must not leave background work deferring forever"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn in_flight_work_is_bounded() {
        let quota = Arc::new(ResendQuota::default());
        let concurrent = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..12 {
            tasks.push(tokio::spawn({
                let quota = Arc::clone(&quota);
                let concurrent = Arc::clone(&concurrent);
                let peak = Arc::clone(&peak);
                async move {
                    let permit = quota.acquire(ResendOpPriority::Background).await;
                    let now = concurrent.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    concurrent.fetch_sub(1, Ordering::SeqCst);
                    drop(permit);
                }
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert!(peak.load(Ordering::SeqCst) <= MAX_IN_FLIGHT);
    }
}
