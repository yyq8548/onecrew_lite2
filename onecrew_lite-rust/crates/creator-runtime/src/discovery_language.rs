//! Bounded original-language evidence, separate from an author's first content read.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryLanguageComment {
    pub source_url: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryLanguageSample {
    pub author_id: String,
    pub source_url: String,
    pub text: String,
    pub comments: Vec<DiscoveryLanguageComment>,
}

#[derive(Debug, Clone)]
pub struct DiscoveryLanguageClaim {
    pub campaign_id: String,
    pub revision: u32,
    pub search_hit_id: String,
    pub candidate_id: String,
    pub platform: String,
    pub profile_ref: String,
    pub author_id: String,
    pub author_handle: String,
    pub root_url: String,
    pub ordinal: u32,
    pub call_number: u32,
    pub previous_urls: Vec<String>,
    pub saved_content_urls: Vec<String>,
}

impl DiscoveryLanguageSample {
    pub fn valid_for(&self, claim: &DiscoveryLanguageClaim) -> bool {
        if claim.ordinal != 1
            || !claim.previous_urls.is_empty()
            || self.source_url != claim.root_url
            || self.comments.len() > 5
        {
            return false;
        }
        if claim.platform == "douyin" {
            let prefix = format!("{}#comment-", self.source_url);
            let mut seen = std::collections::HashSet::new();
            return crate::discovery_douyin::valid_author(&self.author_id)
                && self.author_id == claim.author_id
                && claim.author_handle == claim.author_id
                && crate::discovery_douyin::post_id(&self.source_url).is_some()
                && !self.text.trim().is_empty()
                && self.text.len() <= 12000
                && !self.text.contains('\0')
                && self.comments.iter().all(|c| {
                    c.source_url.strip_prefix(&prefix).is_some_and(|id| {
                        (1..=20).contains(&id.len())
                            && !id.starts_with('0')
                            && id.bytes().all(|b| b.is_ascii_digit())
                    }) && seen.insert(&c.source_url)
                        && c.text.len() <= 2000
                        && !c.text.contains('\0')
                });
        }
        if claim.platform == "linkedin" {
            let prefix = format!("{}#comment-", self.source_url);
            let mut seen = std::collections::HashSet::new();
            return crate::discovery_linkedin::valid_account(&self.author_id)
                && self.author_id == claim.author_id
                && crate::discovery_linkedin::valid_handle(&claim.author_handle)
                && crate::discovery_linkedin::activity_id(&self.source_url).is_some()
                && !self.text.trim().is_empty()
                && self.text.len() <= 16000
                && !self.text.contains('\0')
                && self.comments.iter().all(|c| {
                    c.source_url.strip_prefix(&prefix).is_some_and(|id| {
                        (1..=20).contains(&id.len())
                            && !id.starts_with('0')
                            && id.bytes().all(|b| b.is_ascii_digit())
                    }) && seen.insert(&c.source_url)
                        && c.text.len() <= 2000
                        && !c.text.contains('\0')
                });
        }
        if claim.platform == "hackernews" {
            let root = crate::discovery_hackernews::item_id(&claim.root_url);
            let mut seen = std::collections::HashSet::new();
            return root.is_some()
                && crate::discovery_hackernews::valid_username(&self.author_id)
                && self.author_id == claim.author_id
                && self.source_url == claim.root_url
                && claim.ordinal == 1
                && claim.previous_urls.is_empty()
                && !self.text.trim().is_empty()
                && self.text.len() <= 16000
                && !self.text.contains('\0')
                && self.comments.len() <= 5
                && self.comments.iter().all(|comment| {
                    crate::discovery_hackernews::item_id(&comment.source_url)
                        .is_some_and(|id| Some(id) != root && seen.insert(id))
                        && !comment.text.trim().is_empty()
                        && comment.text.len() <= 2000
                        && !comment.text.contains('\0')
                });
        }
        if claim.platform == "zhihu" {
            let numeric = |s: &str| {
                (1..=20).contains(&s.len())
                    && !s.starts_with('0')
                    && s.bytes().all(|b| b.is_ascii_digit())
            };
            let source_valid = self
                .source_url
                .strip_prefix("https://zhuanlan.zhihu.com/p/")
                .is_some_and(numeric)
                || self
                    .source_url
                    .strip_prefix("https://www.zhihu.com/question/")
                    .and_then(|s| s.split_once("/answer/"))
                    .is_some_and(|(q, a)| numeric(q) && numeric(a));
            let prefix = format!("{}#comment-", self.source_url);
            let mut seen = std::collections::HashSet::new();
            return self.author_id == claim.author_id
                && self.author_id.len() == 32
                && self
                    .author_id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && source_valid
                && self.source_url == claim.root_url
                && claim.ordinal == 1
                && claim.previous_urls.is_empty()
                && self.text.len() <= 16000
                && !self.text.contains('\0')
                && self.comments.len() <= 5
                && self.comments.iter().all(|c| {
                    c.source_url.strip_prefix(&prefix).is_some_and(numeric)
                        && c.text.len() <= 2000
                        && !c.text.contains('\0')
                        && seen.insert(&c.source_url)
                });
        }
        if claim.platform == "bluesky" {
            let mut seen = std::collections::HashSet::new();
            let prefix = format!("{}#comment-", self.source_url);
            return self.author_id == claim.author_id
                && crate::discovery_bluesky::valid_did(&self.author_id)
                && self.source_url == claim.root_url
                && claim.ordinal == 1
                && claim.previous_urls.is_empty()
                && self.source_url.starts_with(&format!(
                    "https://bsky.app/profile/{}/post/",
                    claim.author_id
                ))
                && crate::discovery_bluesky::uri_from_source(&self.source_url).is_some()
                && self.text.len() <= 16000
                && !self.text.contains('\0')
                && (!self.text.trim().is_empty() || !self.comments.is_empty())
                && self.comments.len() <= 5
                && self.comments.iter().all(|c| {
                    c.source_url
                        .strip_prefix(&prefix)
                        .and_then(|uri| {
                            let (did, key) = uri
                                .strip_prefix("at://")?
                                .split_once("/app.bsky.feed.post/")?;
                            let comment_source =
                                format!("https://bsky.app/profile/{did}/post/{key}");
                            crate::discovery_bluesky::uri_from_source(&comment_source)
                                .filter(|v| v == uri && comment_source != self.source_url)
                        })
                        .is_some()
                        && !c.text.trim().is_empty()
                        && c.text.len() <= 2000
                        && !c.text.contains('\0')
                        && seen.insert(&c.source_url)
                });
        }
        if claim.platform == "weibo" {
            let numeric = |id: &str| {
                (1..=20).contains(&id.len())
                    && !id.starts_with('0')
                    && id.bytes().all(|b| b.is_ascii_digit())
            };
            let source = self
                .source_url
                .strip_prefix("https://weibo.com/")
                .and_then(|s| s.split_once('/'));
            let prefix = format!("{}#comment-", self.source_url);
            let mut seen = std::collections::HashSet::new();
            return self.author_id == claim.author_id
                && numeric(&self.author_id)
                && self.source_url == claim.root_url
                && claim.ordinal == 1
                && claim.previous_urls.is_empty()
                && source.is_some_and(|(uid, code)| {
                    uid == claim.author_id
                        && (1..=20).contains(&code.len())
                        && code.bytes().all(|b| b.is_ascii_alphanumeric())
                })
                && self.text.len() <= 16000
                && !self.text.contains('\0')
                && (!self.text.trim().is_empty() || !self.comments.is_empty())
                && self.comments.len() <= 5
                && self.comments.iter().all(|c| {
                    c.source_url.strip_prefix(&prefix).is_some_and(numeric)
                        && !c.text.trim().is_empty()
                        && c.text.len() <= 2000
                        && !c.text.contains('\0')
                        && seen.insert(&c.source_url)
                });
        }
        if claim.platform == "instagram" {
            let code = self
                .source_url
                .strip_prefix("https://www.instagram.com/")
                .and_then(|s| s.strip_suffix('/'))
                .and_then(|s| s.split_once('/'))
                .filter(|(kind, code)| {
                    matches!(*kind, "p" | "reel")
                        && (1..=32).contains(&code.len())
                        && code
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                })
                .map(|(_, code)| code);
            let numeric = |id: &str| {
                (1..=25).contains(&id.len())
                    && !id.starts_with('0')
                    && id.bytes().all(|b| b.is_ascii_digit())
            };
            let mut seen = std::collections::HashSet::new();
            return self.author_id == claim.author_id
                && numeric(&self.author_id)
                && code.is_some()
                && claim.ordinal == 1
                && self.source_url == claim.root_url
                && claim.previous_urls.is_empty()
                && self.text.len() <= 8000
                && !self.text.contains('\0')
                && (!self.text.trim().is_empty() || !self.comments.is_empty())
                && self.comments.len() <= 5
                && self.comments.iter().all(|c| {
                    !c.text.trim().is_empty()
                        && c.text.len() <= 2000
                        && !c.text.contains('\0')
                        && c.source_url
                            .strip_prefix(&format!(
                                "https://www.instagram.com/p/{}/c/",
                                code.unwrap_or("")
                            ))
                            .and_then(|s| s.strip_suffix('/'))
                            .is_some_and(numeric)
                        && seen.insert(&c.source_url)
                });
        }
        if claim.platform == "facebook" {
            let mut seen = std::collections::HashSet::new();
            let prefix = format!("{}?comment_id=", self.source_url);
            return self.author_id == claim.author_id
                && crate::discovery_facebook::valid_reel(&self.source_url)
                && claim.saved_content_urls.contains(&self.source_url)
                && (claim.ordinal != 1 || self.source_url == claim.root_url)
                && !claim.previous_urls.contains(&self.source_url)
                && self.text.len() <= 8000
                && !self.text.contains('\0')
                && (!self.text.trim().is_empty() || !self.comments.is_empty())
                && self.comments.len() <= 5
                && self.comments.iter().all(|c| {
                    !c.text.trim().is_empty()
                        && c.text.len() <= 2000
                        && !c.text.contains('\0')
                        && c.source_url.strip_prefix(&prefix).is_some_and(|id| {
                            (1..=25).contains(&id.len())
                                && !id.starts_with('0')
                                && id.bytes().all(|b| b.is_ascii_digit())
                        })
                        && seen.insert(&c.source_url)
                });
        }
        if claim.platform == "reddit" {
            return self.valid_reddit(claim);
        }
        if claim.platform == "tiktok" {
            return self.valid_tiktok(claim);
        }
        if claim.platform == "bilibili" {
            return self.valid_bilibili(claim);
        }
        if matches!(claim.platform.as_str(), "google" | "yahoo")
            && crate::discovery_douyin::post_id(&claim.root_url).is_some()
        {
            let mut native = claim.clone();
            native.platform = "douyin".into();
            return self.valid_for(&native);
        }
        if matches!(claim.platform.as_str(), "google" | "yahoo")
            && crate::web_source_twitter_target(&claim.root_url).is_some()
        {
            let mut native = claim.clone();
            native.platform = "twitter".into();
            return self.valid_for(&native);
        }
        if matches!(claim.platform.as_str(), "youtube" | "google" | "yahoo") {
            return self.valid_youtube(claim);
        }
        if claim.platform != "twitter" {
            return false;
        }
        let text = |value: &str, limit: usize| {
            !value.trim().is_empty() && value.len() <= limit && !value.contains('\0')
        };
        let numeric = |value: &str| {
            !value.is_empty() && value.len() <= 20 && value.bytes().all(|b| b.is_ascii_digit())
        };
        let url = |value: &str, handle: Option<&str>| {
            value.len() <= 2048
                && url::Url::parse(value).is_ok_and(|u| {
                    let parts: Vec<_> = u.path().split('/').collect();
                    u.scheme() == "https"
                        && u.host_str() == Some("x.com")
                        && u.username().is_empty()
                        && u.password().is_none()
                        && u.port().is_none()
                        && u.query().is_none()
                        && u.fragment().is_none()
                        && parts.len() == 4
                        && !parts[1].is_empty()
                        && parts[1].len() <= 15
                        && parts[1]
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                        && handle.is_none_or(|h| h == parts[1])
                        && parts[2] == "status"
                        && numeric(parts[3])
                })
        };
        let mut comment_urls = std::collections::HashSet::new();
        self.author_id == claim.author_id
            && numeric(&self.author_id)
            && text(&self.text, 8000)
            && url(&self.source_url, Some(&claim.author_handle))
            && (claim.ordinal != 1 || self.source_url == claim.root_url)
            && !claim.previous_urls.contains(&self.source_url)
            && self.comments.len() <= 20
            && self.comments.iter().all(|c| {
                text(&c.text, 2000)
                    && url(&c.source_url, None)
                    && c.source_url != self.source_url
                    && comment_urls.insert(&c.source_url)
            })
    }

