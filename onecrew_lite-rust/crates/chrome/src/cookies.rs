//! Read an app-owned Chrome profile's cookie store and decrypt its values.
//!
//! macOS Chrome uses the Safe Storage v10 AES-128-CBC format. Windows Chrome
//! launched with our non-default `--user-data-dir` does not enable App-Bound
//! encryption; it stores a DPAPI-wrapped AES-256 key in that directory's
//! `Local State` and writes v10 AES-256-GCM cookie values. We deliberately do
//! not import a user's default Chrome profile. A v20 value is App-Bound and is
//! rejected rather than bypassed or silently skipped.

use std::path::{Path, PathBuf};

use onecrew_local_domain::BrowserCookie;
use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

use crate::ChromeError;

#[cfg(not(target_os = "windows"))]
const SALT: &[u8] = b"saltysalt";
#[cfg(not(target_os = "windows"))]
const MACOS_KEY_LEN: usize = 16;
#[cfg(any(target_os = "windows", test))]
const WINDOWS_KEY_LEN: usize = 32;
const V10: &[u8] = b"v10";
#[cfg(target_os = "windows")]
const V20: &[u8] = b"v20";
const SHA256_PREFIX_LEN: usize = 32;
#[cfg(any(target_os = "windows", test))]
const GCM_NONCE_LEN: usize = 12;
#[cfg(any(target_os = "windows", test))]
const GCM_TAG_LEN: usize = 16;

