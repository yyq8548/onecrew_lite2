use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use onecrew_ig_transport::{IgError, IgSession, IgTransport};
use rand::Rng;
use serde_json::{Value, json};
use sha2::Sha256;
use uuid::Uuid;

use crate::models::{IgThread, IgUser};
use crate::parser;

/// Smallest page size a user search accepts.
pub const USER_SEARCH_MIN_COUNT: u16 = 1;
/// Largest page size a user search accepts.
pub const USER_SEARCH_MAX_COUNT: u16 = 100;
/// Page size of a user search that names none.
pub const USER_SEARCH_DEFAULT_COUNT: u16 = 20;

/// Whether `query` names something to search for. Only a blank query is
/// refused; an accepted query is sent as given, untrimmed.
pub fn accepts_user_search_query(query: &str) -> bool {
    !query.trim().is_empty()
}

/// Whether `count` is inside the accepted user-search page-size range.
pub fn accepts_user_search_count(count: u16) -> bool {
    (USER_SEARCH_MIN_COUNT..=USER_SEARCH_MAX_COUNT).contains(&count)
}

/// Instagram private-API reads plus the guarded write actions below, over
/// an [`IgTransport`].
#[derive(Clone)]
pub struct IgClient {
    transport: IgTransport,
}

impl IgClient {
    pub fn new(transport: IgTransport) -> Self {
        Self { transport }
    }

    /// Snapshot the session (refreshed with any `mid`/www-claim) for persistence.
    pub fn session_snapshot(&self) -> IgSession {
        self.transport.session_snapshot()
    }

    /// The signed-in account (`accounts/current_user/?edit=true`, matching
    /// instagrapi's `account_info`).
    pub async fn account_info(&self) -> Result<IgUser, IgError> {
        let payload = self
            .transport
            .get("accounts/current_user/", &[("edit", "true")])
            .await?;
        payload
            .get("user")
            .map(parser::user)
            .ok_or_else(|| IgError::Api("account info response missing user".into()))
    }

    pub async fn user_by_username(&self, username: &str) -> Result<IgUser, IgError> {
        let username = username.trim_start_matches('@');
        let payload = self
            .transport
            .get(&format!("users/{username}/usernameinfo/"), &[])
            .await?;
        payload
            .get("user")
            .map(parser::user)
            .ok_or_else(|| IgError::Api(format!("user not found: {username}")))
    }

    pub async fn search_users(&self, query: &str, count: u16) -> Result<Vec<IgUser>, IgError> {
        if !accepts_user_search_query(query) || !accepts_user_search_count(count) {
            return Err(IgError::Api("invalid Instagram user search input".into()));
        }
        let count = count.to_string();
        let payload = self
            .transport
            .get(
                "users/search/",
                &[
                    ("search_surface", "user_search_page"),
                    ("timezone_offset", "0"),
                    ("count", count.as_str()),
                    ("q", query),
                ],
            )
            .await?;
        Ok(parser::users(&payload))
    }

    pub async fn inbox(&self, amount: u32) -> Result<Vec<IgThread>, IgError> {
        let limit = amount.clamp(1, 100).to_string();
        // Params mirror instagrapi's `direct_threads_chunk`, incl. the request
        // tracking params + include_old_mrs / no_pending_badge / push_disabled.
        let tracking_id = Uuid::new_v4().to_string();
        let payload = self
            .transport
            .get(
                "direct_v2/inbox/",
                &[
                    ("eb_device_id", "0"),
                    ("igd_request_log_tracking_id", tracking_id.as_str()),
                    ("visual_message_return_type", "unseen"),
                    ("thread_message_limit", "10"),
                    ("persistentBadging", "true"),
                    ("limit", limit.as_str()),
                    ("is_prefetching", "false"),
                    ("fetch_reason", "initial_snapshot"),
                    ("include_old_mrs", "false"),
                    ("no_pending_badge", "true"),
                    ("push_disabled", "true"),
                ],
            )
            .await?;
        Ok(payload
            .pointer("/inbox/threads")
            .and_then(Value::as_array)
            .map(|threads| threads.iter().map(parser::thread).collect())
            .unwrap_or_default())
    }

