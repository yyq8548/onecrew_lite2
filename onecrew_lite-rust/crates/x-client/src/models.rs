use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub enum TimelineKind {
    ForYou,
    Following,
    Bookmarks,
}

impl TimelineKind {
    pub(crate) fn operation(self) -> &'static str {
        match self {
            Self::ForYou => "HomeTimeline",
            Self::Following => "HomeLatestTimeline",
            Self::Bookmarks => "Bookmarks",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum UserListKind {
    Followers,
    Following,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationshipAction {
    Follow,
    Unfollow,
}

impl RelationshipAction {
    pub(crate) fn endpoint(self) -> &'static str {
        match self {
            Self::Follow => "create",
            Self::Unfollow => "destroy",
        }
    }
}

impl UserListKind {
    pub(crate) fn operation(self) -> &'static str {
        match self {
            Self::Followers => "Followers",
            Self::Following => "Following",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweetAction {
    Delete,
    Like,
    Unlike,
    Retweet,
    Unretweet,
    Bookmark,
    Unbookmark,
}

impl TweetAction {
    pub(crate) fn operation(self) -> &'static str {
        match self {
            Self::Delete => "DeleteTweet",
            Self::Like => "FavoriteTweet",
            Self::Unlike => "UnfavoriteTweet",
            Self::Retweet => "CreateRetweet",
            Self::Unretweet => "DeleteRetweet",
            Self::Bookmark => "CreateBookmark",
            Self::Unbookmark => "DeleteBookmark",
        }
    }

    pub(crate) fn needs_dark_request(self) -> bool {
        matches!(
            self,
            Self::Delete | Self::Unlike | Self::Retweet | Self::Unretweet
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Author {
    pub id: String,
    pub name: String,
    pub username: String,
    pub profile_image_url: String,
    pub verified: bool,
    #[serde(default)]
    pub blue_verified: bool,
    #[serde(default)]
    pub viewer_follows: Option<bool>,
    #[serde(default)]
    pub viewer_followed_by: Option<bool>,
    #[serde(default)]
    pub can_dm: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Metrics {
    pub likes: u64,
    pub retweets: u64,
    pub replies: u64,
    pub quotes: u64,
    pub views: u64,
    pub bookmarks: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Media {
    pub kind: String,
    pub url: String,
    pub width: Option<u64>,
    pub height: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tweet {
    pub id: String,
    pub text: String,
    pub author: Author,
    pub metrics: Metrics,
    pub created_at: String,
    pub language: String,
    pub media: Vec<Media>,
    pub urls: Vec<String>,
    pub promoted: bool,
    pub article: Option<Article>,
    #[serde(default)]
    pub in_reply_to_status_id: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Article {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub name: String,
    pub username: String,
    pub bio: String,
    pub location: String,
    #[serde(default)]
    pub profile_links: Vec<ProfileLink>,
    pub followers: u64,
    pub following: u64,
    pub posts: u64,
    pub likes: u64,
    pub verified: bool,
    #[serde(default)]
    pub blue_verified: bool,
    pub profile_image_url: String,
    #[serde(default)]
    pub viewer_follows: Option<bool>,
    #[serde(default)]
    pub viewer_followed_by: Option<bool>,
    #[serde(default)]
    pub can_dm: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileLink {
    pub display_text: String,
    pub target_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelinePage {
    pub items: Vec<Tweet>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserPage {
    pub items: Vec<User>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookmarkFolder {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookmarkFolderPage {
    pub items: Vec<BookmarkFolder>,
    pub next_cursor: Option<String>,
}