    fn valid_reddit(&self, claim: &DiscoveryLanguageClaim) -> bool {
        let base36 = |s: &str| {
            (1..=15).contains(&s.len())
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        };
        let id = self
            .source_url
            .strip_prefix("https://www.reddit.com/comments/")
            .and_then(|s| s.strip_suffix('/'));
        let comment_prefix = format!("{}_/", self.source_url);
        let mut seen = std::collections::HashSet::new();
        self.author_id == claim.author_id
            && self.author_id.strip_prefix("t2_").is_some_and(base36)
            && id.is_some_and(base36)
            && (claim.ordinal != 1 || self.source_url == claim.root_url)
            && !claim.previous_urls.contains(&self.source_url)
            && self.text.len() <= 8000
            && !self.text.contains('\0')
            && (!self.text.trim().is_empty() || !self.comments.is_empty())
            && self.comments.len() <= 20
            && self.comments.iter().all(|c| {
                c.source_url
                    .strip_prefix(&comment_prefix)
                    .and_then(|s| s.strip_suffix('/'))
                    .is_some_and(base36)
                    && !c.text.trim().is_empty()
                    && c.text.len() <= 2000
                    && !c.text.contains('\0')
                    && seen.insert(&c.source_url)
            })
    }

    fn valid_tiktok(&self, claim: &DiscoveryLanguageClaim) -> bool {
        let numeric = |v: &str| {
            !v.is_empty()
                && !v.starts_with('0')
                && v.len() <= 25
                && v.bytes().all(|b| b.is_ascii_digit())
        };
        let prefix = format!("https://www.tiktok.com/@{}/video/", claim.author_handle);
        let mut seen = std::collections::HashSet::new();
        self.author_id == claim.author_id
            && numeric(&self.author_id)
            && !claim.author_handle.is_empty()
            && claim.author_handle.len() <= 24
            && claim
                .author_handle
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
            && self.source_url.strip_prefix(&prefix).is_some_and(numeric)
            && (claim.ordinal != 1 || self.source_url == claim.root_url)
            && !claim.previous_urls.contains(&self.source_url)
            && self.text.len() <= 8000
            && !self.text.contains('\0')
            && (!self.text.trim().is_empty() || !self.comments.is_empty())
            && self.comments.len() <= 20
            && self.comments.iter().all(|c| {
                c.source_url == self.source_url
                    && !c.text.trim().is_empty()
                    && c.text.len() <= 2000
                    && !c.text.contains('\0')
                    && seen.insert(&c.text)
            })
    }

