//! Score-gated contact discovery. Search results are leads, never identity proof.
use crate::{CreatorRepositoryCallError, CreatorRuntime};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryContactLookup {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub candidate_id: String,
    pub profile_ref: String,
    pub method: String,
    pub native_author_id: String,
    pub author_label: String,
    pub username: Option<String>,
    pub query: String,
    pub call_number: u32,
    pub status: String,
    pub links: Vec<String>,
    pub stop_code: Option<String>,
}

impl CreatorRuntime {
    pub async fn claim_discovery_contact_lookup_async(
        &self,
        input: DiscoveryContactLookup,
    ) -> Result<DiscoveryContactLookup, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner.repository.claim_discovery_contact_lookup(input)
        })
        .await
    }
    pub async fn finish_discovery_contact_lookup_async(
        &self,
        input: DiscoveryContactLookup,
    ) -> Result<DiscoveryContactLookup, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            let mut rejected = input.clone();
            match owner.repository.finish_discovery_contact_lookup(input) {
                Ok(read) => Ok(read),
                Err(original) => {
                    rejected.status = "failed".into();
                    rejected.links.clear();
                    rejected.stop_code = Some("opencli_invalid_output".into());
                    owner
                        .repository
                        .finish_discovery_contact_lookup(rejected)
                        .map_err(|_| original)
                }
            }
        })
        .await
    }
}

pub(crate) fn exact_username(profile_url: &str) -> Option<String> {
    if let Some(name) = profile_url
        .strip_prefix("https://www.pinterest.com/")
        .and_then(|path| path.strip_suffix('/'))
    {
        return crate::discovery_pinterest::valid_username(name).then(|| name.to_owned());
    }
    if let Some(name) = profile_url.strip_prefix("https://news.ycombinator.com/user?id=") {
        return crate::discovery_hackernews::valid_username(name).then(|| name.to_owned());
    }
    let url = url::Url::parse(profile_url).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let path: Vec<_> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let name = match (url.host_str()?, path.as_slice()) {
        ("x.com" | "twitter.com" | "www.twitter.com", [name]) => *name,
        ("www.tiktok.com" | "tiktok.com" | "www.youtube.com" | "youtube.com", [name])
            if name.starts_with('@') =>
        {
            &name[1..]
        }
        ("www.reddit.com" | "reddit.com", ["user" | "u", name]) => *name,
        ("www.instagram.com" | "instagram.com", [name]) if name.len() <= 30 => *name,
        ("www.zhihu.com", ["people", name]) => *name,
        _ => return None,
    };
    (name.len() >= 2
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'))
    .then(|| name.to_string())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod username_tests {
    use super::*;
    #[test]
    fn pinterest_contact_lookup_uses_only_exact_native_username() {
        assert_eq!(
            exact_username("https://www.pinterest.com/native_author/"),
            Some("native_author".into())
        );
        assert_eq!(
            exact_username("https://www.pinterest.com/a/"),
            Some("a".into())
        );
        for source in [
            "https://www.pinterest.com/pin/123/",
            "https://www.pinterest.com/Display%20Name/",
            "https://www.pinterest.com/native_author/?next=other",
            "https://www.pinterest.com/native_author/#other",
            "https://www.pinterest.com/native_author",
            "https://www.pinterest.com/native.author/",
            "https://pinterest.com/native_author/",
        ] {
            assert_eq!(exact_username(source), None);
        }
    }
    #[test]
    fn usernames_require_native_profile_paths() {
        assert_eq!(
            exact_username("https://news.ycombinator.com/user?id=native_author"),
            Some("native_author".into())
        );
        for source in [
            "https://news.ycombinator.com/item?id=100",
            "https://news.ycombinator.com/user?id=native_author&next=other",
            "https://news.ycombinator.com/user?id=Display%20Name",
            "https://news.ycombinator.com/user?id=native_author#fragment",
        ] {
            assert_eq!(exact_username(source), None);
        }
        assert_eq!(
            exact_username("https://x.com/jeffsu"),
            Some("jeffsu".into())
        );
        assert_eq!(
            exact_username("https://www.youtube.com/@JeffSu"),
            Some("JeffSu".into())
        );
        for source in [
            "https://www.youtube.com/channel/UC123",
            "https://x.com/name/status/123",
            "https://example.com/name",
            "https://www.facebook.com/name",
        ] {
            assert_eq!(exact_username(source), None);
        }
    }
}
