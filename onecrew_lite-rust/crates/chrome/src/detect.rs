//! Locate the user's installed Google Chrome executable and read its version.

use std::path::{Path, PathBuf};
#[cfg(not(target_os = "windows"))]
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeChannel {
    Stable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeInstallScope {
    ExplicitOverride,
    CurrentUser,
    LocalMachine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedChrome {
    pub executable: PathBuf,
    pub version: Option<String>,
    pub channel: ChromeChannel,
    pub install_scope: ChromeInstallScope,
}

/// Resolve a Chrome launcher: the explicit `CHROME_BIN` override first, then
/// OS-native discovery. Windows checks the registered App Path, the per-user
/// install, and both Program Files roots; no drive letter is assumed.
pub fn detect_chrome() -> Option<PathBuf> {
    detect_chrome_path().map(|(executable, _)| executable)
}

pub fn detect_chrome_install() -> Option<DetectedChrome> {
    let (executable, install_scope) = detect_chrome_path()?;
    Some(detected(executable, install_scope))
}

fn detect_chrome_path() -> Option<(PathBuf, ChromeInstallScope)> {
    if let Some(path) = std::env::var_os("CHROME_BIN").map(PathBuf::from)
        && path.is_file()
    {
        return Some((path, ChromeInstallScope::ExplicitOverride));
    }

    #[cfg(target_os = "windows")]
    for (path, scope) in windows_candidates() {
        if path.is_file() {
            return Some((path, scope));
        }
    }

    #[cfg(not(target_os = "windows"))]
    for path in candidates() {
        if path.is_file() {
            return Some((path, ChromeInstallScope::LocalMachine));
        }
    }

    // macOS also allows a per-user install under ~/Applications.
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        let path =
            PathBuf::from(home).join("Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        if path.is_file() {
            return Some((path, ChromeInstallScope::CurrentUser));
        }
    }
    None
}

fn detected(executable: PathBuf, install_scope: ChromeInstallScope) -> DetectedChrome {
    DetectedChrome {
        version: chrome_version(&executable),
        executable,
        channel: ChromeChannel::Stable,
        install_scope,
    }
}

/// The installed browser's major version (e.g. `"151"`), read from
/// `<binary> --version`. Chrome installations differ on whether version text is
/// written to stdout or stderr, so both streams are accepted.
#[cfg(not(target_os = "windows"))]
#[expect(
    clippy::let_and_return,
    reason = "the binding keeps borrowed lossy strings inside output's lifetime; Clippy's tail rewrite fails with E0597"
)]
pub fn chrome_version(binary: &Path) -> Option<String> {
    let output = Command::new(binary).arg("--version").output().ok()?;
    output.status.success().then_some(())?;
    // The binding is load-bearing: the lossy strings borrow `output`, so as a
    // tail expression their temporaries would outlive it. clippy reads this as
    // a redundant `let`, but its suggested rewrite does not compile (E0597).
    let version = [&output.stdout[..], &output.stderr[..]]
        .into_iter()
        .map(String::from_utf8_lossy)
        .find_map(|text| parse_major_version(&text));
    version
}

