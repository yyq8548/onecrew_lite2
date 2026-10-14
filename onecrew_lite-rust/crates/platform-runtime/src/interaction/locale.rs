//! Display preferences shared with the native shell; never interaction authority.
use std::sync::atomic::{AtomicBool, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PresentationLocale {
    #[default]
    #[serde(rename = "en-US")]
    English,
    #[serde(rename = "zh-CN")]
    SimplifiedChinese,
}

/// Process-local projection of the Desktop's already-persisted language setting.
#[derive(Default)]
pub struct PresentationPreferences {
    chinese: AtomicBool,
}

impl PresentationPreferences {
    pub fn set_locale(&self, locale: PresentationLocale) {
        self.chinese.store(
            locale == PresentationLocale::SimplifiedChinese,
            Ordering::Release,
        );
    }

    pub fn locale(&self) -> PresentationLocale {
        if self.chinese.load(Ordering::Acquire) {
            PresentationLocale::SimplifiedChinese
        } else {
            PresentationLocale::English
        }
    }
}

impl crate::LocalPlatform {
    /// Attach the shell's shared preference before publishing this runtime.
    pub fn with_presentation_preferences(
        mut self,
        preferences: std::sync::Arc<PresentationPreferences>,
    ) -> Self {
        self.presentation_preferences = preferences;
        self
    }
}
