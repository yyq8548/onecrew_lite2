use std::borrow::Cow;

use rmcp::model::{CallToolResult, ContentBlock};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PublicParameterSource {
    pub parameter: &'static str,
    pub source: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PublicFailure {
    pub code: &'static str,
    pub message: &'static str,
    pub parameter_source: Option<PublicParameterSource>,
    pub next_action: Option<&'static str>,
    pub guard: Option<&'static str>,
}

const RESULT_PRESENTATION_FAILURE: PublicFailure = PublicFailure {
    code: "result_presentation_failed",
    message: "The result could not be presented through the MCP boundary safely.",
    parameter_source: None,
    next_action: Some(
        "Read the owning status or detail tool before deciding whether any write should be repeated.",
    ),
    guard: Some("Do not blindly retry an external write; its effect may be unknown."),
};

pub(crate) fn guided_failure(failure: PublicFailure) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(render(failure))])
}

/// A typed failure together with its static guidance. Clients that receive
/// `structuredContent` give the model only that, so the guidance rendered in
/// `content[0]` is part of the structured failure too.
#[derive(Serialize)]
pub(crate) struct GuidedFailure<F> {
    #[serde(flatten)]
    failure: F,
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_action: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    guard: Option<&'static str>,
}

impl<F: JsonSchema> JsonSchema for GuidedFailure<F> {
    fn schema_name() -> Cow<'static, str> {
        format!("Guided{}", F::schema_name()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = F::json_schema(generator);
        let Some(root) = schema.as_object_mut() else {
            panic!("{} is not an object schema", F::schema_name());
        };
        // Every variant of a closed tagged failure lists its own properties.
        if let Some(Value::Array(variants)) = root.get_mut("oneOf") {
            for variant in variants {
                let Value::Object(variant) = variant else {
                    panic!("{} variant is not an object schema", F::schema_name());
                };
                add_guidance(variant);
            }
        } else {
            add_guidance(root);
        }
        schema
    }
}

fn add_guidance(variant: &mut Map<String, Value>) {
    let Some(Value::Object(properties)) = variant.get_mut("properties") else {
        panic!("failure variant schema has no properties");
    };
    for field in ["message", "next_action", "guard"] {
        if properties
            .insert(field.to_owned(), json!({ "type": "string" }))
            .is_some()
        {
            panic!("failure variant schema already has {field}");
        }
    }
    let Some(Value::Array(required)) = variant.get_mut("required") else {
        panic!("failure variant schema has no required list");
    };
    required.push("message".into());
}

/// A failure on a model-visible tool: the typed failure with its guidance.
pub(crate) fn structured_failure(value: impl Serialize, failure: PublicFailure) -> CallToolResult {
    structured_failure_value(
        GuidedFailure {
            failure: value,
            message: failure.message,
            next_action: failure.next_action,
            guard: failure.guard,
        },
        failure,
    )
}

/// A failure on an App-only tool: the App reads the closed typed failure and
/// renders its own localized guidance.
pub(crate) fn structured_app_failure(
    value: impl Serialize,
    failure: PublicFailure,
) -> CallToolResult {
    structured_failure_value(value, failure)
}

fn structured_failure_value(value: impl Serialize, failure: PublicFailure) -> CallToolResult {
    match serde_json::to_value(value) {
        Ok(value) => {
            let mut result = CallToolResult::structured_error(value);
            result
                .content
                .insert(0, ContentBlock::text(render(failure)));
            result
        }
        Err(_) => result_presentation_failure(),
    }
}

pub(crate) fn result_presentation_failure() -> CallToolResult {
    guided_failure(RESULT_PRESENTATION_FAILURE)
}

fn render(failure: PublicFailure) -> String {
    let mut text = format!("Code: {}\nMessage: {}", failure.code, failure.message);
    if let Some(source) = failure.parameter_source {
        text.push_str("\nParameter: ");
        text.push_str(source.parameter);
        text.push_str("\nAuthoritative source: ");
        text.push_str(source.source);
    }
    if let Some(next_action) = failure.next_action {
        text.push_str("\nNext action: ");
        text.push_str(next_action);
    }
    if let Some(guard) = failure.guard {
        text.push_str("\nGuard: ");
        text.push_str(guard);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::Error as _;

    struct SerializationFailure;

    impl serde::Serialize for SerializationFailure {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(S::Error::custom("RAW_SERIALIZATION_CAUSE"))
        }
    }

    #[test]
    fn public_failure_uses_only_bounded_safe_fields_and_real_rmcp_results() {
        let failure = PublicFailure {
            code: "safe_code",
            message: "Safe public message.",
            parameter_source: Some(PublicParameterSource {
                parameter: "credential_id",
                source: "inventory youtube[].credential_id",
            }),
            next_action: Some("Read the typed detail."),
            guard: Some("Do not provide an API key."),
        };
        let error = guided_failure(failure);
        assert_eq!(error.is_error, Some(true));
        assert!(error.structured_content.is_none());
        let structured = structured_failure(json!({ "status": "not_found" }), failure);
        assert_eq!(structured.is_error, Some(true));
        assert_eq!(
            structured.structured_content.unwrap(),
            json!({
                "status": "not_found",
                "message": "Safe public message.",
                "next_action": "Read the typed detail.",
                "guard": "Do not provide an API key."
            })
        );
        let app = structured_app_failure(json!({ "status": "not_found" }), failure);
        assert_eq!(app.is_error, Some(true));
        assert_eq!(
            app.structured_content.unwrap(),
            json!({ "status": "not_found" })
        );
        let unguarded = PublicFailure {
            next_action: None,
            guard: None,
            ..failure
        };
        assert_eq!(
            structured_failure(json!({ "status": "not_found" }), unguarded)
                .structured_content
                .unwrap(),
            json!({ "status": "not_found", "message": "Safe public message." })
        );

        let serialization = structured_failure(SerializationFailure, failure);
        let wire = serde_json::to_string(&serialization).unwrap();
        assert!(wire.contains("result_presentation_failed"));
        assert!(!wire.contains("RAW_SERIALIZATION_CAUSE"));
    }
}
