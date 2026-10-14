use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use onecrew_x_transport::{JsonTransport, Method};
use serde_json::{Map, Value, json};
use url::Url;

use crate::graphql::create_tweet_features;
use crate::models::{RelationshipAction, TweetAction};

use super::{XClient, XClientError};

impl<T: JsonTransport> XClient<T> {
    pub async fn relationship(
        &self,
        action: RelationshipAction,
        user_id: &str,
    ) -> Result<(), XClientError> {
        let url = Url::parse(&format!(
            "https://x.com/i/api/1.1/friendships/{}.json",
            action.endpoint()
        ))
        .map_err(|error| XClientError::Url(error.to_string()))?;
        let form = BTreeMap::from([
            ("user_id".into(), user_id.into()),
            ("include_profile_interstitial_type".into(), "1".into()),
        ]);
        self.transport.request_form(Method::POST, url, form).await?;
        Ok(())
    }

    pub async fn upload_media(
        &self,
        bytes: &[u8],
        media_type: &str,
    ) -> Result<String, XClientError> {
        const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
        const SUPPORTED: [&str; 4] = ["image/jpeg", "image/png", "image/gif", "image/webp"];
        if bytes.is_empty() {
            return Err(XClientError::Input("media is empty".into()));
        }
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(XClientError::Input("media exceeds the 5 MB limit".into()));
        }
        if !SUPPORTED.contains(&media_type) {
            return Err(XClientError::Input(format!(
                "unsupported media type: {media_type}"
            )));
        }
        let url = Url::parse("https://upload.twitter.com/i/media/upload.json")
            .map_err(|error| XClientError::Url(error.to_string()))?;
        let init = BTreeMap::from([
            ("command".into(), "INIT".into()),
            ("total_bytes".into(), bytes.len().to_string()),
            ("media_type".into(), media_type.into()),
        ]);
        let response = self
            .transport
            .request_form(Method::POST, url.clone(), init)
            .await?;
        let media_id = response
            .get("media_id_string")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| XClientError::Response("media INIT returned no ID".into()))?;
        let append = BTreeMap::from([
            ("command".into(), "APPEND".into()),
            ("media_id".into(), media_id.clone()),
            ("segment_index".into(), "0".into()),
            ("media_data".into(), STANDARD.encode(bytes)),
        ]);
        self.transport
            .request_form(Method::POST, url.clone(), append)
            .await?;
        let finalize = BTreeMap::from([
            ("command".into(), "FINALIZE".into()),
            ("media_id".into(), media_id.clone()),
        ]);
        self.transport
            .request_form(Method::POST, url, finalize)
            .await?;
        Ok(media_id)
    }

    pub async fn create_post(
        &self,
        text: &str,
        reply_to_id: Option<&str>,
        quote_tweet_id: Option<&str>,
        media_ids: &[String],
    ) -> Result<String, XClientError> {
        let media = media_ids
            .iter()
            .map(|id| json!({"media_id": id, "tagged_users": []}))
            .collect::<Vec<_>>();
        // Variables mirror the web client's CreateTweet exactly.
        let mut variables = json!({
            "tweet_text": text,
            "media": {"media_entities": media, "possibly_sensitive": false},
            "semantic_annotation_ids": [],
            "disallowed_reply_options": null,
            "semantic_annotation_options": {"source": "Htl"},
        });
        if let Some(tweet_id) = reply_to_id {
            variables["reply"] = json!({
                "in_reply_to_tweet_id": tweet_id,
                "exclude_reply_user_ids": [],
            });
        }
        if let Some(tweet_id) = quote_tweet_id {
            variables["attachment_url"] =
                Value::String(format!("https://x.com/i/status/{tweet_id}"));
        }
        let payload = self
            .post("CreateTweet", variables, Some(create_tweet_features()))
            .await?;
        // With visibility limits the result is wrapped as TweetWithVisibilityResults,
        // moving rest_id under `result/tweet/rest_id`; try both.
        let result = payload.pointer("/data/create_tweet/tweet_results/result");
        result
            .and_then(|r| {
                r.pointer("/rest_id")
                    .or_else(|| r.pointer("/tweet/rest_id"))
            })
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                // Surface X's real reason / raw shape instead of a generic message.
                let detail = payload
                    .pointer("/data/create_tweet/errors/0/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| payload.to_string().chars().take(500).collect());
                XClientError::Response(format!("created tweet ID missing; X response: {detail}"))
            })
    }

    pub async fn mutate_tweet(
        &self,
        action: TweetAction,
        tweet_id: &str,
    ) -> Result<(), XClientError> {
        let key = if action == TweetAction::Unretweet {
            "source_tweet_id"
        } else {
            "tweet_id"
        };
        let mut variables = Map::new();
        variables.insert(key.into(), Value::String(tweet_id.into()));
        if action.needs_dark_request() {
            variables.insert("dark_request".into(), Value::Bool(false));
        }
        self.post(action.operation(), Value::Object(variables), None)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use onecrew_x_transport::TransportError;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct NativeFixture {
        response: Value,
        forms: Arc<Mutex<Vec<BTreeMap<String, String>>>>,
    }

    #[async_trait]
    impl JsonTransport for NativeFixture {
        async fn request(
            &self,
            _: Method,
            _: Url,
            _: Option<Value>,
        ) -> Result<Value, TransportError> {
            Ok(self.response.clone())
        }
        async fn request_form(
            &self,
            method: Method,
            _: Url,
            form: BTreeMap<String, String>,
        ) -> Result<Value, TransportError> {
            assert_eq!(method, Method::POST);
            self.forms.lock().unwrap().push(form);
            Ok(self.response.clone())
        }
        async fn fetch_text(&self, _: Url) -> Result<String, TransportError> {
            Err(TransportError::Network("offline fixture".into()))
        }
    }

    #[tokio::test]
    async fn relationship_write_sends_only_the_selected_stable_id() {
        let forms = Arc::new(Mutex::new(Vec::new()));
        let client = XClient::new(NativeFixture {
            response: json!({"id_str":"9007199254740993"}),
            forms: forms.clone(),
        });
        client
            .relationship(RelationshipAction::Follow, "9007199254740993")
            .await
            .unwrap();
        let forms = forms.lock().unwrap();
        assert_eq!(forms.len(), 1);
        assert_eq!(
            forms[0].get("user_id").map(String::as_str),
            Some("9007199254740993")
        );
        assert!(!forms[0].contains_key("screen_name"));
    }

    #[tokio::test]
    async fn native_reply_receipt_requires_a_nonempty_id() {
        for id in ["", "   ", "123456789"] {
            let client = XClient::new(NativeFixture {
                response: json!({"data":{"create_tweet":{"tweet_results":{"result":{"rest_id":id}}}}}),
                forms: Arc::new(Mutex::new(Vec::new())),
            });
            let result = client.create_post("body", Some("100"), None, &[]).await;
            assert_eq!(result.is_ok(), !id.trim().is_empty());
        }
    }
}
