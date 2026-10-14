//! Runtime-owned email completion copy. Provider receipts, event names and
//! internal fingerprints remain owner data and are never interpolated here.

use serde::Deserialize;

use crate::PresentationLocale;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailCompletionCopy {
    title: String,
    sent: String,
    replayed: String,
    batch_completed: String,
    batch_replayed: String,
    batch_unknown: String,
}

fn catalog(locale: PresentationLocale) -> &'static EmailCompletionCopy {
    static EN: std::sync::OnceLock<EmailCompletionCopy> = std::sync::OnceLock::new();
    static ZH: std::sync::OnceLock<EmailCompletionCopy> = std::sync::OnceLock::new();
    let (cell, source) = match locale {
        PresentationLocale::English => (&EN, include_str!("locales/en-US.json")),
        PresentationLocale::SimplifiedChinese => (&ZH, include_str!("locales/zh-CN.json")),
    };
    cell.get_or_init(|| {
        serde_json::from_str(source).expect("bundled email completion locale is valid")
    })
}

pub(super) fn title(locale: PresentationLocale) -> &'static str {
    &catalog(locale).title
}

pub(super) fn sent(locale: PresentationLocale, replayed: bool) -> &'static str {
    let copy = catalog(locale);
    if replayed { &copy.replayed } else { &copy.sent }
}

pub(super) fn batch_completed(
    locale: PresentationLocale,
    accepted: usize,
    rejected: usize,
    replayed: bool,
) -> String {
    let copy = catalog(locale);
    let template = if replayed {
        &copy.batch_replayed
    } else {
        &copy.batch_completed
    };
    replace_counts(template, accepted, rejected, 0)
}

pub(super) fn batch_unknown(
    locale: PresentationLocale,
    accepted: usize,
    rejected: usize,
    unknown: usize,
) -> String {
    replace_counts(&catalog(locale).batch_unknown, accepted, rejected, unknown)
}

fn replace_counts(template: &str, accepted: usize, rejected: usize, unknown: usize) -> String {
    template
        .replace("{accepted}", &accepted.to_string())
        .replace("{rejected}", &rejected.to_string())
        .replace("{unknown}", &unknown.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_locales_cover_safe_completion_copy() {
        assert_eq!(
            sent(PresentationLocale::SimplifiedChinese, false),
            "邮件服务已接受该邮件，OneCrew 已完成记录。"
        );
        assert_eq!(
            sent(PresentationLocale::SimplifiedChinese, true),
            "OneCrew 返回了此前已记录的发送结果；未再次发送邮件。"
        );
        assert_eq!(
            batch_completed(PresentationLocale::English, 2, 1, false),
            "The email batch completed: 2 accepted, 1 rejected."
        );
        assert!(
            batch_unknown(PresentationLocale::SimplifiedChinese, 2, 1, 3)
                .contains("结果不确定 3 封")
        );
    }
}
