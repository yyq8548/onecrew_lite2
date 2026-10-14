//! Spawn Chrome for a fresh, isolated profile and ask it to close gracefully
//! (so cookies flush to disk). Uses `std::process` — launching a browser needs
//! no async runtime; the caller can wrap the blocking wait in `spawn_blocking`.

use std::path::PathBuf;
use std::process::{Child, Command};

use crate::ChromeError;
use crate::fingerprint::Fingerprint;

pub(crate) struct LaunchOptions {
    /// Path to the installed Chrome executable.
    pub binary: PathBuf,
    /// Fresh isolated `--user-data-dir` for this profile.
    pub profile_dir: PathBuf,
    /// Optional loopback no-auth proxy forwarder. Throwaway public-page windows
    /// use the host's direct connection and therefore leave this unset.
    pub proxy: Option<(String, u16)>,
    pub fingerprint: Fingerprint,
    /// Optional initial URL (the platform's login page).
    pub url: Option<String>,
}

pub(crate) fn spawn(opts: &LaunchOptions) -> std::io::Result<Child> {
    let mut cmd = Command::new(&opts.binary);
    cmd.arg(format!("--user-data-dir={}", opts.profile_dir.display()));
    if let Some((host, port)) = &opts.proxy {
        cmd.arg(format!("--proxy-server=http://{host}:{port}"));
    }
    // WebRTC must never leak the real IP behind the proxy: force UDP to only go
    // through paths the proxy can carry (falling back to proxied TCP), and kill
    // QUIC (which would bypass the HTTP proxy).
    cmd.arg("--force-webrtc-ip-handling-policy=disable_non_proxied_udp");
    cmd.arg("--disable-quic");
    // `--accept-lang` sets Accept-Language + navigator.language (the real
    // fingerprint knob); `--lang` (UI language) is best-effort.
    if let Some(lang) = &opts.fingerprint.accept_language {
        cmd.arg(format!("--accept-lang={lang}"));
        cmd.arg(format!("--lang={}", lang.split(',').next().unwrap_or(lang)));
    }
    // Clean, non-automation startup. NOT `--incognito` (it would wipe the login
    // cookies on exit); NOT `--remote-debugging-port` / `--enable-automation`
    // (those set automation tells — with neither, navigator.webdriver is false).
    for flag in [
        "--no-first-run",
        "--no-default-browser-check",
        "--no-service-autorun",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-client-side-phishing-detection",
        "--disable-sync",
        "--disable-default-apps",
        "--disable-background-mode",
        "--metrics-recording-only",
    ] {
        cmd.arg(flag);
    }
    if let Some(url) = &opts.url {
        cmd.arg(url);
    }
    // Chrome reads TZ via ICU once at startup (macOS/Linux), aligning
    // `Date`/`Intl` to the 家宽 region. Must be set before spawn.
    if let Some(tz) = &opts.fingerprint.timezone {
        cmd.env("TZ", tz);
    }
    cmd.spawn()
}

/// Close a launched Chrome process without leaking it on any error path.
///
/// Users may close the browser themselves before finalizing. In that case the
/// child has already exited and its cookie store can be read immediately. If a
/// graceful-close request or wait fails, forcefully reap the app-owned process
/// before returning the original error so its profile is never left locked.
pub(crate) fn close_and_wait(child: &mut Child) -> Result<(), ChromeError> {
    match child.try_wait() {
        Ok(Some(_)) => return Ok(()),
        Ok(None) => {}
        Err(error) => {
            force_cleanup(child);
            return Err(error.into());
        }
    }

    if let Err(error) = request_close(child) {
        force_cleanup(child);
        return Err(error);
    }
    if let Err(error) = wait_for_close(child) {
        force_cleanup(child);
        return Err(error);
    }
    Ok(())
}

