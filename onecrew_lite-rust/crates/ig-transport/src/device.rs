//! Android device profile + UUIDs the private API requires. Values mirror
//! instagrapi's defaults so a session can be persisted and reused verbatim.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The per-install identifiers Instagram ties a session to. Generate once at
/// import and never regenerate — Instagram flags device churn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uuids {
    pub phone_id: String,
    pub uuid: String,
    pub client_session_id: String,
    pub advertising_id: String,
    pub android_device_id: String,
    pub request_id: String,
    pub tray_session_id: String,
}

impl Uuids {
    pub fn generate() -> Self {
        Self {
            phone_id: uuid4(),
            uuid: uuid4(),
            client_session_id: uuid4(),
            advertising_id: uuid4(),
            android_device_id: android_device_id(),
            request_id: uuid4(),
            tray_session_id: uuid4(),
        }
    }
}

fn uuid4() -> String {
    Uuid::new_v4().to_string()
}

fn android_device_id() -> String {
    format!("android-{:016x}", rand::random::<u64>())
}

/// Hardware/app profile used to build the `User-Agent` and device headers.
/// Defaults to a Pixel 8 Pro on the pinned app version (matches instagrapi).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSettings {
    pub app_version: String,
    pub android_version: u32,
    pub android_release: String,
    pub dpi: String,
    pub resolution: String,
    pub manufacturer: String,
    pub device: String,
    pub model: String,
    pub cpu: String,
    pub version_code: String,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            app_version: "428.0.0.47.67".into(),
            android_version: 34,
            android_release: "14".into(),
            dpi: "480dpi".into(),
            resolution: "1344x2992".into(),
            manufacturer: "Google/google".into(),
            device: "husky".into(),
            model: "Pixel 8 Pro".into(),
            cpu: "husky".into(),
            version_code: "961145276".into(),
        }
    }
}

impl DeviceSettings {
    /// `Instagram <app> Android (<sdk>/<rel>; <dpi>; <res>; <maker>; <model>; <device>; <cpu>; <locale>; <code>)`
    pub fn user_agent(&self, locale: &str) -> String {
        format!(
            "Instagram {} Android ({}/{}; {}; {}; {}; {}; {}; {}; {}; {})",
            self.app_version,
            self.android_version,
            self.android_release,
            self.dpi,
            self.resolution,
            self.manufacturer,
            self.model,
            self.device,
            self.cpu,
            locale,
            self.version_code,
        )
    }
}
