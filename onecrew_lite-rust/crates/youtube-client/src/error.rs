use onecrew_youtube_transport::YoutubeTransportError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum YoutubeClientError {
    #[error(transparent)]
    Transport(#[from] YoutubeTransportError),
    #[error("YouTube search query must not be empty")]
    EmptyQuery,
    #[error("YouTube search count must be between 1 and 50")]
    InvalidSearchCount,
    #[error("YouTube search publication bound must be YYYY-MM-DDTHH:MM:SSZ")]
    InvalidPublishedAfter,
    #[error("YouTube channel ID must be a stable 24-character UC identifier")]
    InvalidChannelId,
    #[error("YouTube channel was not found")]
    ChannelNotFound,
    #[error("YouTube page parser failed: {0}")]
    Parse(String),
}
