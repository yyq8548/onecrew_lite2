use std::{
    ffi::c_void,
    fs::File,
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
    ptr::{NonNull, null_mut},
};
use windows_sys::Win32::{
    Foundation::{INVALID_HANDLE_VALUE, LocalFree},
    Security::{
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
            SE_FILE_OBJECT,
        },
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, PSECURITY_DESCRIPTOR,
        SE_DACL_PRESENT, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::{
        CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    },
};

struct LocalDescriptor(NonNull<c_void>);

impl Drop for LocalDescriptor {
    fn drop(&mut self) {
        // SAFETY: both constructors below check successful Windows allocations
        // requiring LocalFree. This guard is their sole owner, freed exactly once.
        let _ = unsafe { LocalFree(self.0.as_ptr()) };
    }
}

/// Create an empty recovery point with its DACL supplied atomically, not inherited
/// and repaired after creation. The returned handle prevents final-file replacement.
pub(super) fn create_private_backup_file(path: &Path) -> io::Result<File> {
    let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide_path.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "backup path contains NUL",
        ));
    }
    wide_path.push(0);
    let sddl: Vec<u16> = "D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)"
        .encode_utf16()
        .chain([0])
        .collect();
    let mut raw_descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: sddl is initialized, NUL-terminated and live throughout the call.
    // The initialized exclusive output is checked before taking allocation ownership.
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut raw_descriptor,
            null_mut(),
        )
    };
    if converted == 0 {
        return Err(io::Error::last_os_error());
    }
    let descriptor = NonNull::new(raw_descriptor)
        .map(LocalDescriptor)
        .ok_or_else(|| io::Error::other("null backup security descriptor"))?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>())
            .expect("SECURITY_ATTRIBUTES fits in u32"),
        lpSecurityDescriptor: descriptor.0.as_ptr(),
        bInheritHandle: 0,
    };
    // SAFETY: path and descriptor are live, initialized and correctly terminated;
    // attributes has the ABI size. CREATE_NEW cannot overwrite an existing target.
    // No pointers are retained. The failure sentinel is checked before owning handle.
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful CreateFileW returned a fresh owned, valid file handle;
    // File assumes unique ownership and closes it on every subsequent exit.
    let file = unsafe { File::from_raw_handle(handle) };
    if let Err(error) = verify_protected_dacl(&file) {
        drop(file);
        // No database bytes have been written; never clean up a pre-existing file.
        if let Err(cleanup_error) = std::fs::remove_file(path) {
            return Err(io::Error::other(format!(
                "backup permission verification failed: {error}; empty backup cleanup failed: {cleanup_error}"
            )));
        }
        return Err(error);
    }
    Ok(file)
}

fn verify_protected_dacl(file: &File) -> io::Result<()> {
    let mut raw_descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let mut dacl = null_mut();
    // SAFETY: file owns a live handle with READ_CONTROL. Optional outputs are
    // null except the exclusive DACL/descriptor outputs; both are checked. The
    // DACL points into the returned descriptor and is not retained past its owner.
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut raw_descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let descriptor = NonNull::new(raw_descriptor)
        .map(LocalDescriptor)
        .ok_or_else(|| io::Error::other("null backup security readback"))?;
    let mut control = 0_u16;
    let mut revision = 0_u32;
    // SAFETY: descriptor owns a valid live security descriptor. Both output
    // values are initialized, aligned, exclusive writable storage; none escapes.
    let loaded =
        unsafe { GetSecurityDescriptorControl(descriptor.0.as_ptr(), &mut control, &mut revision) };
    if loaded == 0 {
        return Err(io::Error::last_os_error());
    }
    if dacl.is_null()
        || control & (SE_DACL_PRESENT | SE_DACL_PROTECTED) != (SE_DACL_PRESENT | SE_DACL_PROTECTED)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "backup DACL is not protected",
        ));
    }
    Ok(())
}
