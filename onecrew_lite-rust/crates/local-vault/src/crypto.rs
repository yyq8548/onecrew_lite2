use std::{
    fs::{self, File},
    io::{Read, Seek, Write},
    path::Path,
};

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng, rand_core::RngCore},
};
use zeroize::Zeroizing;

use crate::{
    VaultError,
    path_security::{AnchoredParentPath, same_file_identity},
};

const NONCE_LEN: usize = 12;

pub(crate) struct VaultCipher(Aes256Gcm);

impl VaultCipher {
    pub(crate) fn load_existing(key_path: &Path) -> Result<Self, VaultError> {
        let file = open_existing_key_file(key_path)?;
        Self::load_existing_file(&file)
    }

    pub(crate) fn load_existing_file(file: &File) -> Result<Self, VaultError> {
        validate_key_file_handle(file)?;
        let mut reader = file
            .try_clone()
            .map_err(|error| VaultError::Keychain(error.to_string()))?;
        reader
            .rewind()
            .map_err(|error| VaultError::Keychain(error.to_string()))?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(33));
        reader
            .take(33)
            .read_to_end(&mut bytes)
            .map_err(|error| VaultError::Keychain(error.to_string()))?;
        if bytes.len() != 32 {
            return Err(VaultError::Keychain("invalid master key length".into()));
        }
        let cipher = Aes256Gcm::new_from_slice(&bytes).map_err(|_| VaultError::Encryption)?;
        Ok(Self(cipher))
    }

    /// Load the AES-256 master key from `key_path`, creating it (32 random
    /// bytes, `0600`) on first run. The key lives next to the vault DB under the
    /// app data dir — no OS keychain for *our* key, so there's no password
    /// prompt for opening the vault. (The only Keychain touch in the app is the
    /// chrome crate reading Chrome's own "Chrome Safe Storage" key to decrypt
    /// extracted cookies; that's separate from this at-rest DB encryption.)
    pub(crate) fn load_or_create(key_path: &Path) -> Result<Self, VaultError> {
        let key = if key_path.exists() {
            let before = open_existing_key_file(key_path)?;
            restrict_permissions(key_path)?;
            let after = open_existing_key_file(key_path)?;
            if !same_file_identity(&before, &after)
                .map_err(|error| VaultError::Keychain(error.to_string()))?
            {
                return Err(VaultError::Keychain(
                    "local credential key identity changed".into(),
                ));
            }
            return Self::load_existing_file(&after);
        } else {
            let mut key = Zeroizing::new(vec![0_u8; 32]);
            OsRng.fill_bytes(&mut key);
            if let Some(parent) = key_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| VaultError::Keychain(error.to_string()))?;
            }
            write_private_key(key_path, &key)?;
            key
        };
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| VaultError::Encryption)?;
        Ok(Self(cipher))
    }

    pub(crate) fn encrypt(&self, plaintext: &Zeroizing<Vec<u8>>) -> Result<Vec<u8>, VaultError> {
        let mut nonce_bytes = [0_u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ciphertext = self
            .0
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_slice())
            .map_err(|_| VaultError::Encryption)?;
        let mut envelope = nonce_bytes.to_vec();
        envelope.extend(ciphertext);
        Ok(envelope)
    }

    pub(crate) fn decrypt(&self, envelope: &[u8]) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        let (nonce, ciphertext) = envelope
            .split_at_checked(NONCE_LEN)
            .ok_or(VaultError::Encryption)?;
        self.0
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map(Zeroizing::new)
            .map_err(|_| VaultError::Encryption)
    }
}

pub(crate) fn open_existing_key_file(path: &Path) -> Result<File, VaultError> {
    let anchored =
        AnchoredParentPath::new(path).map_err(|error| VaultError::Keychain(error.to_string()))?;
    let file = open_existing_key_file_resolved(anchored.resolved())?;
    anchored
        .verify_parent()
        .map_err(|error| VaultError::Keychain(error.to_string()))?;
    Ok(file)
}

#[cfg(unix)]
fn open_existing_key_file_resolved(path: &Path) -> Result<File, VaultError> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| VaultError::Keychain(error.to_string()))
}

#[cfg(target_os = "windows")]
fn open_existing_key_file_resolved(path: &Path) -> Result<File, VaultError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };

    fs::OpenOptions::new()
        .read(true)
        // Open directory handles only so the shared validator can reject them
        // as non-regular keys. Reparse points are still never followed.
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .map_err(|error| VaultError::Keychain(error.to_string()))
}

#[cfg(not(any(unix, target_os = "windows")))]
fn open_existing_key_file_resolved(path: &Path) -> Result<File, VaultError> {
    fs::File::open(path).map_err(|error| VaultError::Keychain(error.to_string()))
}

pub(crate) fn restrict_permissions(path: &Path) -> Result<(), VaultError> {
    let anchored =
        AnchoredParentPath::new(path).map_err(|error| VaultError::Keychain(error.to_string()))?;
    let file = anchored
        .open_final_for_permissions()
        .map_err(|error| VaultError::Keychain(error.to_string()))?;
    if let Err(error) = crate::path_security::set_private_file_permissions(&file) {
        if let Err(cleanup) = anchored.remove_verified_final(&file) {
            log::warn!("key file cleanup after a permission failure failed: {cleanup}");
        }
        return Err(VaultError::Keychain(error.to_string()));
    }
    anchored
        .verify_parent()
        .map_err(|error| VaultError::Keychain(error.to_string()))
}

fn write_private_key(path: &Path, key: &[u8]) -> Result<(), VaultError> {
    let anchored =
        AnchoredParentPath::new(path).map_err(|error| VaultError::Keychain(error.to_string()))?;
    let mut file = anchored
        .create_final_private()
        .map_err(|error| VaultError::Keychain(error.to_string()))?;
    crate::path_security::set_private_file_permissions(&file)
        .map_err(|error| VaultError::Keychain(error.to_string()))?;
    file.write_all(key)
        .and_then(|_| file.sync_all())
        .map_err(|error| VaultError::Keychain(error.to_string()))?;
    anchored
        .sync_parent()
        .map_err(|error| VaultError::Keychain(error.to_string()))
}

fn validate_key_file_handle(file: &File) -> Result<(), VaultError> {
    let metadata = file
        .metadata()
        .map_err(|error| VaultError::Keychain(error.to_string()))?;
    if !metadata.is_file() {
        return Err(VaultError::Keychain(
            "local credential key is not a regular file".into(),
        ));
    }
    if metadata.len() != 32 {
        return Err(VaultError::Keychain("invalid master key length".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(VaultError::Keychain(
                "local credential key permissions are too broad".into(),
            ));
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(VaultError::Keychain(
                "local credential key is a reparse point".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use std::{os::windows::ffi::OsStrExt, ptr};

    use tempfile::tempdir;
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetFileSecurityW, GetSecurityDescriptorControl,
        PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
    };

    use super::VaultCipher;

    #[test]
    fn newly_created_key_has_a_protected_dacl() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.key");
        VaultCipher::load_or_create(&path).unwrap();

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