    pub async fn thread(&self, thread_id: &str, amount: u32) -> Result<IgThread, IgError> {
        let limit = amount.clamp(1, 100).to_string();
        let payload = self
            .transport
            .get(
                &format!("direct_v2/threads/{thread_id}/"),
                &[
                    ("visual_message_return_type", "unseen"),
                    ("direction", "older"),
                    ("seq_id", "40065"),
                    ("limit", limit.as_str()),
                ],
            )
            .await?;
        payload
            .get("thread")
            .map(parser::thread)
            .ok_or_else(|| IgError::Api(format!("thread not found: {thread_id}")))
    }

    /// A page of a user's posts (`feed/user/{pk}/`). Pass `next_max_id` from the
    /// previous page as `max_id` to page further back.
    pub async fn user_medias(
        &self,
        user_id: &str,
        amount: u16,
        max_id: Option<&str>,
    ) -> Result<crate::models::IgMediaPage, IgError> {
        let count = amount.clamp(1, 50).to_string();
        let rank_token = self.rank_token();
        let mut params: Vec<(&str, &str)> = vec![
            ("count", &count),
            ("rank_token", &rank_token),
            ("ranked_content", "true"),
        ];
        if let Some(max_id) = max_id {
            params.push(("max_id", max_id));
        }
        let payload = self
            .transport
            .get(&format!("feed/user/{user_id}/"), &params)
            .await?;
        Ok(parser::media_page(&payload))
    }

    /// A page of a user's followers (`friendships/{pk}/followers/`).
    pub async fn followers(
        &self,
        user_id: &str,
        amount: u16,
        max_id: Option<&str>,
    ) -> Result<crate::models::IgUserPage, IgError> {
        self.friendship_page("followers", user_id, amount, max_id)
            .await
    }

    /// A page of the accounts a user follows (`friendships/{pk}/following/`).
    pub async fn following(
        &self,
        user_id: &str,
        amount: u16,
        max_id: Option<&str>,
    ) -> Result<crate::models::IgUserPage, IgError> {
        self.friendship_page("following", user_id, amount, max_id)
            .await
    }

    /// Read the current operator-to-target relationship state.
    pub async fn relationship_status(
        &self,
        user_id: &str,
    ) -> Result<crate::models::IgRelationshipStatus, IgError> {
        let payload = self
            .transport
            .get(&format!("friendships/show/{user_id}/"), &[])
            .await?;
        parse_relationship_status(&payload)
    }

    async fn friendship_page(
        &self,
        edge: &str,
        user_id: &str,
        amount: u16,
        max_id: Option<&str>,
    ) -> Result<crate::models::IgUserPage, IgError> {
        let count = amount.clamp(1, 100).to_string();
        let rank_token = self.rank_token();
        let mut params: Vec<(&str, &str)> = vec![
            ("count", &count),
            ("rank_token", &rank_token),
            ("search_surface", "follow_list_page"),
            ("query", ""),
            ("enable_groups", "true"),
        ];
        if let Some(max_id) = max_id {
            params.push(("max_id", max_id));
        }
        let payload = self
            .transport
            .get(&format!("friendships/{user_id}/{edge}/"), &params)
            .await?;
        Ok(parser::user_page(&payload))
    }

    /// A page of comments on a media (`media/{id}/comments/`).
    pub async fn media_comments(
        &self,
        media_id: &str,
        max_id: Option<&str>,
    ) -> Result<crate::models::IgCommentPage, IgError> {
        let params: Vec<(&str, &str)> = match max_id {
            Some(max_id) => vec![("max_id", max_id)],
            None => vec![],
        };
        let payload = self
            .transport
            .get(&format!("media/{media_id}/comments/"), &params)
            .await?;
        Ok(parser::comment_page(&payload))
    }

    /// instagrapi's `rank_token`: `{self_user_id}_{device_uuid}`.
    fn rank_token(&self) -> String {
        let session = self.transport.session_snapshot();
        format!("{}_{}", session.user_id(), session.uuids.uuid)
    }