/// Extract cookies whose host ends with one of `host_suffixes`; pass an empty
/// slice for all cookies. Chrome must have exited first so the DB is flushed.
pub(crate) fn read_cookies(
    profile_dir: &Path,
    host_suffixes: &[&str],
) -> Result<Vec<BrowserCookie>, ChromeError> {
    let db = cookie_store_path(profile_dir)?;
    let conn = open_read_only(&db)?;

    // Schema v24+ prepends SHA256(host_key) to encrypted plaintext. Chromium
    // verifies this digest before accepting a cookie; we must do the same.
    let schema_version: i64 = conn
        .query_row("SELECT value FROM meta WHERE key='version'", [], |row| {
            row.get::<_, String>(0)
        })
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let verify_host_prefix = schema_version >= 24;

    #[cfg(target_os = "windows")]
    let key = windows_profile_key(profile_dir)?;
    #[cfg(not(target_os = "windows"))]
    let key = safe_storage_key()?;

    let mut stmt = conn.prepare(
        "SELECT host_key, name, value, encrypted_value, path, is_secure, is_httponly FROM cookies",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(RawCookie {
            host_key: row.get(0)?,
            name: row.get(1)?,
            value: row.get(2)?,
            encrypted_value: row.get(3)?,
            path: row.get(4)?,
            secure: row.get::<_, i64>(5)? != 0,
            http_only: row.get::<_, i64>(6)? != 0,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        let raw = row?;
        if !host_suffixes.is_empty()
            && !host_suffixes
                .iter()
                .any(|suffix| host_matches_suffix(&raw.host_key, suffix))
        {
            continue;
        }
        let value = if !raw.encrypted_value.is_empty() {
            let Some(plaintext) = decrypt_value(&key, &raw.encrypted_value)? else {
                // Preserve the historical macOS/Linux behavior for unknown
                // legacy rows. Windows unknown/v20 formats fail closed below.
                continue;
            };
            verify_and_strip_host_prefix(plaintext, &raw.host_key, verify_host_prefix)?
        } else {
            raw.value
        };
        if value.is_empty() {
            continue;
        }
        out.push(BrowserCookie {
            name: raw.name,
            value,
            domain: raw.host_key,
            path: raw.path,
            secure: raw.secure,
            http_only: raw.http_only,
        });
    }
    Ok(out)
}

fn host_matches_suffix(host_key: &str, suffix: &str) -> bool {
    let host = host_key.trim_start_matches('.');
    let suffix = suffix.trim_start_matches('.');
    if suffix.is_empty() {
        return false;
    }
    host == suffix
        || host
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn cookie_store_path(profile_dir: &Path) -> Result<PathBuf, ChromeError> {
    let modern = profile_dir.join("Default").join("Network").join("Cookies");
    if modern.is_file() {
        return Ok(modern);
    }
    let legacy = profile_dir.join("Default").join("Cookies");
    if legacy.is_file() {
        return Ok(legacy);
    }
    Err(ChromeError::CookieStoreNotFound(profile_dir.to_path_buf()))
}

fn open_read_only(path: &Path) -> Result<Connection, ChromeError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| match &error {
        rusqlite::Error::SqliteFailure(detail, _)
            if matches!(
                detail.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            ChromeError::CookieStoreLocked(path.to_path_buf())
        }
        _ => ChromeError::Sqlite(error),
    })
}

struct RawCookie {
    host_key: String,
    name: String,
    value: String,
    encrypted_value: Vec<u8>,
    path: String,
    secure: bool,
    http_only: bool,
}

fn verify_and_strip_host_prefix(
    plaintext: Vec<u8>,
    host_key: &str,
    verify: bool,
) -> Result<String, ChromeError> {
    let value = if verify {
        let expected = Sha256::digest(host_key.as_bytes());
        let Some((actual, value)) = plaintext.split_at_checked(SHA256_PREFIX_LEN) else {
            return Err(ChromeError::CookieHostIntegrityFailed(host_key.into()));
        };
        if actual != expected.as_slice() {
            return Err(ChromeError::CookieHostIntegrityFailed(host_key.into()));
        }
        value
    } else {
        &plaintext
    };
    String::from_utf8(value.to_vec()).map_err(|_| ChromeError::CookieDecryptionFailed)
}

#[cfg(target_os = "windows")]
fn decrypt_value(key: &[u8], encrypted: &[u8]) -> Result<Option<Vec<u8>>, ChromeError> {
    if encrypted.starts_with(V20) {
        return Err(ChromeError::AppBoundCookieUnsupported);
    }
    if !encrypted.starts_with(V10) {
        return Err(ChromeError::CookieDecryptionFailed);
    }
    decrypt_windows_v10(key, encrypted).map(Some)
}

#[cfg(not(target_os = "windows"))]
fn decrypt_value(key: &[u8], encrypted: &[u8]) -> Result<Option<Vec<u8>>, ChromeError> {
    let Some(body) = encrypted.strip_prefix(V10) else {
        return Ok(None);
    };
    let iv = [0x20u8; 16];
    let mut buf = body.to_vec();
    Ok(decrypt_aes_128_cbc(key, &iv, &mut buf))
}

#[cfg(any(target_os = "windows", test))]
fn decrypt_windows_v10(key: &[u8], encrypted: &[u8]) -> Result<Vec<u8>, ChromeError> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};

    if key.len() != WINDOWS_KEY_LEN {
        return Err(ChromeError::CookieKeyInvalid(format!(
            "expected a {WINDOWS_KEY_LEN}-byte key"
        )));
    }
    let body = encrypted
        .strip_prefix(V10)
        .ok_or(ChromeError::CookieDecryptionFailed)?;
    if body.len() < GCM_NONCE_LEN + GCM_TAG_LEN {
        return Err(ChromeError::CookieDecryptionFailed);
    }
    let (nonce, ciphertext_and_tag) = body.split_at(GCM_NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| ChromeError::CookieDecryptionFailed)?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext_and_tag)
        .map_err(|_| ChromeError::CookieDecryptionFailed)
}

#[cfg(not(target_os = "windows"))]
fn decrypt_aes_128_cbc(key: &[u8], iv: &[u8; 16], data: &mut [u8]) -> Option<Vec<u8>> {
    use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
    let decryptor = Aes128CbcDec::new_from_slices(key, iv).ok()?;
    decryptor
        .decrypt_padded_mut::<Pkcs7>(data)
        .ok()
        .map(<[u8]>::to_vec)
}

#[cfg(not(target_os = "windows"))]
fn safe_storage_key() -> Result<Vec<u8>, ChromeError> {
    let (password, iterations) = os_safe_storage_password()?;
    let mut key = vec![0u8; MACOS_KEY_LEN];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(&password, SALT, iterations, &mut key);
    Ok(key)
}

#[cfg(target_os = "macos")]
fn os_safe_storage_password() -> Result<(Vec<u8>, u32), ChromeError> {
    let password =
        security_framework::passwords::get_generic_password("Chrome Safe Storage", "Chrome")
            .map_err(|error| ChromeError::Keychain(error.to_string()))?;
    Ok((password, 1003))
}

