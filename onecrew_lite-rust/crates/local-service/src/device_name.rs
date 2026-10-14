//! Side-effect-free native computer display identity for enrollment.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use onecrew_creator_sync::{DeviceLabel, DeviceLabelError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeviceNameError {
    Unavailable,
    /// Produced only where a native UTF-16 name source exists.
    #[cfg(any(target_os = "macos", target_os = "windows", test))]
    InvalidEncoding,
    InvalidLabel(DeviceLabelError),
}

/// Synchronous OS calls belong on the caller's blocking boundary.
pub(super) fn current_device_label() -> Result<DeviceLabel, DeviceNameError> {
    #[cfg(target_os = "macos")]
    let value = macos::computer_name()?;
    #[cfg(target_os = "windows")]
    let value = windows::computer_name()?;
    // Hosts without a native name source report the capability as absent;
    // the label check below is still compiled so the error type stays uniform.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let value: String = Err(DeviceNameError::Unavailable)?;
    DeviceLabel::new(value).map_err(DeviceNameError::InvalidLabel)
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn decode_name(value: &[u16]) -> Result<String, DeviceNameError> {
    String::from_utf16(value).map_err(|_| DeviceNameError::InvalidEncoding)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_unicode_decode_never_replaces_invalid_code_units() {
        let name = "river-fan 的电脑 🖥️";
        assert_eq!(
            decode_name(&name.encode_utf16().collect::<Vec<_>>()).unwrap(),
            name
        );
        assert_eq!(
            decode_name(&[0xd800]),
            Err(DeviceNameError::InvalidEncoding)
        );
        assert_eq!(
            decode_name(&[0xdc00]),
            Err(DeviceNameError::InvalidEncoding)
        );
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    #[ignore = "Explicit read-only native-host evidence; normal unit tests do not assume a host name"]
    fn native_computer_name_readback() {
        let label =
            current_device_label().expect("system computer name must be readable and valid");
        println!("Native computer name: {}", label.as_str());
    }
}