/// Windows Chrome is a GUI executable: invoking `chrome.exe --version` can
/// open a browser window and keep the caller blocked until Chrome exits. Read
/// the executable's VERSIONINFO resource instead, so capability detection is
/// side-effect free.
#[cfg(target_os = "windows")]
pub fn chrome_version(binary: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FFI_SIGNATURE, VS_FIXEDFILEINFO,
        VerQueryValueW,
    };

    let binary = binary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut ignored = 0;
    // SAFETY: `binary` owns an initialized, NUL-terminated UTF-16 path and is
    // neither moved nor mutated during the call. `ignored` is live, aligned,
    // initialized, and exclusively writable for one `u32`. The API retains no
    // pointer, and the returned byte count is rejected when it reports failure.
    let size = unsafe { GetFileVersionInfoSizeW(binary.as_ptr(), &mut ignored) };
    if size == 0 {
        return None;
    }
    let data_len = usize::try_from(size).ok()?;
    let mut data = Vec::new();
    data.try_reserve_exact(data_len).ok()?;
    data.resize(data_len, 0_u8);
    // SAFETY: `binary` satisfies the path facts above. `data` owns exactly
    // `size` initialized, writable bytes after the checked conversion and
    // allocation; it stays live and stationary for the call, with no aliasing
    // Rust access. The API writes at most the supplied byte count, retains no
    // pointer, and its status is checked before the buffer is queried.
    if unsafe { GetFileVersionInfoW(binary.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
        return None;
    }

    let root = ['\\' as u16, 0];
    let mut fixed = std::ptr::null_mut();
    let mut fixed_len = 0;
    // SAFETY: `data` is the initialized version-information block produced by
    // the successful call above and remains live and immutably borrowed here.
    // `root` is an initialized, NUL-terminated UTF-16 root sub-block name.
    // `fixed` and `fixed_len` are live, aligned, initialized, exclusive output
    // slots. On success the API returns a pointer into `data` plus its extent;
    // neither output is interpreted until status, null, and length checks pass.
    if unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            root.as_ptr(),
            &mut fixed,
            &mut fixed_len,
        )
    } == 0
        || fixed.is_null()
        || fixed_len < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
    {
        return None;
    }

    // SAFETY: successful `VerQueryValueW` for the root block guarantees
    // `fixed` points into the still-live `data` allocation at an initialized
    // `VS_FIXEDFILEINFO`; the checked byte extent covers the complete value.
    // `read_unaligned` imposes no alignment requirement, copies the value
    // without creating a reference, and `data` is not mutated during the read.
    let fixed = unsafe { std::ptr::read_unaligned(fixed.cast::<VS_FIXEDFILEINFO>()) };
    fixed_file_version_major(
        fixed.dwSignature,
        fixed.dwFileVersionMS,
        VS_FFI_SIGNATURE as u32,
    )
}

#[cfg(any(target_os = "windows", test))]
fn fixed_file_version_major(
    signature: u32,
    version_ms: u32,
    expected_signature: u32,
) -> Option<String> {
    (signature == expected_signature).then(|| (version_ms >> 16).to_string())
}

#[cfg(any(not(target_os = "windows"), test))]
fn parse_major_version(text: &str) -> Option<String> {
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    let marker = tokens
        .iter()
        .position(|token| matches!(*token, "Chrome" | "Chromium"))?;
    tokens[marker + 1..].iter().find_map(|token| {
        if !token.contains('.') {
            return None;
        }
        let major = token.split('.').next()?;
        (!major.is_empty()
            && major.bytes().all(|byte| byte.is_ascii_digit())
            && token
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())))
        .then(|| major.to_string())
    })
}

#[cfg(target_os = "macos")]
fn candidates() -> Vec<PathBuf> {
    vec![PathBuf::from(
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    )]
}

