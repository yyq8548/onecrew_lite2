//! Creator-owned outcome counts and visible copy shared by terminal cards and
//! continuations. Locale comes from the presented result, never global settings.

use crate::{CreatorSaveBatchOperation, CreatorSaveBatchOutcome, PresentationLocale};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CreatorSaveCopy {
    title: String,
    lead: String,
    saved: String,
    failed: String,
    unknown: String,
    not_run: String,
    not_selected: String,
    separator: String,
    tail: String,
}

#[derive(Default)]
pub(super) struct CreatorSaveCounts {
    saved: usize,
    failed: usize,
    unknown: usize,
    not_run: usize,
    not_selected: usize,
}

impl CreatorSaveCounts {
    pub(super) fn from_rows(rows: &[CreatorSaveBatchOutcome]) -> Self {
        let mut counts = Self::default();
        for row in rows {
            match row.operation {
                CreatorSaveBatchOperation::Added | CreatorSaveBatchOperation::Updated => {
                    counts.saved += 1
                }
                CreatorSaveBatchOperation::Failed => counts.failed += 1,
                CreatorSaveBatchOperation::Unknown => counts.unknown += 1,
                CreatorSaveBatchOperation::ExistingSkipped
                | CreatorSaveBatchOperation::DuplicateSkipped => counts.not_run += 1,
                CreatorSaveBatchOperation::NotSelected => counts.not_selected += 1,
            }
        }
        counts
    }
}

fn creator_save_copy(locale: PresentationLocale) -> &'static CreatorSaveCopy {
    static EN: std::sync::OnceLock<CreatorSaveCopy> = std::sync::OnceLock::new();
    static ZH: std::sync::OnceLock<CreatorSaveCopy> = std::sync::OnceLock::new();
    let (cell, source) = match locale {
        PresentationLocale::English => (&EN, include_str!("locales/en-US.json")),
        PresentationLocale::SimplifiedChinese => (&ZH, include_str!("locales/zh-CN.json")),
    };
    cell.get_or_init(|| serde_json::from_str(source).expect("bundled Creator save locale is valid"))
}

pub(super) fn creator_save_title(locale: PresentationLocale) -> &'static str {
    &creator_save_copy(locale).title
}

pub(super) fn creator_save_batch_message(
    locale: PresentationLocale,
    counts: &CreatorSaveCounts,
) -> String {
    let copy = creator_save_copy(locale);
    let parts = [
        (&copy.saved, counts.saved),
        (&copy.failed, counts.failed),
        (&copy.unknown, counts.unknown),
        (&copy.not_run, counts.not_run),
        (&copy.not_selected, counts.not_selected),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(template, count)| template.replace("{count}", &count.to_string()))
    .collect::<Vec<_>>();
    format!("{}{}{}", copy.lead, parts.join(&copy.separator), copy.tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_outcomes_retain_all_states_and_no_repeat_instruction() {
        let counts = CreatorSaveCounts {
            saved: 1,
            failed: 2,
            unknown: 3,
            not_run: 4,
            not_selected: 5,
        };
        assert_eq!(
            creator_save_batch_message(PresentationLocale::English, &counts),
            "Creator save finished: 1 saved, 2 failed, 3 uncertain, 4 not run, 5 not selected. Do not repeat completed or uncertain saves."
        );
        assert_eq!(
            creator_save_batch_message(PresentationLocale::SimplifiedChinese, &counts),
            "达人保存完成：已保存 1 个，失败 2 个，结果不确定 3 个，未执行 4 个，未选择 5 个。不要重复已完成或结果不确定的保存。"
        );
        let partial = CreatorSaveCounts {
            saved: 4,
            not_selected: 1,
            ..Default::default()
        };
        assert_eq!(
            creator_save_batch_message(PresentationLocale::SimplifiedChinese, &partial),
            "达人保存完成：已保存 4 个，未选择 1 个。不要重复已完成或结果不确定的保存。"
        );
    }
}