    // ---- write actions ----

    /// Follow a user by numeric pk. Returns true if we now follow or requested.
    pub async fn follow(&self, user_id: &str) -> Result<bool, IgError> {
        let session = self.transport.session_snapshot();
        let data = json!({
            "_uuid": session.uuids.uuid,
            "device_id": session.uuids.android_device_id,
            "radio_type": "wifi-none",
            "user_id": user_id,
            "_uid": session.user_id(),
            "include_follow_friction_check": "1",
            "container_module": "profile",
        });
        let result = self
            .transport
            .post_signed(&format!("friendships/create/{user_id}/"), &data)
            .await?;
        Ok(friendship_bool(&result, "following")? || friendship_bool(&result, "outgoing_request")?)
    }

    /// Unfollow a user by numeric pk. Returns true once we no longer follow.
    pub async fn unfollow(&self, user_id: &str) -> Result<bool, IgError> {
        let session = self.transport.session_snapshot();
        let data = json!({
            "_uuid": session.uuids.uuid,
            "device_id": session.uuids.android_device_id,
            "radio_type": "wifi-none",
            "user_id": user_id,
            "_uid": session.user_id(),
            "container_module": "profile",
        });
        let result = self
            .transport
            .post_signed(&format!("friendships/destroy/{user_id}/"), &data)
            .await?;
        Ok(!friendship_bool(&result, "following")?)
    }

    /// Like (or, with `revert`, unlike) a media by id. Returns true on success.
    pub async fn like(&self, media_id: &str, revert: bool) -> Result<bool, IgError> {
        let session = self.transport.session_snapshot();
        let feed_position = rand::thread_rng().gen_range(0..=6).to_string();
        let data = json!({
            "_uuid": session.uuids.uuid,
            "device_id": session.uuids.android_device_id,
            "radio_type": "wifi-none",
            "inventory_source": "media_or_ad",
            "media_id": media_id,
            "_uid": session.user_id(),
            "delivery_class": "organic",
            "tap_source": "button",
            "is_2m_enabled": "false",
            "is_from_swipe": "false",
            "is_carousel_bumped_post": "false",
            "floating_context_items": "[]",
            "media_pct_watched": "0",
            "container_module": "feed_timeline",
            "feed_position": feed_position,
        });
        let name = if revert { "unlike" } else { "like" };
        let result = self
            .transport
            .post_signed(&format!("media/{media_id}/{name}/"), &data)
            .await?;
        Ok(result.get("status").and_then(Value::as_str) == Some("ok"))
    }

    /// Comment on a media (optionally replying to a comment id). Returns the new
    /// comment id.
    pub async fn comment(
        &self,
        media_id: &str,
        text: &str,
        replied_to: Option<&str>,
    ) -> Result<String, IgError> {
        let session = self.transport.session_snapshot();
        let feed_position = rand::thread_rng().gen_range(0..=6).to_string();
        let idempotence = Uuid::new_v4().to_string();
        let creation_key = idempotence.clone();
        let mut data = json!({
            "_uuid": session.uuids.uuid,
            "device_id": session.uuids.android_device_id,
            "radio_type": "wifi-none",
            "delivery_class": "organic",
            "feed_position": feed_position,
            "container_module": "feed_timeline",
            "media_id": media_id,
            "_uid": session.user_id(),
            "tap_source": "button",
            "is_2m_enabled": "false",
            "is_carousel_bumped_post": "false",
            "is_from_swipe": "false",
            "floating_context_items": "[]",
            "media_pct_watched": "0",
            "user_breadcrumb": user_breadcrumb(text.chars().count()),
            "idempotence_token": idempotence,
            "comment_creation_key": creation_key,
            "include_e2ee_mentioned_user_list": "true",
            "include_media_code": "true",
            "comment_text": text,
        });
        if let Some(comment_id) = replied_to {
            data["replied_to_comment_id"] = Value::String(comment_id.to_string());
        }
        let result = self
            .transport
            .post_signed(&format!("media/{media_id}/comment/"), &data)
            .await?;
        result
            .get("comment")
            .map(|comment| id_string(comment, "pk"))
            .filter(|id| !id.is_empty())
            .ok_or_else(|| IgError::Api("comment response missing id".into()))
    }

