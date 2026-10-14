//! Ask User Question input, identifiers, and answer validation bounds.
use onecrew_local_runtime::{ResultChoiceAnswer, ResultChoiceQuestion};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct AskUserQuestionInput {
    /// Identifier for this group of questions; one answer submission completes it.
    pub(in crate::operation_registry) interaction_id: UserQuestionId,
    /// Short title written in the user's conversation language.
    #[serde(deserialize_with = "deserialize_choice_title")]
    #[schemars(length(min = 1, max = 512))]
    pub(in crate::operation_registry) title: String,
    /// One to four questions, each with two to four suggested answers. Set
    /// selection_mode to multiple when multiple options may be selected;
    /// users may instead provide their own text and review before submitting.
    #[serde(deserialize_with = "deserialize_choice_questions")]
    #[schemars(schema_with = "choice_request_questions_schema")]
    pub(in crate::operation_registry) questions: Vec<ResultChoiceQuestion>,
}

impl AskUserQuestionInput {
    pub(in crate::operation_registry::v2) fn valid(&self) -> bool {
        valid_choice_title(&self.title) && valid_new_choice_questions(&self.questions)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(in crate::operation_registry) struct UserQuestionId(String);

impl UserQuestionId {
    pub(in crate::operation_registry) fn parse(
        value: impl Into<String>,
    ) -> Result<Self, QuestionContractFailure> {
        let value = value.into();
        valid_choice_identifier(&value)
            .then_some(Self(value))
            .ok_or(QuestionContractFailure)
    }

    pub(in crate::operation_registry) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for UserQuestionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?)
            .map_err(|_| serde::de::Error::custom("invalid choice interaction ID"))
    }
}

impl JsonSchema for UserQuestionId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "UserQuestionId".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "maxLength": 128,
            "pattern": "^[A-Za-z0-9._:-]+$"
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::operation_registry) struct QuestionContractFailure;
fn deserialize_choice_title<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    valid_choice_title(&value)
        .then_some(value)
        .ok_or_else(|| serde::de::Error::custom("choice title is outside the allowed bounds"))
}

fn deserialize_choice_questions<'de, D>(
    deserializer: D,
) -> Result<Vec<ResultChoiceQuestion>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let questions = Vec::<ResultChoiceQuestion>::deserialize(deserializer)?;
    valid_new_choice_questions(&questions)
        .then_some(questions)
        .ok_or_else(|| serde::de::Error::custom("choice questions are invalid"))
}

// Request limits do not narrow the persisted question model or historical resume.
fn choice_request_questions_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let mut question = ResultChoiceQuestion::json_schema(generator).to_value();
    question["properties"]["options"]["minItems"] = 2.into();
    question["properties"]["options"]["maxItems"] = 4.into();
    schemars::json_schema!({
        "type": "array",
        "minItems": 1,
        "maxItems": 4,
        "items": question
    })
}

fn valid_new_choice_questions(questions: &[ResultChoiceQuestion]) -> bool {
    (1..=4).contains(&questions.len())
        && questions
            .iter()
            .all(|question| (2..=4).contains(&question.options.len()))
        && valid_choice_questions(questions)
}

pub(super) fn deserialize_choice_answers<'de, D>(
    deserializer: D,
) -> Result<Vec<ResultChoiceAnswer>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let answers = Vec::<ResultChoiceAnswer>::deserialize(deserializer)?;
    let unique_questions = answers
        .iter()
        .map(|answer| answer.question_id())
        .collect::<BTreeSet<_>>()
        .len()
        == answers.len();
    ((1..=32).contains(&answers.len())
        && unique_questions
        && answers.iter().all(ResultChoiceAnswer::is_well_formed))
    .then_some(answers)
    .ok_or_else(|| serde::de::Error::custom("choice answers are invalid"))
}

fn valid_choice_title(value: &str) -> bool {
    valid_choice_text(value, 512)
}

pub(in crate::operation_registry::v2) fn valid_choice_questions(
    questions: &[ResultChoiceQuestion],
) -> bool {
    let unique_questions = questions
        .iter()
        .map(|question| question.id.as_str())
        .collect::<BTreeSet<_>>()
        .len()
        == questions.len();
    (1..=32).contains(&questions.len())
        && unique_questions
        && questions.iter().all(|question| {
            let unique_options = question
                .options
                .iter()
                .map(|option| option.id.as_str())
                .collect::<BTreeSet<_>>()
                .len()
                == question.options.len();
            valid_choice_identifier(&question.id)
                && valid_choice_text(&question.header, 8192)
                && valid_choice_text(&question.prompt, 8192)
                && (1..=32).contains(&question.options.len())
                && unique_options
                && question.options.iter().all(|option| {
                    valid_choice_identifier(&option.id)
                        && valid_choice_text(&option.label, 8192)
                        && option
                            .description
                            .as_deref()
                            .is_none_or(|description| valid_choice_text(description, 8192))
                })
        })
}

fn valid_choice_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
}

fn valid_choice_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}
