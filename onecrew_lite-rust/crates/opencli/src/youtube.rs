use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::OpenCliError;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct YoutubeSearchHit {
    #[serde(default)]
    pub rank: Option<u32>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub views: String,
    #[serde(default)]
    pub duration: String,
    #[serde(default)]
    pub published: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct YoutubeChannel {
    pub channel_id: String,
    pub name: String,
    pub handle: String,
    pub description: String,
    pub subscribers_raw: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct YoutubeVideo {
    pub video_id: String,
    pub channel_id: String,
    pub title: String,
    pub channel: String,
    pub description: String,
    pub subscribers_raw: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct YoutubeTranscriptLine {
    pub timestamp: String,
    #[serde(default)]
    pub speaker: String,
    pub text: String,
}

#[derive(Deserialize)]
struct FieldValue {
    field: String,
    value: String,
}

pub(super) fn validate_channel(channel: &str) -> Result<(), OpenCliError> {
    let valid_id = channel.starts_with("UC")
        && channel.len() >= 10
        && channel.len() <= 64
        && channel
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    let valid_handle = channel.starts_with('@')
        && (2..=101).contains(&channel.len())
        && channel[1..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid_id || valid_handle {
        Ok(())
    } else {
        Err(OpenCliError::InvalidInput("channel"))
    }
}

pub(super) fn video_id(video: &str) -> Result<String, OpenCliError> {
    if valid_video_id(video) {
        return Ok(video.to_owned());
    }
    let url = Url::parse(video).map_err(|_| OpenCliError::InvalidInput("video"))?;
    if !youtube_url(video) {
        return Err(OpenCliError::InvalidInput("video"));
    }
    if url.path() == "/watch" {
        let mut ids = url.query_pairs().filter(|(key, _)| key == "v");
        if let Some((_, id)) = ids.next()
            && valid_video_id(&id)
            && ids.next().is_none()
        {
            return Ok(id.into_owned());
        }
    }
    if let Some(segments) = url.path_segments() {
        let segments: Vec<_> = segments.collect();
        if let ["shorts" | "live", id] = segments.as_slice()
            && valid_video_id(id)
        {
            return Ok((*id).to_owned());
        }
    }
    Err(OpenCliError::InvalidInput("video"))
}

fn valid_video_id(value: &str) -> bool {
    value.len() == 11
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(super) fn parse_search(json: &str) -> Result<Vec<YoutubeSearchHit>, OpenCliError> {
    let hits: Vec<YoutubeSearchHit> =
        serde_json::from_str(json).map_err(|_| OpenCliError::InvalidOutput)?;
    if hits.len() > 50
        || hits.iter().any(|hit| {
            !youtube_url(&hit.url)
                || hit.title.trim().is_empty()
                || hit.title.chars().count() > 500
                || hit.title.chars().any(char::is_control)
                || hit.channel.chars().count() > 200
                || hit.channel.chars().any(char::is_control)
        })
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(hits)
}

pub(super) fn parse_channel(json: &str) -> Result<YoutubeChannel, OpenCliError> {
    let fields = parse_fields(json)?;
    let channel_id = required(&fields, "channelId")?;
    validate_channel(&channel_id).map_err(|_| OpenCliError::InvalidOutput)?;
    let url = required(&fields, "url")?;
    if !youtube_url(&url) {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(YoutubeChannel {
        channel_id,
        name: required(&fields, "name")?,
        handle: optional(&fields, "handle"),
        description: optional(&fields, "description"),
        subscribers_raw: optional(&fields, "subscribers"),
        url,
    })
}

pub(super) fn parse_video(json: &str, requested_id: &str) -> Result<YoutubeVideo, OpenCliError> {
    let fields = parse_fields(json)?;
    let video_id = required(&fields, "videoId")?;
    if !valid_video_id(&video_id) || video_id != requested_id {
        return Err(OpenCliError::InvalidOutput);
    }
    let channel_id = required(&fields, "channelId")?;
    validate_channel(&channel_id).map_err(|_| OpenCliError::InvalidOutput)?;
    if !channel_id.starts_with("UC") {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(YoutubeVideo {
        url: format!("https://www.youtube.com/watch?v={video_id}"),
        video_id,
        channel_id,
        title: required(&fields, "title")?,
        channel: required(&fields, "channel")?,
        description: optional(&fields, "description"),
        subscribers_raw: optional(&fields, "subscribers"),
    })
}

pub(super) fn parse_transcript(json: &str) -> Result<Vec<YoutubeTranscriptLine>, OpenCliError> {
    let lines: Vec<YoutubeTranscriptLine> =
        serde_json::from_str(json).map_err(|_| OpenCliError::InvalidOutput)?;
    if lines.is_empty()
        || lines.len() > 2_000
        || lines.iter().any(|line| {
            line.timestamp.is_empty()
                || line.timestamp.len() > 24
                || !line
                    .timestamp
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b':')
                || line.speaker.chars().count() > 120
                || line.text.trim().is_empty()
                || line.text.chars().count() > 4_000
        })
        || lines.iter().map(|line| line.text.len()).sum::<usize>() > 250_000
    {
        return Err(OpenCliError::InvalidOutput);
    }
    Ok(lines)
}

fn parse_fields(json: &str) -> Result<BTreeMap<String, String>, OpenCliError> {
    let rows: Vec<FieldValue> =
        serde_json::from_str(json).map_err(|_| OpenCliError::InvalidOutput)?;
    if rows.len() > 100 {
        return Err(OpenCliError::InvalidOutput);
    }
    let mut fields = BTreeMap::new();
    for row in rows {
        if fields.insert(row.field, row.value).is_some() {
            return Err(OpenCliError::InvalidOutput);
        }
    }
    Ok(fields)
}

fn required(fields: &BTreeMap<String, String>, key: &str) -> Result<String, OpenCliError> {
    fields
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(OpenCliError::InvalidOutput)
}

fn optional(fields: &BTreeMap<String, String>, key: &str) -> String {
    fields.get(key).cloned().unwrap_or_default()
}

fn youtube_url(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && matches!(url.host_str(), Some("www.youtube.com" | "youtube.com"))
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_value_rows_keep_youtube_identity_and_evidence() {
        let channel = parse_channel(
            r#"[{"field":"name","value":"AI Maker"},{"field":"channelId","value":"UC1234567890"},{"field":"subscribers","value":"12.4K subscribers"},{"field":"url","value":"https://www.youtube.com/channel/UC1234567890"}]"#,
        )
        .expect("channel");
        assert_eq!(channel.subscribers_raw, "12.4K subscribers");
        assert_eq!(channel.channel_id, "UC1234567890");
        assert_eq!(channel.description, "");
    }

    #[test]
    fn non_youtube_video_target_is_rejected() {
        assert_eq!(
            video_id("https://example.com/watch?v=dQw4w9WgXcQ"),
            Err(OpenCliError::InvalidInput("video"))
        );
        assert_eq!(video_id("dQw4w9WgXcQ"), Ok("dQw4w9WgXcQ".into()));
    }

    #[test]
    fn youtube_url_with_non_id_query_is_rejected() {
        assert_eq!(
            video_id("https://www.youtube.com/watch?v=%20%20%20%20%20%20%20%20%20%20%20"),
            Err(OpenCliError::InvalidInput("video"))
        );
    }

    #[test]
    fn transcript_is_bounded_and_keeps_source_timestamps() {
        let lines =
            parse_transcript(r#"[{"timestamp":"0:10","speaker":"","text":"I open the AI tool."}]"#)
                .unwrap();
        assert_eq!(lines[0].timestamp, "0:10");
        assert!(parse_transcript(r#"[{"timestamp":"soon","text":"demo"}]"#).is_err());
    }

    #[test]
    fn video_targets_resolve_to_one_id_without_credentials_or_navigation_suffixes() {
        for target in [
            "dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=30s",
            "https://youtube.com/shorts/dQw4w9WgXcQ",
            "https://youtube.com/live/dQw4w9WgXcQ",
        ] {
            assert_eq!(video_id(target).unwrap(), "dQw4w9WgXcQ");
        }
        for target in [
            "https://user@youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com:444/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=dQw4w9WgXcQ&v=LR04bU_yV5k",
            "https://youtube.com/shorts/dQw4w9WgXcQ/extra",
            "https://youtube.com/live/dQw4w9WgXcQ/extra",
        ] {
            assert!(video_id(target).is_err(), "{target}");
        }
    }

    #[test]
    fn video_metadata_must_match_requested_identity() {
        let rows = r#"[{"field":"videoId","value":"dQw4w9WgXcQ"},{"field":"channelId","value":"UC1234567890"},{"field":"title","value":"Example"},{"field":"channel","value":"Creator"}]"#;
        let video = parse_video(rows, "dQw4w9WgXcQ").unwrap();
        assert_eq!(video.url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(
            parse_video(rows, "LR04bU_yV5k"),
            Err(OpenCliError::InvalidOutput)
        );
        assert_eq!(
            parse_video(
                &rows.replace("UC1234567890", "@mutable_handle"),
                "dQw4w9WgXcQ"
            ),
            Err(OpenCliError::InvalidOutput)
        );
        assert_eq!(
            parse_video(
                &rows.replace("dQw4w9WgXcQ", "https://youtube.com/watch?v=dQw4w9WgXcQ"),
                "dQw4w9WgXcQ"
            ),
            Err(OpenCliError::InvalidOutput)
        );
    }

    #[test]
    fn duplicate_metadata_fields_are_ambiguous_not_last_value_wins() {
        assert_eq!(
            parse_fields(
                r#"[{"field":"videoId","value":"dQw4w9WgXcQ"},{"field":"videoId","value":"LR04bU_yV5k"}]"#
            ),
            Err(OpenCliError::InvalidOutput)
        );
    }
}
