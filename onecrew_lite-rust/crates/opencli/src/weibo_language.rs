//! Audience originals from one fixed native post ID, never a display-name lookup.
use crate::{NativeCommentSource, OpenCli, OpenCliError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WeiboOriginalComment {
    pub id: String,
    pub text: String,
}

fn numeric(id: &str) -> bool {
    (1..=20).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())
}

fn project(rows: &[serde_json::Value]) -> Result<Vec<WeiboOriginalComment>, OpenCliError> {
    if rows.len() > 5 {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let id = row["id"]
                .as_str()
                .filter(|id| numeric(id))
                .ok_or(OpenCliError::InvalidOutput)?;
            let text = row["text"].as_str().ok_or(OpenCliError::InvalidOutput)?;
            if row["rank"].as_u64() != Some(index as u64 + 1)
                || row["text_truncated"] != false
                || text.trim().is_empty()
                || text.len() > 2000
                || text.contains('\0')
                || !seen.insert(id)
            {
                return Err(OpenCliError::InvalidOutput);
            }
            // Commenter identity and display labels do not certify the post author.
            Ok(WeiboOriginalComment {
                id: id.into(),
                text: text.into(),
            })
        })
        .collect()
}

impl OpenCli {
    /// Only one request. The caller supplies the native numeric post ID from a
    /// saved author-bound post receipt, not the unrelated base62 share locator.
    pub async fn weibo_original_comments(
        &self,
        profile: &str,
        native_post_id: &str,
    ) -> Result<Vec<WeiboOriginalComment>, OpenCliError> {
        if !numeric(native_post_id) {
            return Err(OpenCliError::InvalidInput("weibo_comment_post"));
        }
        let rows = self
            .native_comments(profile, NativeCommentSource::WeiboPost(native_post_id), 5)
            .await?;
        project(&rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rows() -> Vec<serde_json::Value> {
        (1..=5).map(|i|serde_json::json!({"rank":i,"id":format!("1234{i}"),"text":format!("这款键盘的第{i}个测评很好"),"text_truncated":false,"native_author_id":null,"author":"not author evidence"})).collect()
    }
    #[test]
    fn weibo_comment_projection_preserves_first_five_original_ids_and_order() {
        let evidence = project(&rows()).unwrap();
        assert_eq!(evidence.len(), 5);
        assert_eq!(evidence[0].id, "12341");
        assert_eq!(evidence[4].id, "12345");
        assert_eq!(
            serde_json::to_value(&evidence[0])
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn weibo_comment_projection_refuses_sixth_refill_duplicates_truncation_and_rank_drift() {
        let mut six = rows();
        six.push(
            serde_json::json!({"rank":6,"id":"99999","text":"不能补足票数","text_truncated":false}),
        );
        assert!(project(&six).is_err());
        for (key, value) in [
            ("id", serde_json::json!("12341")),
            ("text_truncated", serde_json::json!(true)),
            ("rank", serde_json::json!(6)),
            ("text", serde_json::json!("")),
            ("id", serde_json::json!(12342)),
        ] {
            let mut bad = rows();
            bad[1][key] = value;
            assert!(project(&bad).is_err());
        }
        let mut too_long = rows();
        too_long[0]["text"] = serde_json::json!("字".repeat(667));
        assert!(project(&too_long).is_err());
    }
}
