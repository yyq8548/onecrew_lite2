//! Runtime-owned fixed copy. IDs, protocol values and user text are never
//! translated; a presented result supplies its own frozen locale.

use std::{collections::BTreeMap, sync::OnceLock};

use crate::{PresentationLocale, PresentationProjection, PresentationSection, ResultStoreError};

fn catalog(locale: PresentationLocale) -> &'static BTreeMap<String, String> {
    static EN: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    static ZH: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    let (cell, source) = match locale {
        PresentationLocale::English => (&EN, include_str!("locales/en-US.json")),
        PresentationLocale::SimplifiedChinese => (&ZH, include_str!("locales/zh-CN.json")),
    };
    cell.get_or_init(|| {
        serde_json::from_str(source).expect("bundled outreach copy catalog is valid")
    })
}

pub(super) fn text(locale: PresentationLocale, source: &'static str) -> &'static str {
    catalog(locale)
        .get(source)
        .expect("runtime-owned outreach copy has both translations")
}

pub(crate) fn review_confirmation_text(locale: PresentationLocale) -> (&'static str, &'static str) {
    (
        text(locale, "Confirm this external action?"),
        text(locale, "Confirm action"),
    )
}

/// Called only for the sealed outreach projection, never arbitrary peer copy.
pub(crate) fn localize_review(
    projection: &mut PresentationProjection,
    locale: PresentationLocale,
) -> Result<(), ResultStoreError> {
    projection.title = translated(locale, &projection.title)?.into();
    if let Some(summary) = &mut projection.summary {
        *summary = translated(locale, summary)?.into();
    }
    for section in &mut projection.sections {
        if let PresentationSection::Fields { fields, .. } = section {
            for field in fields {
                field.label = match field.label.rsplit_once(" (part ") {
                    Some((base, part)) => format!(
                        "{} ({} {})",
                        translated(locale, base)?,
                        text(locale, "part"),
                        part.trim_end_matches(')')
                    ),
                    None => translated(locale, &field.label)?.into(),
                };
            }
        }
    }
    Ok(())
}

pub(super) fn translated(
    locale: PresentationLocale,
    source: &str,
) -> Result<&'static str, ResultStoreError> {
    catalog(locale)
        .get(source)
        .map(String::as_str)
        .ok_or(ResultStoreError::InvalidInput("outreach_copy_missing"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_catalogs_cover_the_same_owned_copy_and_distinct_remote_states() {
        let en = catalog(PresentationLocale::English);
        let zh = catalog(PresentationLocale::SimplifiedChinese);
        assert_eq!(en.keys().collect::<Vec<_>>(), zh.keys().collect::<Vec<_>>());
        assert!(zh.values().all(|text| !text.trim().is_empty()));
        let pending = text(
            PresentationLocale::SimplifiedChinese,
            "The remote effect is verified; Creator recording is pending. Retry this exact intent with the same key to finish recording without sending again.",
        );
        assert!(pending.contains("已核验") && pending.contains("补记"));
        assert!(text(PresentationLocale::SimplifiedChinese, "The remote effect is unknown. Keep this key and inspect or resume this exact intent; do not send with a new key.").contains("效果未知"));
    }

    #[test]
    fn translating_a_review_never_translates_the_reviewed_body_or_identifier() {
        let mut projection = PresentationProjection {
            kind: crate::ResultPresentationKind::new("outreach_confirmation").unwrap(),
            title: "Review direct message".into(),
            summary: Some(
                "Confirm the exact operator, stable target and content shown below.".into(),
            ),
            sections: vec![PresentationSection::Fields {
                heading: None,
                fields: vec![
                    crate::PresentationField {
                        label: "Message (part 2)".into(),
                        value: "Creator".into(),
                    },
                    crate::PresentationField {
                        label: "Stable target ID".into(),
                        value: "123".into(),
                    },
                ],
            }],
        };
        localize_review(&mut projection, PresentationLocale::SimplifiedChinese).unwrap();
        assert_eq!(projection.title, "确认私信");
        let PresentationSection::Fields { fields, .. } = &projection.sections[0] else {
            panic!("fields");
        };
        assert_eq!(fields[0].value, "Creator");
        assert!(fields[0].label.starts_with("私信正文"));
        assert_eq!(fields[1].value, "123");
    }
}