    fn valid_bilibili(&self, claim: &DiscoveryLanguageClaim) -> bool {
        let numeric = |v: &str| {
            !v.starts_with('0')
                && !v.is_empty()
                && v.len() <= 20
                && v.bytes().all(|b| b.is_ascii_digit())
        };
        let Some(id) = self
            .source_url
            .strip_prefix("https://www.bilibili.com/video/")
        else {
            return false;
        };
        let prefix = format!("{}#reply", self.source_url);
        let mut seen = std::collections::HashSet::new();
        id.len() == 12
            && id.starts_with("BV")
            && id.bytes().all(|b| b.is_ascii_alphanumeric())
            && numeric(&self.author_id)
            && self.author_id == claim.author_id
            && (claim.ordinal != 1 || self.source_url == claim.root_url)
            && !claim.previous_urls.contains(&self.source_url)
            && self.text.len() <= 8000
            && !self.text.contains('\0')
            && (!self.text.trim().is_empty() || !self.comments.is_empty())
            && self.comments.len() <= 20
            && self.comments.iter().all(|c| {
                !c.text.trim().is_empty()
                    && c.text.len() <= 2000
                    && !c.text.contains('\0')
                    && (c.source_url == self.source_url
                        || c.source_url.strip_prefix(&prefix).is_some_and(numeric))
                    && seen.insert(if c.source_url == self.source_url {
                        c.text.clone()
                    } else {
                        c.source_url.clone()
                    })
            })
    }