fn force_cleanup(child: &mut Child) {
    // `kill` can legitimately report that a process exited between the failed
    // close request and cleanup. `wait` is still required to reap either case.
    if let Err(error) = child.kill() {
        log::debug!("browser force kill reported: {error}");
    }
    if let Err(error) = child.wait() {
        log::debug!("browser force wait reported: {error}");
    }
}

/// Ask Chrome to shut down *gracefully* so it flushes cookies to the SQLite
/// store. A hard kill (SIGKILL) could lose unwritten cookies.
#[cfg(unix)]
pub(crate) fn request_close(child: &mut Child) -> Result<(), ChromeError> {
    let process_id = child.id();
    let pid = process_id_to_pid(process_id).ok_or_else(|| {
        ChromeError::GracefulClose(format!(
            "Chrome process id {process_id} cannot be represented as a positive pid_t"
        ))
    })?;
    // SIGTERM → Chrome runs its normal shutdown (cookie flush) and exits.
    // SAFETY: `process_id_to_pid` proved `pid` is strictly positive and
    // representable as `pid_t`, so POSIX `kill` targets that one process rather
    // than a process group. `pid` came from this live `Child`; the exclusive
    // borrow prevents this wrapper from reaping it concurrently. `SIGTERM` is
    // a valid signal constant, the call receives no pointers, and its status is
    // checked immediately below.
    let result = unsafe { libc::kill(pid, libc::SIGTERM) };
    if result == 0 {
        Ok(())
    } else {
        Err(ChromeError::GracefulClose(
            std::io::Error::last_os_error().to_string(),
        ))
    }
}

#[cfg(unix)]
fn process_id_to_pid(process_id: u32) -> Option<libc::pid_t> {
    let pid = libc::pid_t::try_from(process_id).ok()?;
    (pid > 0).then_some(pid)
}