#[cfg(target_os = "linux")]
fn candidates() -> Vec<PathBuf> {
    [
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/opt/google/chrome/chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn candidates() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(any(target_os = "windows", test))]
fn known_install_candidates(
    local_app_data: Option<PathBuf>,
    program_files: Option<PathBuf>,
    program_files_x86: Option<PathBuf>,
) -> Vec<PathBuf> {
    let suffix = Path::new("Google").join("Chrome/Application/chrome.exe");
    [local_app_data, program_files, program_files_x86]
        .into_iter()
        .flatten()
        .map(|root| root.join(&suffix))
        .collect()
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RegistryUtf16Buffer {
    unit_len: usize,
    byte_capacity: u32,
}

#[cfg(any(target_os = "windows", test))]
impl RegistryUtf16Buffer {
    const UNIT_BYTES: u32 = std::mem::size_of::<u16>() as u32;

    fn for_required_bytes(required_byte_len: u32) -> Option<Self> {
        if required_byte_len < Self::UNIT_BYTES {
            return None;
        }

        let required_units = required_byte_len
            .checked_add(Self::UNIT_BYTES - 1)?
            .checked_div(Self::UNIT_BYTES)?;
        // RegGetValueW may append a missing string terminator when the caller
        // supplies room for it, so keep one initialized UTF-16 unit in reserve.
        let unit_len_u32 = required_units.checked_add(1)?;
        let byte_capacity = unit_len_u32.checked_mul(Self::UNIT_BYTES)?;
        let unit_len = usize::try_from(unit_len_u32).ok()?;
        Some(Self {
            unit_len,
            byte_capacity,
        })
    }

    fn returned_units(self, returned_byte_len: u32) -> Option<usize> {
        if returned_byte_len < Self::UNIT_BYTES
            || returned_byte_len > self.byte_capacity
            || !returned_byte_len.is_multiple_of(Self::UNIT_BYTES)
        {
            return None;
        }
        usize::try_from(returned_byte_len / Self::UNIT_BYTES).ok()
    }
}

#[cfg(target_os = "windows")]
fn windows_candidates() -> Vec<(PathBuf, ChromeInstallScope)> {
    let mut out = registered_chrome_paths();
    if let Some(root) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        out.extend(
            known_install_candidates(Some(root), None, None)
                .into_iter()
                .map(|path| (path, ChromeInstallScope::CurrentUser)),
        );
    }
    for root in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(root).map(PathBuf::from) {
            out.extend(
                known_install_candidates(None, Some(root), None)
                    .into_iter()
                    .map(|path| (path, ChromeInstallScope::LocalMachine)),
            );
        }
    }
    out.dedup_by(|left, right| left.0 == right.0);
    out
}

