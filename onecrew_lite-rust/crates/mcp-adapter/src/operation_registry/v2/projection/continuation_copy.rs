//! Localized visible continuation messages. The Host shows them in the
//! user's turn, so they stay short, business-language, and free of IDs.
use onecrew_local_runtime::PresentationLocale;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry::v2) struct ContinuationCopy {
    pub(in crate::operation_registry::v2) search_plan_confirmed: String,
    pub(in crate::operation_registry::v2) cancelled: String,
    pub(in crate::operation_registry::v2) execution: String,
}

pub(in crate::operation_registry::v2) fn continuation_copy(
    locale: PresentationLocale,
) -> &'static ContinuationCopy {
    static EN: std::sync::OnceLock<ContinuationCopy> = std::sync::OnceLock::new();
    static ZH: std::sync::OnceLock<ContinuationCopy> = std::sync::OnceLock::new();
    let (cell, source) = match locale {
        PresentationLocale::English => (&EN, include_str!("locales/en-US/continuation.json")),
        PresentationLocale::SimplifiedChinese => {
            (&ZH, include_str!("locales/zh-CN/continuation.json"))
        }
    };
    cell.get_or_init(|| serde_json::from_str(source).expect("bundled continuation locale is valid"))
}
