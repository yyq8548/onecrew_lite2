use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
};

#[cfg(unix)]
use std::ffi::OsString;

/// A logical path resolved through an identity-anchored canonical parent.
/// Only ancestor aliases are followed; the final component is appended after
/// canonicalization and must be opened separately with no-follow semantics.
pub(crate) struct AnchoredParentPath {
    resolved: PathBuf,
    canonical_parent: PathBuf,
    parent_identity: DirectoryIdentity,
    _parent_file: File,
    #[cfg(unix)]
    file_name: OsString,
}

impl AnchoredParentPath {
    pub(crate) fn new(logical: &Path) -> io::Result<Self> {
        Self::new_with_resolution_hook(logical, || {})
    }

    fn new_with_resolution_hook(
        logical: &Path,
        after_logical_parent_open: impl FnOnce(),
    ) -> io::Result<Self> {
        let parent = logical
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
        let file_name = logical.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "path has no final component")
        })?;
        if !matches!(
            Path::new(file_name)
                .components()
                .collect::<Vec<_>>()
                .as_slice(),
            [Component::Normal(_)]
        ) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path has an unsafe final component",
            ));
        }
        let logical_parent_file = open_directory_following_aliases(parent)?;
        let logical_parent_identity = directory_identity(&logical_parent_file)?;
        after_logical_parent_open();
        let canonical_parent = fs::canonicalize(parent)?;
        let parent_file = open_directory_nofollow(&canonical_parent)?;
        let parent_identity = directory_identity(&parent_file)?;
        if logical_parent_identity != parent_identity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "parent directory identity changed during resolution",
            ));
        }
        Ok(Self {
            resolved: canonical_parent.join(file_name),
            canonical_parent,
            parent_identity,
            _parent_file: parent_file,
            #[cfg(unix)]
            file_name: file_name.to_owned(),
        })
    }

    #[cfg(all(test, unix))]
    pub(crate) fn new_with_test_hook(
        logical: &Path,
        after_logical_parent_open: impl FnOnce(),
    ) -> io::Result<Self> {
        Self::new_with_resolution_hook(logical, after_logical_parent_open)
    }

    pub(crate) fn resolved(&self) -> &Path {
        &self.resolved
    }

    pub(crate) fn verify_parent(&self) -> io::Result<()> {
        let current = open_directory_nofollow(&self.canonical_parent)?;
        if directory_identity(&current)? == self.parent_identity {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "parent directory identity changed",
            ))
        }
    }

    pub(crate) fn open_final_nofollow(&self, write: bool) -> io::Result<File> {
        let file = openat_final(self, write)?;
        self.verify_parent()?;
        Ok(file)
    }

    pub(crate) fn create_final_private(&self) -> io::Result<File> {
        let file = createat_final_private(self)?;
        self.verify_parent()?;
        Ok(file)
    }

    pub(crate) fn open_final_for_delete(&self) -> io::Result<File> {
        let file = openat_final_for_delete(self)?;
        self.verify_parent()?;
        Ok(file)
    }

    pub(crate) fn open_final_for_permissions(&self) -> io::Result<File> {
        let file = openat_final_for_permissions(self)?;
        self.verify_parent()?;
        Ok(file)
    }

    pub(crate) fn remove_verified_final(&self, expected: &File) -> io::Result<()> {
        let current = self.open_final_for_delete()?;
        if !same_file_identity(&current, expected)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "final component identity changed",
            ));
        }
        remove_open_final(self, &current)?;
        self.verify_parent()
    }

    #[cfg(unix)]
    pub(crate) fn unlink_final(&self) -> io::Result<()> {
        use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};

        let name = std::ffi::CString::new(self.file_name.as_bytes())?;
        // SAFETY: `name` is a NUL-free single component and parent_file is a
        // retained live directory fd. No pointer or fd is retained.
        let unlinked = unsafe { libc::unlinkat(self._parent_file.as_raw_fd(), name.as_ptr(), 0) };
        if unlinked == 0 {
            self.verify_parent()
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(unix)]
    pub(crate) fn sync_parent(&self) -> io::Result<()> {
        self._parent_file.sync_all()?;
        self.verify_parent()
    }

    #[cfg(not(unix))]
    pub(crate) fn sync_parent(&self) -> io::Result<()> {
        self.verify_parent()
    }
}