    fn valid_youtube(&self, claim: &DiscoveryLanguageClaim) -> bool {
        let channel = |v: &str| {
            v.len() == 24
                && v.starts_with("UC")
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        };
        let video = |v: &str| {
            v.len() == 11
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        };
        let Some(id) = self
            .source_url
            .strip_prefix("https://www.youtube.com/watch?v=")
            .filter(|id| video(id))
        else {
            return false;
        };
        let prefix = format!("https://www.youtube.com/watch?v={id}&lc=");
        let mut seen = std::collections::HashSet::new();
        self.author_id == claim.author_id
            && channel(&self.author_id)
            && (claim.ordinal != 1 || self.source_url == claim.root_url)
            && !claim.previous_urls.contains(&self.source_url)
            && self.text.len() <= 8000
            && !self.text.contains('\0')
            && (!self.text.trim().is_empty() || !self.comments.is_empty())
            && self.comments.len() <= 20
            && self.comments.iter().all(|c| {
                !c.text.trim().is_empty()
                    && c.text.len() <= 2000
                    && !c.text.contains('\0')
                    && c.source_url.strip_prefix(&prefix).is_some_and(|id| {
                        !id.is_empty()
                            && id.len() <= 200
                            && id.bytes().all(|b| {
                                b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')
                            })
                    })
                    && seen.insert(&c.source_url)
            })
    }
}

impl crate::CreatorRuntime {
    pub async fn claim_discovery_language_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<Option<DiscoveryLanguageClaim>, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner
                .repository
                .claim_discovery_language(&campaign, revision, &hit)
        })
        .await
    }

    pub async fn finish_discovery_language_async(
        &self,
        claim: DiscoveryLanguageClaim,
        sample: Option<DiscoveryLanguageSample>,
        stop: Option<String>,
    ) -> Result<(), crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            if sample
                .as_ref()
                .is_some_and(|value| !value.valid_for(&claim))
                || sample.is_some() == stop.is_some()
            {
                return Err("language_sample_invalid".into());
            }
            owner
                .repository
                .finish_discovery_language(claim, sample, stop)
        })
        .await
    }
}