    /// Delete one or more of your own comments (`comment_ids` = comma-separated).
    pub async fn delete_comments(
        &self,
        media_id: &str,
        comment_ids: &str,
    ) -> Result<bool, IgError> {
        let session = self.transport.session_snapshot();
        // instagrapi bulk_delete data + with_action_data (no _uid; its own container).
        let data = json!({
            "_uuid": session.uuids.uuid,
            "device_id": session.uuids.android_device_id,
            "radio_type": "wifi-none",
            "comment_ids_to_delete": comment_ids,
            "container_module": "self_comments_v2_newsfeed_you",
        });
        let result = self
            .transport
            .post_signed(&format!("media/{media_id}/comment/bulk_delete/"), &data)
            .await?;
        Ok(result.get("status").and_then(Value::as_str) == Some("ok"))
    }

    /// Send a direct message to a user pk (creates or reuses the 1:1 thread).
    /// Returns the new message item id. Uses the plain-form broadcast endpoint
    /// (instagrapi `with_signature=False`), auto-switching to the link variant
    /// when the text contains a URL.
    pub async fn send_dm(&self, user_id: &str, text: &str) -> Result<String, IgError> {
        let session = self.transport.session_snapshot();
        let token = mutation_token();
        let recipients = format!("[[{user_id}]]");
        let uuid = session.uuids.uuid.clone();
        let device_id = session.uuids.android_device_id.clone();
        let is_link = text.contains("http");
        let method = if is_link { "link" } else { "text" };
        let link_urls = if is_link {
            serde_json::to_string(&extract_urls(text)).unwrap_or_else(|_| "[]".to_string())
        } else {
            String::new()
        };
        // Field set mirrors instagrapi's `direct_send` (direct.py) verbatim,
        // including the newer dual-send / transport flags and the current
        // 6-segment nav chain.
        let mut fields: Vec<(&str, &str)> = vec![
            ("action", "send_item"),
            ("is_x_transport_forward", "false"),
            ("send_silently", "false"),
            ("is_shh_mode", "0"),
            ("send_attribution", "message_button"),
            ("client_context", &token),
            ("device_id", &device_id),
            ("mutation_token", &token),
            ("_uuid", &uuid),
            ("btt_dual_send", "false"),
            (
                "nav_chain",
                "1qT:feed_timeline:1,1qT:feed_timeline:2,1qT:feed_timeline:3,\
                 7Az:direct_inbox:4,7Az:direct_inbox:5,5rG:direct_thread:7",
            ),
            ("is_ae_dual_send", "false"),
            ("offline_threading_id", &token),
        ];
        if is_link {
            fields.push(("link_text", text));
            fields.push(("link_urls", &link_urls));
        } else {
            fields.push(("text", text));
        }
        fields.push(("recipient_users", &recipients));
        let result = self
            .transport
            .post_form(&format!("direct_v2/threads/broadcast/{method}/"), &fields)
            .await?;
        direct_message_receipt(&result)
    }
}

fn direct_message_receipt(result: &Value) -> Result<String, IgError> {
    result
        .get("payload")
        .unwrap_or(result)
        .get("item_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| IgError::Api("direct send response missing item_id".into()))
}

fn friendship_bool(result: &Value, key: &str) -> Result<bool, IgError> {
    result
        .pointer(&format!("/friendship_status/{key}"))
        .and_then(Value::as_bool)
        .ok_or_else(|| IgError::Api("relationship response is missing a required flag".into()))
}

fn parse_relationship_status(
    payload: &Value,
) -> Result<crate::models::IgRelationshipStatus, IgError> {
    let status = payload.get("friendship_status").unwrap_or(payload);
    let flag = |key| {
        status
            .get(key)
            .and_then(Value::as_bool)
            .ok_or_else(|| IgError::Api("relationship response is missing a required flag".into()))
    };
    Ok(crate::models::IgRelationshipStatus {
        following: flag("following")?,
        outgoing_request: flag("outgoing_request")?,
        followed_by: flag("followed_by")?,
        blocking: flag("blocking")?,
    })
}

/// instagrapi's mutation token: a random 19-digit int used as client_context /
/// offline_threading_id when sending DMs.
fn mutation_token() -> String {
    rand::thread_rng()
        .gen_range(6_800_011_111_111_111_111u64..=6_800_099_999_999_999_999u64)
        .to_string()
}

fn extract_urls(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|word| word.starts_with("http://") || word.starts_with("https://"))
        .map(str::to_string)
        .collect()
}