#[cfg(unix)]
pub(crate) fn wait_for_close(child: &mut Child) -> Result<(), ChromeError> {
    child.wait()?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn process_id_conversion_rejects_group_and_truncating_values() {
        assert_eq!(process_id_to_pid(0), None);
        assert_eq!(process_id_to_pid(1), Some(1));
        assert_eq!(process_id_to_pid(i32::MAX as u32), Some(i32::MAX));
        assert_eq!(process_id_to_pid(i32::MAX as u32 + 1), None);
        assert_eq!(process_id_to_pid(u32::MAX), None);
    }

    #[test]
    fn already_exited_child_is_a_successful_close() {
        let mut child = Command::new("true").spawn().unwrap();
        child.wait().unwrap();

        close_and_wait(&mut child).unwrap();
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn request_close(child: &mut Child) -> Result<(), ChromeError> {
    use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
    };
    use windows_sys::core::BOOL;

    struct CloseContext {
        pid: u32,
        matched: bool,
        window_query_error: Option<u32>,
        post_failed: bool,
    }

    /// # Safety
    ///
    /// `context` must be the unchanged `LPARAM` representation of an exclusive,
    /// live `CloseContext` pointer for the complete callback invocation. No
    /// other thread or callback may access that context concurrently.
    unsafe extern "system" fn close_window(hwnd: HWND, context: LPARAM) -> BOOL {
        // SAFETY: `request_close` passes the address of its aligned stack
        // `CloseContext` as an `LPARAM`. `EnumWindows` calls this callback
        // synchronously, one window at a time, without retaining that value;
        // the caller does not access the context until enumeration returns.
        // The cast therefore recovers the original provenance and alignment,
        // and this callback owns the only mutable reference for its duration.
        let context = unsafe { &mut *(context as *mut CloseContext) };
        let mut pid = 0;
        // SAFETY: `hwnd` is the top-level-window handle supplied for this
        // callback by `EnumWindows`. `pid` is live, aligned, initialized, and
        // exclusively writable for one `u32`. A concurrently destroyed window
        // is reported by the checked zero return and is not interpreted.
        let window_thread = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if window_thread == 0 {
            // SAFETY: `GetLastError` has no pointer or lifetime obligations and
            // is called immediately after `GetWindowThreadProcessId` returned
            // zero, before another native call on this thread can overwrite the
            // extended error code documented for an invalid window handle.
            context.window_query_error = Some(unsafe { GetLastError() });
            return 1;
        }
        if pid == context.pid {
            context.matched = true;
            // SAFETY: `hwnd` is the handle supplied by `EnumWindows`, and the
            // checked process id associates it with this launched child.
            // `WM_CLOSE` carries integer zero parameters, so no Rust pointer or
            // borrow is retained by the queued message. A stale/destroyed
            // handle is reported by the checked zero return.
            if unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) } == 0 {
                context.post_failed = true;
            }
        }
        1
    }

    let mut context = CloseContext {
        pid: child.id(),
        matched: false,
        window_query_error: None,
        post_failed: false,
    };
    // SAFETY: `context` is an initialized, aligned stack value that remains
    // live and stationary for the call. Its exclusive pointer is represented
    // losslessly by pointer-width `LPARAM`. `EnumWindows` invokes callbacks
    // synchronously and sequentially before returning and does not retain the
    // application value; `close_window` upholds that pointer contract. No Rust
    // access to `context` occurs until this call completes, and the return
    // status is checked before its fields are read.
    let enumerated = unsafe {
        EnumWindows(
            Some(close_window),
            (&mut context as *mut CloseContext) as LPARAM,
        )
    };
    if enumerated == 0 {
        // SAFETY: `GetLastError` has no pointer or lifetime obligations and is
        // called immediately after the failed `EnumWindows` call, before any
        // other native call on this thread can overwrite its error value.
        let error = unsafe { GetLastError() };
        return Err(ChromeError::GracefulClose(format!(
            "EnumWindows failed with Windows error {}",
            error
        )));
    }
    if let Some(error) = context.window_query_error {
        return Err(ChromeError::GracefulClose(format!(
            "a Chrome window process id could not be queried during enumeration: Windows error {error}"
        )));
    }
    if !context.matched {
        return Err(ChromeError::GracefulClose(
            "no top-level Chrome window belonged to the launched browser process".into(),
        ));
    }
    if context.post_failed {
        return Err(ChromeError::GracefulClose(
            "WM_CLOSE could not be posted to every Chrome window".into(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn wait_for_close(child: &mut Child) -> Result<(), ChromeError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    const GRACEFUL_CLOSE_TIMEOUT_MS: u32 = 15_000;
    let handle = child.as_raw_handle().cast();
    // SAFETY: `handle` is borrowed from this live `Child` and remains open for
    // the entire wait because the exclusive borrow prevents concurrent wait,
    // close, or drop through this wrapper. `WaitForSingleObject` neither owns
    // nor closes the handle. Every documented result, including invalid-handle
    // failure, is handled below before the child is reaped.
    let result = unsafe { WaitForSingleObject(handle, GRACEFUL_CLOSE_TIMEOUT_MS) };
    match result {
        WAIT_OBJECT_0 => {
            child.wait()?;
            Ok(())
        }
        WAIT_TIMEOUT => {
            // The caller performs the common forced cleanup. A timeout means
            // cookie flush is unproven, so extraction must not proceed.
            Err(ChromeError::CloseTimeout)
        }
        WAIT_FAILED => Err(ChromeError::GracefulClose(
            std::io::Error::last_os_error().to_string(),
        )),
        other => Err(ChromeError::GracefulClose(format!(
            "unexpected process wait result {other}"
        ))),
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub(crate) fn request_close(child: &mut Child) -> Result<(), ChromeError> {
    child.kill()?;
    Ok(())
}

#[cfg(not(any(unix, target_os = "windows")))]
pub(crate) fn wait_for_close(child: &mut Child) -> Result<(), ChromeError> {
    child.wait()?;
    Ok(())
}
