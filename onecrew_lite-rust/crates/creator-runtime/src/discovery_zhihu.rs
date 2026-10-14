//! Creator evidence policy. Native transport and HTML projection belong to OpenCLI.
use serde_json::Value;

fn exact(value: &Value, keys: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|row| row.len() == keys.len() && keys.iter().all(|key| row.contains_key(*key)))
}
fn text(value: &Value, max: usize) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.len() <= max && !s.contains('\0'))
}
fn number(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        (1..=20).contains(&s.len()) && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit())
    })
}
fn author(value: &Value) -> bool {
    let id = value["author_id"].as_str().unwrap_or_default();
    let token = value["url_token"].as_str().unwrap_or_default();
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && (1..=128).contains(&token.len())
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        && value["author_url"] == format!("https://www.zhihu.com/people/{token}")
        && value["author_label"].as_str().is_some_and(|s| {
            !s.trim().is_empty() && s.len() <= 512 && !s.chars().any(char::is_control)
        })
}
pub(crate) fn valid_profile(value: &Value) -> bool {
    exact(
        value,
        &[
            "author_id",
            "url_token",
            "author_label",
            "author_url",
            "follower_count",
            "bio",
        ],
    ) && author(value)
        && text(&value["bio"], 8000)
        && (value["follower_count"].is_null()
            || value["follower_count"]
                .as_u64()
                .is_some_and(|n| n <= 9_007_199_254_740_991))
}
pub(crate) fn valid_content(value: &Value, id: &str, source: &str) -> bool {
    let source_matches = if value.get("article_id").is_some() {
        exact(
            value,
            &[
                "author_id",
                "url_token",
                "author_label",
                "author_url",
                "article_id",
                "source_url",
                "title",
                "text",
                "public_links",
            ],
        ) && number(&value["article_id"])
            && text(&value["title"], 2000)
            && value["article_id"]
                .as_str()
                .is_some_and(|key| source == format!("https://zhuanlan.zhihu.com/p/{key}"))
    } else {
        exact(
            value,
            &[
                "author_id",
                "url_token",
                "author_label",
                "author_url",
                "answer_id",
                "question_id",
                "source_url",
                "text",
                "public_links",
            ],
        ) && number(&value["answer_id"])
            && number(&value["question_id"])
            && source
                == format!(
                    "https://www.zhihu.com/question/{}/answer/{}",
                    value["question_id"].as_str().unwrap_or_default(),
                    value["answer_id"].as_str().unwrap_or_default()
                )
    };
    source_matches && author(value) && value["author_id"] == id && value["source_url"] == source
        && text(&value["text"], 16000) && value["text"].as_str().is_some_and(|s| !s.trim().is_empty())
        && value["public_links"].as_array().is_some_and(|links| {
            let mut seen = std::collections::HashSet::new();
            links.len() <= 10 && links.iter().all(|value| value.as_str().is_some_and(|link| {
                seen.insert(link) && link.len() <= 2048 && !link.chars().any(char::is_control)
                    && url::Url::parse(link).is_ok_and(|url| {
                        url.username().is_empty() && url.password().is_none() && url.port().is_none()
                            && !url.query_pairs().any(|(key, _)| matches!(key.to_ascii_lowercase().as_str(), "token" | "access_token" | "api_key" | "authorization" | "cookie" | "session" | "password" | "secret" | "auth"))
                            && match url.scheme() {
                                "mailto" => url.query().is_none() && url.fragment().is_none() && url.path().contains('@') && !url.path().contains(['/', '%']),
                                "https" => matches!(url.host(), Some(url::Host::Domain(host)) if host.contains('.') && !host.ends_with(".local") && !host.ends_with(".internal") && !host.ends_with(".localhost")),
                                _ => false,
                            }
                    })
            }))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn author() -> Value {
        json!({"author_id":"a".repeat(32), "url_token":"creator-123", "author_label":"Creator", "author_url":"https://www.zhihu.com/people/creator-123"})
    }
    #[test]
    fn profile_requires_native_identity_without_fabricating_followers() {
        let mut row = author();
        row["bio"] = "商务联系 contact@example.com".into();
        row["follower_count"] = Value::Null;
        assert!(valid_profile(&row));
        row["follower_count"] = json!(0);
        assert!(valid_profile(&row));
        row.as_object_mut().unwrap().remove("follower_count");
        assert!(!valid_profile(&row));
        row["follower_count"] = Value::Null;
        row["author_id"] = "Creator".into();
        assert!(!valid_profile(&row));
    }
    #[test]
    fn answer_and_article_bind_distinct_sources_and_reject_contact_secrets() {
        for article in [false, true] {
            let mut row = author();
            let source = if article {
                row["article_id"] = "123".into();
                row["title"] = "键盘".into();
                "https://zhuanlan.zhihu.com/p/123"
            } else {
                row["answer_id"] = "123".into();
                row["question_id"] = "456".into();
                "https://www.zhihu.com/question/456/answer/123"
            };
            row["source_url"] = source.into();
            row["text"] = "原文内容".into();
            row["public_links"] = json!(["mailto:contact@example.com"]);
            assert!(valid_content(&row, &"a".repeat(32), source));
            assert!(!valid_content(&row, &"b".repeat(32), source));
            row["public_links"] = json!(["https://example.com/?token=secret"]);
            assert!(!valid_content(&row, &"a".repeat(32), source));
        }
    }
}
