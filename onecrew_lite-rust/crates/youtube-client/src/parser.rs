use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::YoutubeClientError;

const INITIAL_DATA_MARKER: &str = "var ytInitialData = ";

#[derive(Debug, Default)]
pub(crate) struct AboutPage {
    pub description: String,
    pub links: Vec<String>,
    pub hidden_business_email: bool,
}

pub(crate) fn parse_about_html(body: &str) -> Result<AboutPage, YoutubeClientError> {
    let Some(index) = body.find(INITIAL_DATA_MARKER) else {
        return Err(YoutubeClientError::Parse(
            "About section was not returned".into(),
        ));
    };
    let tail = &body[index + INITIAL_DATA_MARKER.len()..];
    let mut deserializer = serde_json::Deserializer::from_str(tail);
    let root = Value::deserialize(&mut deserializer)
        .map_err(|error| YoutubeClientError::Parse(error.to_string()))?;
    let Some(model) = find_object(&root, "aboutChannelViewModel") else {
        return Err(YoutubeClientError::Parse(
            "About section was not returned".into(),
        ));
    };
    let description = model
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let hidden_business_email = match model.get("signInForBusinessEmail") {
        Some(Value::Bool(value)) => *value,
        Some(Value::Null) | None => false,
        Some(Value::String(value)) => !value.trim().is_empty(),
        Some(Value::Object(value)) => !value.is_empty(),
        Some(_) => true,
    };
    let mut links = Vec::new();
    if let Some(Value::Array(items)) = model.get("links") {
        for item in items {
            if let Some(target) = channel_link_target(item)
                && is_public_link_target(&target)
            {
                push_unique(&mut links, target);
            }
        }
    }
    Ok(AboutPage {
        description,
        links,
        hidden_business_email,
    })
}

fn is_public_link_target(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| {
        // Mailto is a provider-published destination, not a classified contact.
        // Creator validates its recipient and ignores message/query fields.
        url.scheme() == "mailto"
            || (matches!(url.scheme(), "http" | "https")
            // A registrable name has a dot. Without this, any bare word that
            // had a scheme prefixed onto it parses as a host, and a display
            // label became the contact route `https://linustech/`.
            && url.host_str().is_some_and(|host| host.contains('.'))
            && url.username().is_empty()
            && url.password().is_none())
    })
}

fn find_object<'a>(value: &'a Value, key: &str) -> Option<&'a serde_json::Map<String, Value>> {
    match value {
        Value::Object(object) => {
            if let Some(Value::Object(found)) = object.get(key) {
                return Some(found);
            }
            object.values().find_map(|value| find_object(value, key))
        }
        Value::Array(values) => values.iter().find_map(|value| find_object(value, key)),
        _ => None,
    }
}

/// The destination of one curated channel link.
///
/// Every field is addressed by name. A generic "first URL-shaped string
/// anywhere in here" walk cannot work: `serde_json::Map` orders keys
/// alphabetically, so `favicon` is visited before `link`, and a favicon's own
/// `?q=tbn:...` parameter then unwraps into something that is not a URL at
/// all — which silently dropped every curated link a channel had.
fn channel_link_target(item: &Value) -> Option<String> {
    let link = item.get("channelExternalLinkViewModel")?.get("link")?;
    // The command carries the whole destination. The visible text is the
    // fallback because YouTube shortens it for display.
    let endpoint = link
        .get("commandRuns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|run| {
            run.get("onTap")?
                .get("innertubeCommand")?
                .get("urlEndpoint")?
                .get("url")?
                .as_str()
        })
        .find_map(youtube_redirect_target);
    let target = endpoint.or_else(|| {
        link.get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })?;
    Some(if target.contains("://") || target.starts_with("mailto:") {
        target
    } else {
        format!("https://{}", target.trim_start_matches('/'))
    })
}

/// Unwrap YouTube's own outbound redirect, which carries the destination in
/// `q`. Any other address is already the destination: unwrapping `q` from an
/// arbitrary URL corrupts it.
fn youtube_redirect_target(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    if url.host_str().map(|host| host.trim_start_matches("www.")) != Some("youtube.com")
        || url.path() != "/redirect"
    {
        return Some(raw.to_owned());
    }
    url.query_pairs()
        .find_map(|(key, value)| (key == "q").then(|| value.into_owned()))
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}
