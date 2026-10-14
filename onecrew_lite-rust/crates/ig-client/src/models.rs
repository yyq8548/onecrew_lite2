use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct IgUser {
    pub id: String,
    pub username: String,
    pub full_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_private: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_pic_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follower_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub following_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biography: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_url: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bio_links: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_street: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IgMessage {
    pub item_id: String,
    pub item_type: String,
    pub user_id: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IgThread {
    pub thread_id: String,
    pub title: String,
    pub users: Vec<IgUser>,
    pub messages: Vec<IgMessage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IgMedia {
    pub id: String,
    pub code: String,
    /// 1 = photo, 2 = video/reel, 8 = album/carousel.
    pub media_type: i64,
    pub caption: String,
    pub like_count: i64,
    pub comment_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IgComment {
    pub id: String,
    pub text: String,
    pub username: String,
    pub like_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

/// A page of users (followers/following) with the cursor for the next page.
#[derive(Debug, Clone, Serialize)]
pub struct IgUserPage {
    pub users: Vec<IgUser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_max_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IgRelationshipStatus {
    pub following: bool,
    pub outgoing_request: bool,
    pub followed_by: bool,
    pub blocking: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IgMediaPage {
    pub items: Vec<IgMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_max_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IgCommentPage {
    pub comments: Vec<IgComment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_max_id: Option<String>,
}