#[cfg(target_os = "windows")]
fn registered_chrome_paths() -> Vec<(PathBuf, ChromeInstallScope)> {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6432KEY,
        RRF_SUBKEY_WOW6464KEY, RegGetValueW,
    };

    const APP_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe";
    let subkey = wide_null(APP_PATH);
    let mut out = Vec::new();
    for (root, view) in [
        (HKEY_CURRENT_USER, RRF_SUBKEY_WOW6464KEY),
        (HKEY_CURRENT_USER, RRF_SUBKEY_WOW6432KEY),
        (HKEY_LOCAL_MACHINE, RRF_SUBKEY_WOW6464KEY),
        (HKEY_LOCAL_MACHINE, RRF_SUBKEY_WOW6432KEY),
    ] {
        let flags = RRF_RT_REG_SZ | view;
        let mut required_byte_len = 0u32;
        // SAFETY: `root` is one of the predefined registry handles accepted by
        // RegGetValueW. `subkey` is an initialized, NUL-terminated UTF-16
        // allocation that is not mutated or dropped during the call. A null
        // value name selects the default value, and null type/data pointers are
        // permitted for this size query. `required_byte_len` is live, aligned,
        // and exclusively writable for one `u32` result.
        let status = unsafe {
            RegGetValueW(
                root,
                subkey.as_ptr(),
                std::ptr::null(),
                flags,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut required_byte_len,
            )
        };
        if status != 0 {
            continue;
        }

        let Some(buffer) = RegistryUtf16Buffer::for_required_bytes(required_byte_len) else {
            continue;
        };
        let mut value = Vec::new();
        if value.try_reserve_exact(buffer.unit_len).is_err() {
            continue;
        }
        value.resize(buffer.unit_len, 0u16);
        let mut returned_byte_len = buffer.byte_capacity;
        // SAFETY: the registry handle, flags, and NUL-terminated `subkey`
        // satisfy the same facts as the checked size query above. `value`
        // owns `buffer.unit_len` initialized, aligned `u16` elements and is
        // not moved, borrowed, resized, or dropped during the call. The helper
        // proves `buffer.byte_capacity == value.len() * size_of::<u16>()`, so
        // the byte capacity passed to RegGetValueW equals the initialized
        // writable extent and cannot exceed the allocation. `returned_byte_len`
        // is a live, aligned, exclusive `u32`. The status and returned length
        // are checked before `value` is read.
        let status = unsafe {
            RegGetValueW(
                root,
                subkey.as_ptr(),
                std::ptr::null(),
                flags,
                std::ptr::null_mut(),
                value.as_mut_ptr().cast(),
                &mut returned_byte_len,
            )
        };
        if status != 0 {
            continue;
        }

        let Some(returned_units) = buffer.returned_units(returned_byte_len) else {
            continue;
        };
        let Some(end) = value[..returned_units].iter().position(|unit| *unit == 0) else {
            continue;
        };
        if end > 0 {
            use std::os::windows::ffi::OsStringExt;
            out.push((
                PathBuf::from(std::ffi::OsString::from_wide(&value[..end])),
                if root == HKEY_CURRENT_USER {
                    ChromeInstallScope::CurrentUser
                } else {
                    ChromeInstallScope::LocalMachine
                },
            ));
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_from_either_chrome_or_chromium_text() {
        assert_eq!(
            parse_major_version("Google Chrome 151.0.6779.0").as_deref(),
            Some("151")
        );
        assert_eq!(
            parse_major_version("Chromium 127.0.6533.81").as_deref(),
            Some("127")
        );
        assert_eq!(parse_major_version("Google Chrome unknown"), None);
        assert_eq!(parse_major_version("error 127 while finding Chrome"), None);
    }

    #[test]
    fn parses_windows_fixed_file_version_without_launching_the_executable() {
        const SIGNATURE: u32 = 0xFEEF_04BD;
        assert_eq!(
            fixed_file_version_major(SIGNATURE, (151 << 16) | 2, SIGNATURE).as_deref(),
            Some("151")
        );
        assert_eq!(fixed_file_version_major(0, 151 << 16, SIGNATURE), None);
    }

    #[test]
    fn windows_known_roots_do_not_assume_the_system_drive() {
        let candidates = known_install_candidates(
            Some(PathBuf::from(r"D:\Users\Alice\AppData\Local")),
            Some(PathBuf::from(r"E:\Programs")),
            None,
        );
        assert_eq!(candidates.len(), 2);
        assert!(candidates[0].ends_with("Google/Chrome/Application/chrome.exe"));
        assert!(candidates[1].starts_with(r"E:\Programs"));
    }

    #[test]
    fn registry_utf16_buffer_rounds_bytes_and_reserves_a_terminator() {
        assert_eq!(RegistryUtf16Buffer::for_required_bytes(0), None);
        assert_eq!(RegistryUtf16Buffer::for_required_bytes(1), None);
        assert_eq!(
            RegistryUtf16Buffer::for_required_bytes(2),
            Some(RegistryUtf16Buffer {
                unit_len: 2,
                byte_capacity: 4,
            })
        );
        assert_eq!(
            RegistryUtf16Buffer::for_required_bytes(3),
            Some(RegistryUtf16Buffer {
                unit_len: 3,
                byte_capacity: 6,
            })
        );
        assert_eq!(
            RegistryUtf16Buffer::for_required_bytes(4)
                .unwrap()
                .byte_capacity,
            6
        );
        assert_eq!(RegistryUtf16Buffer::for_required_bytes(u32::MAX), None);
        assert_eq!(RegistryUtf16Buffer::for_required_bytes(u32::MAX - 1), None);
    }

    #[test]
    fn registry_utf16_buffer_validates_second_call_length() {
        let buffer = RegistryUtf16Buffer::for_required_bytes(4).unwrap();
        assert_eq!(buffer.returned_units(0), None);
        assert_eq!(buffer.returned_units(1), None);
        assert_eq!(buffer.returned_units(2), Some(1));
        assert_eq!(buffer.returned_units(3), None);
        assert_eq!(buffer.returned_units(4), Some(2));
        assert_eq!(buffer.returned_units(6), Some(3));
        assert_eq!(buffer.returned_units(8), None);
    }
}
