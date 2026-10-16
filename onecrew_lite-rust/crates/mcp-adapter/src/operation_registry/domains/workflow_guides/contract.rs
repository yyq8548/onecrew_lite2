//! Wire contracts and projection; topic membership comes only from the registry.
use super::registry::{self, GuideRegistry};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(super) struct Topic(&'static str);

impl<'de> Deserialize<'de> for Topic {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let guides = registry::registered()
            .map_err(|_| serde::de::Error::custom("invalid workflow guide registration"))?;
        guides
            .lookup(&value)
            .map(|definition| Self(definition.topic))
            .ok_or_else(|| serde::de::Error::custom("unknown workflow guide topic"))
    }
}

impl JsonSchema for Topic {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Topic".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let topics = registry::registered()
            .expect("guide registry validated before schema compilation")
            .definitions()
            .map(|definition| definition.topic)
            .collect::<Vec<_>>();
        schemars::json_schema!({ "type": "string", "enum": topics })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum Category {
    Find,
    Enrich,
}

impl Category {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "find" => Some(Self::Find),
            "enrich" => Some(Self::Enrich),
            _ => None,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GuideInput {
    /// Omit to list the available search and enrichment scenarios.
    topic: Option<Topic>,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TopicSummary {
    topic: Topic,
    title: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CategoryDirectory {
    category: Category,
    title: String,
    topics: Vec<TopicSummary>,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkflowGuide {
    topic: Topic,
    title: String,
    questions: Vec<String>,
    workflow: Vec<String>,
    tool_discovery_queries: Vec<String>,
    confirmation_boundary: String,
    expected_output: Vec<String>,
    example: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum GuideOutput {
    Directory { categories: Vec<CategoryDirectory> },
    Guide { guide: WorkflowGuide },
}

pub(super) fn project(guides: &GuideRegistry, input: GuideInput) -> GuideOutput {
    let Some(topic) = input.topic else {
        return GuideOutput::Directory {
            categories: guides
                .groups()
                .iter()
                .map(|group| CategoryDirectory {
                    category: group.category,
                    title: group.title.into(),
                    topics: group
                        .definitions
                        .iter()
                        .map(|definition| TopicSummary {
                            topic: Topic(definition.topic),
                            title: definition.title.into(),
                        })
                        .collect(),
                })
                .collect(),
        };
    };
    let definition = guides
        .lookup(topic.0)
        .expect("topic validated against the same registry");
    GuideOutput::Guide { guide: WorkflowGuide {
        topic, title: definition.title.into(),
        questions: definition.questions.iter().map(|value| (*value).into()).collect(),
        workflow: vec![
            "First confirm the user's goal and scope for this request. Use onecrew_tool_search to discover currently available capabilities, then use onecrew_tool_get to retrieve the selected operation's live contract; follow the current schema_revision, connection requirements, confirmation method, and readback rules exactly. Use only identifiers and references returned by the current calls; do not guess arguments or reuse expired references. External searches follow the confirmed real-time search-plan contract. Keep actual user-facing questions/plans/explanations in the user's language.".into(),
            definition.workflow.into(),
        ],
        tool_discovery_queries: definition.queries.iter().map(|value| (*value).into()).collect(),
        confirmation_boundary: "This tool provides guidance only: it does not execute business operations or create durable tasks. Guidance is not authorization to write; do not write to Creator before user confirmation. Saves and updates must follow the user's actual authorization and the current owner's confirmation contract. User sending is available only through GmailBulk on the Mail page.".into(),
        expected_output: vec![definition.expected.into()], example: definition.example.into(),
    } }
}