#[cfg(target_os = "linux")]
fn os_safe_storage_password() -> Result<(Vec<u8>, u32), ChromeError> {
    Ok((b"peanuts".to_vec(), 1))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn os_safe_storage_password() -> Result<(Vec<u8>, u32), ChromeError> {
    Err(ChromeError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn windows_profile_key(profile_dir: &Path) -> Result<Vec<u8>, ChromeError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let local_state = profile_dir.join("Local State");
    if !local_state.is_file() {
        return Err(ChromeError::LocalStateNotFound(local_state));
    }
    let document = std::fs::read_to_string(&local_state)?;
    let value: serde_json::Value = serde_json::from_str(&document)
        .map_err(|error| ChromeError::CookieKeyInvalid(error.to_string()))?;
    let encoded = value
        .pointer("/os_crypt/encrypted_key")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ChromeError::CookieKeyInvalid("missing os_crypt.encrypted_key".into()))?;
    let wrapped = STANDARD
        .decode(encoded)
        .map_err(|error| ChromeError::CookieKeyInvalid(error.to_string()))?;
    let ciphertext = wrapped
        .strip_prefix(b"DPAPI")
        .ok_or_else(|| ChromeError::CookieKeyInvalid("encrypted key has no DPAPI header".into()))?;
    let key = dpapi_unprotect(ciphertext)?;
    if key.len() != WINDOWS_KEY_LEN {
        return Err(ChromeError::CookieKeyInvalid(format!(
            "DPAPI returned {} bytes, expected {WINDOWS_KEY_LEN}",
            key.len()
        )));
    }
    Ok(key)
}

#[cfg(target_os = "windows")]
fn dpapi_unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, ChromeError> {
    use std::ptr::NonNull;
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
    };

    struct LocalBuffer(NonNull<u8>);

    impl LocalBuffer {
        fn from_dpapi(pointer: *mut u8) -> Option<Self> {
            NonNull::new(pointer).map(Self)
        }

        fn as_ptr(&self) -> *const u8 {
            self.0.as_ptr()
        }
    }

    impl Drop for LocalBuffer {
        fn drop(&mut self) {
            // SAFETY: `from_dpapi` is called only with the non-null `pbData`
            // returned by a successful `CryptUnprotectData`, whose contract
            // transfers one LocalAlloc-family allocation to the caller and
            // requires `LocalFree`. This guard is the sole cleanup owner and
            // Drop runs once, after every derived slice borrow has ended, on
            // success, validation error, or unwinding. A valid LocalAlloc
            // handle makes `LocalFree` return null; no later use is possible.
            let _ = unsafe { LocalFree(self.0.as_ptr().cast()) };
        }
    }

    let input = CRYPT_INTEGER_BLOB {
        cbData: ciphertext
            .len()
            .try_into()
            .map_err(|_| ChromeError::CookieKeyInvalid("DPAPI input is too large".into()))?,
        pbData: ciphertext.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` is live and initialized; its checked `cbData` exactly
    // describes the initialized `ciphertext` bytes. Although the C blob stores
    // a mutable pointer, DPAPI documents `pDataIn` as input-only, and the shared
    // Rust slice remains live and unmodified for the call. `output` is a live,
    // aligned, initialized, exclusively writable output blob. All optional
    // pointers are null as permitted, UI is forbidden, no pointer is retained,
    // and status plus output pointer/length are checked before interpretation.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        // SAFETY: `GetLastError` has no pointer or lifetime obligations and is
        // called immediately after `CryptUnprotectData` reported failure,
        // before another native call can overwrite this thread's error value.
        let error = unsafe { GetLastError() };
        return Err(ChromeError::Dpapi(format!("Windows error {error}")));
    }
    let Some(output_buffer) = LocalBuffer::from_dpapi(output.pbData) else {
        return Err(ChromeError::CookieKeyInvalid(
            "DPAPI returned a null output buffer".into(),
        ));
    };
    let output_len = usize::try_from(output.cbData)
        .map_err(|_| ChromeError::CookieKeyInvalid("DPAPI output is too large".into()))?;
    if output_len != WINDOWS_KEY_LEN {
        return Err(ChromeError::CookieKeyInvalid(format!(
            "DPAPI returned {output_len} bytes, expected {WINDOWS_KEY_LEN}"
        )));
    }
    // SAFETY: successful DPAPI output gives this sole `LocalBuffer` owner a
    // non-null allocation containing `output.cbData` initialized bytes. The
    // checked length is exactly `WINDOWS_KEY_LEN`, so it is within `isize::MAX`;
    // `u8` has alignment one. The allocation remains live, is not mutated or
    // aliased mutably, and the slice is copied before the guard is dropped.
    let plaintext =
        unsafe { std::slice::from_raw_parts(output_buffer.as_ptr(), output_len) }.to_vec();
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};

    #[cfg(not(target_os = "windows"))]
    fn encrypt_macos_v10(key: &[u8], plaintext: &[u8]) -> Vec<u8> {
        use cbc::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
        type Enc = cbc::Encryptor<aes::Aes128>;
        let iv = [0x20u8; 16];
        let mut buf = vec![0u8; plaintext.len() + 16];
        buf[..plaintext.len()].copy_from_slice(plaintext);
        let ct = Enc::new_from_slices(key, &iv)
            .unwrap()
            .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
            .unwrap();
        let mut out = V10.to_vec();
        out.extend_from_slice(ct);
        out
    }

    fn encrypt_windows_v10(key: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let nonce = [3u8; GCM_NONCE_LEN];
        let cipher = Aes256Gcm::new_from_slice(key).unwrap();
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .unwrap();
        [V10, &nonce, &ciphertext].concat()
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn decrypts_macos_v10_without_prefix() {
        let key = [7u8; MACOS_KEY_LEN];
        let encrypted = encrypt_macos_v10(&key, b"auth_token_value");
        assert_eq!(
            decrypt_value(&key, &encrypted).unwrap(),
            Some(b"auth_token_value".to_vec())
        );
    }

    #[test]
    fn decrypts_windows_dpapi_provider_v10_layout() {
        let key = [9u8; WINDOWS_KEY_LEN];
        let encrypted = encrypt_windows_v10(&key, b"ct0_value");
        assert_eq!(decrypt_windows_v10(&key, &encrypted).unwrap(), b"ct0_value");
    }

    #[test]
    fn v24_host_digest_is_verified_before_stripping() {
        let host = ".x.com";
        let mut plaintext = Sha256::digest(host.as_bytes()).to_vec();
        plaintext.extend_from_slice(b"cookie-value");
        assert_eq!(
            verify_and_strip_host_prefix(plaintext.clone(), host, true).unwrap(),
            "cookie-value"
        );
        assert!(matches!(
            verify_and_strip_host_prefix(plaintext, ".attacker.example", true),
            Err(ChromeError::CookieHostIntegrityFailed(_))
        ));
    }

    #[test]
    fn cookie_host_suffix_requires_a_dns_label_boundary() {
        assert!(host_matches_suffix("x.com", "x.com"));
        assert!(host_matches_suffix(".x.com", "x.com"));
        assert!(host_matches_suffix("api.x.com", "x.com"));
        assert!(host_matches_suffix(".www.instagram.com", ".instagram.com"));
        assert!(!host_matches_suffix("evilx.com", "x.com"));
        assert!(!host_matches_suffix("fakeinstagram.com", "instagram.com"));
        assert!(!host_matches_suffix("x.com.attacker.example", "x.com"));
        assert!(!host_matches_suffix("x.com", ""));
    }

    #[test]
    fn modern_cookie_store_wins_and_missing_store_is_not_created() {
        let root = tempfile::tempdir().unwrap();
        let modern = root.path().join("Default/Network/Cookies");
        std::fs::create_dir_all(modern.parent().unwrap()).unwrap();
        std::fs::write(&modern, []).unwrap();
        std::fs::write(root.path().join("Default/Cookies"), []).unwrap();
        assert_eq!(cookie_store_path(root.path()).unwrap(), modern);

        let missing = tempfile::tempdir().unwrap();
        assert!(matches!(
            cookie_store_path(missing.path()),
            Err(ChromeError::CookieStoreNotFound(_))
        ));
        assert!(!missing.path().join("Default/Cookies").exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn app_bound_cookie_is_rejected_with_a_typed_error() {
        assert!(matches!(
            decrypt_value(&[1; WINDOWS_KEY_LEN], b"v20ciphertext"),
            Err(ChromeError::AppBoundCookieUnsupported)
        ));
    }
}