#[cfg(not(windows))]
fn open_directory_following_aliases(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(windows)]
fn open_directory_following_aliases(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

pub(crate) fn same_file_identity(left: &File, right: &File) -> io::Result<bool> {
    Ok(object_identity(left)? == object_identity(right)?)
}

#[cfg(unix)]
pub(crate) fn set_private_file_permissions(file: &File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(windows)]
pub(crate) fn set_private_file_permissions(file: &File) -> io::Result<()> {
    use std::{ffi::c_void, os::windows::io::AsRawHandle, ptr::NonNull};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            ACL,
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
                SDDL_REVISION_1, SE_FILE_OBJECT, SetSecurityInfo,
            },
            DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
            PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
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
            // SAFETY: the only constructors below accept non-null descriptors
            // allocated by Windows APIs that require LocalFree.
            let _ = unsafe { LocalFree(self.0.as_ptr()) };
        }
    }

    const PRIVATE_FILE_SDDL: &str = "D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)";
    let sddl = PRIVATE_FILE_SDDL
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut raw_descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `sddl` is initialized and NUL-terminated; the output pointer is
    // live and exclusively writable, and status/pointer are checked.
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut raw_descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(io::Error::last_os_error());
    }
    let descriptor = NonNull::new(raw_descriptor)
        .map(LocalDescriptor)
        .ok_or_else(|| io::Error::other("null security descriptor"))?;
    let mut dacl_present = 0;
    let mut dacl_defaulted = 0;
    let mut dacl: *mut ACL = std::ptr::null_mut();
    // SAFETY: descriptor is a live self-relative security descriptor and all
    // output pointers are initialized, aligned, and exclusively writable.
    let got_dacl = unsafe {
        GetSecurityDescriptorDacl(
            descriptor.as_ptr(),
            &mut dacl_present,
            &mut dacl,
            &mut dacl_defaulted,
        )
    };
    if got_dacl == 0 || dacl_present == 0 || dacl.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: file is a live handle opened with WRITE_DAC; `dacl` remains
    // owned by descriptor through the call and no pointer is retained.
    let status = unsafe {
        SetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }

    let mut readback_raw: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: all optional outputs are null; readback_raw is a live exclusive
    // output. A successful allocation is checked and owned immediately.
    let readback_status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut readback_raw,
        )
    };
    if readback_status != 0 {
        return Err(io::Error::from_raw_os_error(readback_status as i32));
    }
    let readback = NonNull::new(readback_raw)
        .map(LocalDescriptor)
        .ok_or_else(|| io::Error::other("null security readback"))?;
    let mut control = 0_u16;
    let mut revision = 0_u32;
    // SAFETY: readback is a live descriptor and outputs are initialized,
    // aligned, exclusive writable storage. Status is checked.
    let got_control =
        unsafe { GetSecurityDescriptorControl(readback.as_ptr(), &mut control, &mut revision) };
    if got_control == 0 || control & SE_DACL_PROTECTED == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn set_private_file_permissions(_file: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn openat_final(anchor: &AnchoredParentPath, write: bool) -> io::Result<File> {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    };

    let name = std::ffi::CString::new(anchor.file_name.as_bytes())?;
    let access = if write { libc::O_RDWR } else { libc::O_RDONLY };
    // SAFETY: `name` is a NUL-free single component, the retained parent fd is
    // live, and the returned fd is checked before ownership transfers to File.
    let descriptor = unsafe {
        libc::openat(
            anchor._parent_file.as_raw_fd(),
            name.as_ptr(),
            access | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: successful openat returned a fresh owned descriptor.
        Ok(unsafe { File::from_raw_fd(descriptor) })
    }
}

#[cfg(windows)]
fn openat_final(anchor: &AnchoredParentPath, write: bool) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

    OpenOptions::new()
        .read(true)
        .write(write)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(anchor.resolved())
}

#[cfg(not(any(unix, windows)))]
fn openat_final(anchor: &AnchoredParentPath, write: bool) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(write)
        .open(anchor.resolved())
}

#[cfg(unix)]
fn createat_final_private(anchor: &AnchoredParentPath) -> io::Result<File> {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    };

    let name = std::ffi::CString::new(anchor.file_name.as_bytes())?;
    // SAFETY: same single-component/live-dirfd reasoning as openat_final. A
    // successful O_EXCL open returns a fresh descriptor owned by File.
    let descriptor = unsafe {
        libc::openat(
            anchor._parent_file.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
    };
    if descriptor < 0 {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: successful openat returned a fresh owned descriptor.
        Ok(unsafe { File::from_raw_fd(descriptor) })
    }
}

#[cfg(windows)]
fn createat_final_private(anchor: &AnchoredParentPath) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, WRITE_DAC,
    };
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        // SQLite does not share DELETE access. Retain this identity handle
        // without DELETE; verified removal opens a separate deletion handle.
        .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(anchor.resolved())
}

#[cfg(not(any(unix, windows)))]
fn createat_final_private(anchor: &AnchoredParentPath) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(anchor.resolved())
}

