//! Read System Settings' ComputerName, not LocalHostName or the login user.
use super::{DeviceNameError, decode_name};
use std::ffi::c_void;

#[repr(C)]
struct CFRange {
    location: isize,
    length: isize,
}

#[link(name = "SystemConfiguration", kind = "framework")]
unsafe extern "C" {
    fn SCDynamicStoreCopyComputerName(store: *const c_void, encoding: *mut u32) -> *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringGetLength(value: *const c_void) -> isize;
    fn CFStringGetCharacters(value: *const c_void, range: CFRange, buffer: *mut u16);
    fn CFRelease(value: *const c_void);
}

struct OwnedComputerName(*const c_void);
impl Drop for OwnedComputerName {
    fn drop(&mut self) {
        // SAFETY: Only a non-null Copy-rule CFString is wrapped, and this owner
        // releases that single retained reference exactly once after all reads.
        unsafe {
            CFRelease(self.0);
        }
    }
}

pub(super) fn computer_name() -> Result<String, DeviceNameError> {
    // SAFETY: NULL requests a temporary SystemConfiguration session and no
    // encoding output. The API returns an owned CFString or NULL on failure.
    let raw = unsafe { SCDynamicStoreCopyComputerName(std::ptr::null(), std::ptr::null_mut()) };
    if raw.is_null() {
        return Err(DeviceNameError::Unavailable);
    }
    let value = OwnedComputerName(raw);
    // SAFETY: value owns a live CFString throughout this read.
    let length = unsafe { CFStringGetLength(value.0) };
    let length = usize::try_from(length).map_err(|_| DeviceNameError::InvalidEncoding)?;
    // Bound allocation independently of Access's shorter display-label policy.
    if length > 4096 {
        return Err(DeviceNameError::InvalidLabel(
            onecrew_creator_sync::DeviceLabelError::TooLong,
        ));
    }
    let mut characters = vec![0u16; length];
    // SAFETY: The valid range covers exactly length UTF-16 units, and the
    // initialized writable buffer has that capacity. value remains retained.
    unsafe {
        CFStringGetCharacters(
            value.0,
            CFRange {
                location: 0,
                length: length as isize,
            },
            characters.as_mut_ptr(),
        );
    }
    decode_name(&characters)
}
