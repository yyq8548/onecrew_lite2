//! Use the physical computer's Unicode DNS name, including on cluster nodes.
use super::{DeviceNameError, decode_name};
use windows_sys::Win32::{
    Foundation::{ERROR_MORE_DATA, GetLastError},
    System::SystemInformation::{ComputerNamePhysicalDnsHostname, GetComputerNameExW},
};

pub(super) fn computer_name() -> Result<String, DeviceNameError> {
    let mut required = 0;
    // SAFETY: A NULL buffer and zero size request the required UTF-16 capacity.
    // The writable size pointer remains valid for this synchronous Win32 call.
    let result = unsafe {
        GetComputerNameExW(
            ComputerNamePhysicalDnsHostname,
            std::ptr::null_mut(),
            &mut required,
        )
    };
    if result != 0
        // SAFETY: GetLastError reads this thread's status immediately after failure.
        || unsafe { GetLastError() } != ERROR_MORE_DATA
        || required == 0
        || required > 4097
    {
        return Err(DeviceNameError::Unavailable);
    }
    let mut buffer = vec![0u16; required as usize];
    let mut written = required;
    // SAFETY: The buffer has required initialized UTF-16 slots; Win32 receives
    // that exact capacity and a valid mutable length pointer, with no aliases.
    if unsafe {
        GetComputerNameExW(
            ComputerNamePhysicalDnsHostname,
            buffer.as_mut_ptr(),
            &mut written,
        )
    } == 0
    {
        return Err(DeviceNameError::Unavailable);
    }
    let written = written as usize;
    // Success excludes the terminating NUL from written. Reject inconsistent
    // API output or a concurrent name growth instead of returning a prefix.
    if written >= buffer.len() || buffer[written] != 0 {
        return Err(DeviceNameError::InvalidEncoding);
    }
    decode_name(&buffer[..written])
}
