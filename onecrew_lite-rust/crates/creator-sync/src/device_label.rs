//! Validated display identity supplied by the native enrollment owner.
//! This type is intentionally not deserializable from IPC or provider input.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceLabel(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceLabelError {
    Empty,
    TooLong,
    ForbiddenCharacter,
}

impl DeviceLabel {
    pub fn new(value: String) -> Result<Self, DeviceLabelError> {
        let value = value.trim_matches(ecmascript_whitespace);
        if value.is_empty() {
            return Err(DeviceLabelError::Empty);
        }
        // Access validates string.length: UTF-16 units, not bytes or scalars.
        if value.encode_utf16().count() > 80 {
            return Err(DeviceLabelError::TooLong);
        }
        if value.chars().any(|c| matches!(c, '\u{0000}'..='\u{001f}' | '\u{007f}'..='\u{009f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')) {
            return Err(DeviceLabelError::ForbiddenCharacter);
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn ecmascript_whitespace(value: char) -> bool {
    matches!(value, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_preserve_unicode_and_trim_like_access_without_truncation() {
        for value in [
            "river-fan-mac-mini-M4",
            "小凡的电脑 🖥️",
            "Cafe\u{301}",
            "A&B + % # ?",
        ] {
            assert_eq!(
                DeviceLabel::new(format!("\u{feff} {value} \u{3000}"))
                    .unwrap()
                    .as_str(),
                value
            );
        }
        assert!(DeviceLabel::new("界".repeat(80)).is_ok());
        assert_eq!(
            DeviceLabel::new("界".repeat(81)),
            Err(DeviceLabelError::TooLong)
        );
        assert!(DeviceLabel::new("😀".repeat(40)).is_ok());
        assert_eq!(
            DeviceLabel::new("😀".repeat(41)),
            Err(DeviceLabelError::TooLong)
        );
        assert_eq!(
            DeviceLabel::new(" \n\u{feff}".into()),
            Err(DeviceLabelError::Empty)
        );
    }
    #[test]
    fn labels_reject_control_and_direction_override_characters() {
        for code in (0..=31)
            .chain(127..=159)
            .chain(0x202a..=0x202e)
            .chain(0x2066..=0x2069)
        {
            let c = char::from_u32(code).unwrap();
            assert_eq!(
                DeviceLabel::new(format!("computer{c}name")),
                Err(DeviceLabelError::ForbiddenCharacter)
            );
        }
        assert_eq!(
            DeviceLabel::new("\u{85}computer".into()),
            Err(DeviceLabelError::ForbiddenCharacter)
        );
    }
}
