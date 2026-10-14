//! Cross-platform creation of the staged MCP connection file.
//!
//! Secret bytes are written only after the file has its final private access
//! boundary: mode `0600` at Unix create time, or a protected Windows DACL on
//! the still-empty file. Any protection or write failure removes the file.

use std::io::Write as _;
use std::path::Path;

#[cfg(unix)]
pub(super) fn write_private_file(path: &Path, contents: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt as _;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| error.to_string())?;
    let result = file.write_all(contents).and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = result {
        return Err(error_with_file_cleanup(path, error.to_string()));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(super) fn write_private_file(path: &Path, contents: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    if let Err(error) = restrict_private_file(path) {
        drop(file);
        return Err(error_with_file_cleanup(path, error));
    }
    let result = file.write_all(contents).and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = result {
        return Err(error_with_file_cleanup(path, error.to_string()));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn restrict_private_file(path: &Path) -> Result<(), String> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt as _, ptr::NonNull};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
            SetFileSecurityW,
        },
    };

    struct LocalDescriptor(NonNull<c_void>);

    impl LocalDescriptor {
        fn as_ptr(&self) -> PSECURITY_DESCRIPTOR {
            self.0.as_ptr()
        }
    }

    impl Drop for LocalDescriptor {
        fn drop(&mut self) {
            // SAFETY: this guard is constructed only from the non-null
            // self-relative descriptor allocated by the successful conversion
            // below. Microsoft requires that allocation be released with
            // `LocalFree`; the guard is its sole cleanup owner and Drop runs
            // once after every native/Rust use on success, error, or unwinding.
            // A valid LocalAlloc handle makes `LocalFree` return null.
            let _ = unsafe { LocalFree(self.0.as_ptr()) };
        }
    }

    const PRIVATE_FILE_SDDL: &str = "D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)";
    let path_wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let sddl_wide = PRIVATE_FILE_SDDL
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut raw_descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `sddl_wide` owns an initialized, NUL-terminated UTF-16 SDDL
    // string and remains live and immutable for the call. `raw_descriptor` is
    // a live, aligned, initialized, exclusively writable pointer output. The
    // null optional size output is permitted. Status and pointer are checked
    // before the returned allocation gains an owner or is interpreted.
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl_wide.as_ptr(),
            SDDL_REVISION_1,
            &mut raw_descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let descriptor = NonNull::new(raw_descriptor)
        .map(LocalDescriptor)
        .ok_or_else(|| "security descriptor conversion returned a null descriptor".to_string())?;
    // SAFETY: `path_wide` owns an initialized, NUL-terminated UTF-16 path and
    // remains live and immutable for the call. `descriptor` solely owns the
    // live self-relative descriptor returned above; its stable pointer is
    // readable for the call and is not retained. The flags apply its DACL and
    // protected state. The status is checked before success is reported.
    let applied = unsafe {
        SetFileSecurityW(
            path_wide.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor.as_ptr(),
        )
    };
    if applied == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(not(any(unix, target_os = "windows")))]
pub(super) fn write_private_file(_path: &Path, _contents: &[u8]) -> Result<(), String> {
    Err("private MCP connection files are unsupported on this platform".into())
}

#[cfg(any(unix, target_os = "windows"))]
fn error_with_file_cleanup(path: &Path, primary: String) -> String {
    match std::fs::remove_file(path) {
        Ok(()) => primary,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => primary,
        Err(error) => format!("{primary}; private file cleanup failed: {error}"),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::{error_with_file_cleanup, write_private_file};

    #[test]
    fn unix_connection_file_is_private_and_never_overwritten() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join(".mcp.json");
        write_private_file(&path, b"secret-one").expect("private file");
        assert_eq!(
            std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(write_private_file(&path, b"secret-two").is_err());
        assert_eq!(std::fs::read(&path).expect("original bytes"), b"secret-one");
    }

    #[test]
    fn cleanup_failure_is_never_hidden() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("cannot-remove-as-file");
        std::fs::create_dir(&path).expect("directory");
        let error = error_with_file_cleanup(&path, "injected write failure".into());
        assert!(error.contains("injected write failure"));
        assert!(error.contains("private file cleanup failed"));
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use std::{os::windows::ffi::OsStrExt as _, ptr};

    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetFileSecurityW, GetSecurityDescriptorControl,
        PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
    };

    use super::write_private_file;

    #[test]
    fn windows_connection_file_has_a_protected_dacl() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join(".mcp.json");
        write_private_file(&path, b"secret").expect("private file");
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut needed = 0_u32;
        // SAFETY: `wide` is an initialized, NUL-terminated UTF-16 path and
        // remains live for the call. A null descriptor with zero capacity is
        // the documented size query. `needed` is live, aligned, initialized,
        // and exclusively writable; the return and length are checked.
        let sized = unsafe {
            GetFileSecurityW(
                wide.as_ptr(),
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        // SAFETY: `GetLastError` has no pointer or lifetime obligations and is
        // called immediately after the zero size-query return, before another
        // native call can overwrite its documented failure classification.
        let size_error = unsafe { GetLastError() };
        assert_eq!(sized, 0);
        assert_eq!(size_error, ERROR_INSUFFICIENT_BUFFER);
        assert!(needed > 0);
        let word_bytes = std::mem::size_of::<usize>();
        let needed_bytes = usize::try_from(needed).expect("descriptor length must fit usize");
        let word_len = needed_bytes.div_ceil(word_bytes);
        let mut descriptor = vec![0_usize; word_len];
        let capacity_bytes = descriptor
            .len()
            .checked_mul(word_bytes)
            .and_then(|bytes| u32::try_from(bytes).ok())
            .expect("rounded descriptor capacity must fit u32");
        let mut returned_bytes = capacity_bytes;
        // SAFETY: `wide` satisfies the path facts above. `descriptor` owns
        // `capacity_bytes` initialized, writable bytes with `usize` alignment,
        // remains live and stationary, and has no aliasing Rust access. The
        // output length slot is exclusive. Status and the returned extent are
        // checked before the descriptor is passed to another native API.
        let read = unsafe {
            GetFileSecurityW(
                wide.as_ptr(),
                DACL_SECURITY_INFORMATION,
                descriptor.as_mut_ptr().cast::<core::ffi::c_void>() as PSECURITY_DESCRIPTOR,
                capacity_bytes,
                &mut returned_bytes,
            )
        };
        assert_ne!(read, 0);
        assert!(returned_bytes > 0);
        assert!(returned_bytes <= capacity_bytes);
        let mut control = 0_u16;
        let mut revision = 0_u32;
        // SAFETY: successful `GetFileSecurityW` initialized a complete
        // self-relative descriptor within the checked returned extent. The
        // aligned backing allocation remains live and unmodified. `control`
        // and `revision` are live, aligned, initialized, exclusive outputs;
        // the status is checked before `control` is interpreted.
        let controlled = unsafe {
            GetSecurityDescriptorControl(
                descriptor.as_mut_ptr().cast::<core::ffi::c_void>(),
                &mut control,
                &mut revision,
            )
        };
        assert_ne!(controlled, 0);
        assert_ne!(control & SE_DACL_PROTECTED, 0);
    }
}