/// Read an id field that may be a JSON number or string.
fn id_string(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// instagrapi's `gen_user_breadcrumb`: an HMAC-SHA256 anti-automation token the
/// comment endpoint expects, keyed on the (opaque) typing telemetry.
fn user_breadcrumb(size: usize) -> String {
    let mut rng = rand::thread_rng();
    let dt = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let elapsed = rng.gen_range(500..1500) + size as u64 * rng.gen_range(500..1500);
    let count = (size as f64 / rng.gen_range(3..=5) as f64).max(1.0);
    let data = format!("{size} {elapsed} {count} {dt}");
    let mut mac =
        Hmac::<Sha256>::new_from_slice(b"iN4$aGr0m").expect("hmac accepts any key length");
    mac.update(data.as_bytes());
    let signature = STANDARD.encode(mac.finalize().into_bytes());
    format!("{signature}\n{}\n", STANDARD.encode(data.as_bytes()))
}

#[cfg(test)]
mod relationship_readback_tests {
    use super::*;

    #[test]
    fn native_direct_message_receipts_never_accept_empty_or_missing_ids() {
        for fixture in [
            json!({}),
            json!({"payload":{}}),
            json!({"payload":{"item_id":""}}),
            json!({"payload":{"item_id":"   "}}),
        ] {
            assert!(direct_message_receipt(&fixture).is_err());
        }
        assert_eq!(
            direct_message_receipt(&json!({"payload":{"item_id":"123456"}})).unwrap(),
            "123456"
        );
    }

    #[test]
    fn native_readback_preserves_pending_requests_and_requires_explicit_flags() {
        let fixture = json!({"friendship_status": {
            "following": false, "outgoing_request": true,
            "followed_by": false, "blocking": false
        }});
        let status = parse_relationship_status(&fixture).unwrap();
        assert!(!status.following);
        assert!(status.outgoing_request);
        assert!(!status.followed_by);
        for malformed in [
            json!({}),
            json!({"status":"ok"}),
            json!({"following":false}),
        ] {
            assert!(parse_relationship_status(&malformed).is_err());
            assert!(friendship_bool(&malformed, "following").is_err());
        }
        assert!(!friendship_bool(&fixture, "following").unwrap());
    }
}

#[cfg(test)]
mod user_search_tests {
    use super::{
        USER_SEARCH_DEFAULT_COUNT, USER_SEARCH_MAX_COUNT, USER_SEARCH_MIN_COUNT,
        accepts_user_search_count, accepts_user_search_query,
    };

    #[test]
    fn user_search_accepts_page_sizes_from_1_through_100_and_defaults_to_20() {
        assert_eq!(
            (
                USER_SEARCH_MIN_COUNT,
                USER_SEARCH_MAX_COUNT,
                USER_SEARCH_DEFAULT_COUNT
            ),
            (1, 100, 20)
        );
        for (count, accepted) in [
            (0, false),
            (1, true),
            (20, true),
            (100, true),
            (101, false),
            (u16::MAX, false),
        ] {
            assert_eq!(accepts_user_search_count(count), accepted, "{count}");
        }
    }

    #[test]
    fn a_user_search_query_is_refused_only_when_blank() {
        for (query, accepted) in [
            ("", false),
            (" \t\n", false),
            ("rust", true),
            ("  rust creators  ", true),
        ] {
            assert_eq!(accepts_user_search_query(query), accepted, "{query:?}");
        }
    }
}