#[cfg(unix)]
fn openat_final_for_delete(anchor: &AnchoredParentPath) -> io::Result<File> {
    openat_final(anchor, false)
}

#[cfg(windows)]
fn openat_final_for_delete(anchor: &AnchoredParentPath) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    OpenOptions::new()
        .read(true)
        .access_mode(FILE_GENERIC_READ | DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(anchor.resolved())
}

#[cfg(not(any(unix, windows)))]
fn openat_final_for_delete(anchor: &AnchoredParentPath) -> io::Result<File> {
    openat_final(anchor, false)
}

#[cfg(unix)]
fn openat_final_for_permissions(anchor: &AnchoredParentPath) -> io::Result<File> {
    openat_final(anchor, false)
}

#[cfg(unix)]
fn remove_open_final(anchor: &AnchoredParentPath, _file: &File) -> io::Result<()> {
    anchor.unlink_final()
}

#[cfg(windows)]
fn remove_open_final(_anchor: &AnchoredParentPath, file: &File) -> io::Result<()> {
    use std::{mem::size_of, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO_EX, FileDispositionInfoEx,
        SetFileInformationByHandle,
    };
    let disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    };
    // SAFETY: `file` is a live handle opened with DELETE access; disposition
    // is initialized exact-size input and is not retained.
    let deleted = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfoEx,
            std::ptr::from_ref(&disposition).cast(),
            u32::try_from(size_of::<FILE_DISPOSITION_INFO_EX>())
                .expect("FILE_DISPOSITION_INFO_EX fits in u32"),
        )
    };
    if deleted == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn remove_open_final(anchor: &AnchoredParentPath, _file: &File) -> io::Result<()> {
    fs::remove_file(anchor.resolved())
}

#[cfg(windows)]
fn openat_final_for_permissions(anchor: &AnchoredParentPath) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, WRITE_DAC,
    };
    OpenOptions::new()
        .read(true)
        .access_mode(FILE_GENERIC_READ | WRITE_DAC)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(anchor.resolved())
}

#[cfg(not(any(unix, windows)))]
fn openat_final_for_permissions(anchor: &AnchoredParentPath) -> io::Result<File> {
    openat_final(anchor, false)
}

#[cfg(unix)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct DirectoryIdentity {
    device: u64,
    inode: u64,
}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct DirectoryIdentity {
    volume: u64,
    file_id: [u8; 16],
}

#[cfg(not(any(unix, windows)))]
#[derive(Clone, Copy, PartialEq, Eq)]
struct DirectoryIdentity;

#[cfg(unix)]
fn open_directory_nofollow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
fn open_directory_nofollow(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(any(unix, windows)))]
fn open_directory_nofollow(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(unix)]
fn directory_identity(file: &File) -> io::Result<DirectoryIdentity> {
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "parent is not a directory",
        ));
    }
    object_identity(file)
}

#[cfg(unix)]
fn object_identity(file: &File) -> io::Result<DirectoryIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(DirectoryIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
fn directory_identity(file: &File) -> io::Result<DirectoryIdentity> {
    if !file.metadata()?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "parent is not a directory",
        ));
    }
    object_identity(file)
}

#[cfg(windows)]
fn object_identity(file: &File) -> io::Result<DirectoryIdentity> {
    use std::{mem::size_of, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };
    let mut info = FILE_ID_INFO::default();
    // SAFETY: the file owns a live directory handle. `info` is initialized,
    // aligned, exact-size writable storage and is not retained by the API.
    let loaded = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            std::ptr::from_mut(&mut info).cast(),
            u32::try_from(size_of::<FILE_ID_INFO>()).expect("FILE_ID_INFO fits in u32"),
        )
    };
    if loaded == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(DirectoryIdentity {
            volume: info.VolumeSerialNumber,
            file_id: info.FileId.Identifier,
        })
    }
}

#[cfg(not(any(unix, windows)))]
fn directory_identity(file: &File) -> io::Result<DirectoryIdentity> {
    if file.metadata()?.is_dir() {
        object_identity(file)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "parent is not a directory",
        ))
    }
}

#[cfg(not(any(unix, windows)))]
fn object_identity(_file: &File) -> io::Result<DirectoryIdentity> {
    Ok(DirectoryIdentity)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::AnchoredParentPath;

    #[test]
    fn parent_alias_swap_between_open_and_canonicalize_is_rejected() {
        let root = tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        let alias = root.path().join("alias");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        symlink(&first, &alias).unwrap();
        let target = alias.join("vault.db");

        let result = AnchoredParentPath::new_with_test_hook(&target, || {
            std::fs::remove_file(&alias).unwrap();
            symlink(&second, &alias).unwrap();
        });

        assert!(result.is_err());
    }
}
