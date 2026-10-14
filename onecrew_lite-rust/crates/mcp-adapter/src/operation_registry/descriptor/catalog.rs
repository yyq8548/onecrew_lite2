use onecrew_local_runtime::ToolkitId;
use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ConnectionRequirement, DescriptorError, EffectClass, EntryId, EntryKind,
    OperationSafetySummary, PlaybookDifficulty, PlaybookStep, filters::deserialize_unique_filter,
};

macro_rules! bounded_string {
    ($name:ident, $schema_name:literal, $min:literal, $max:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
        #[serde(transparent)]
        pub(crate) struct $name(String);

        impl $name {
            pub(crate) fn parse(value: impl Into<String>) -> Result<Self, DescriptorError> {
                let value = value.into();
                (($min..=$max).contains(&value.len()))
                    .then_some(Self(value))
                    .ok_or(DescriptorError::InvalidBoundedString)
            }

            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self(String::new())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(value).map_err(|_| {
                    serde::de::Error::custom("bounded string is outside the allowed length")
                })
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                $schema_name.into()
            }

            fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({
                    "type": "string",
                    "minLength": $min,
                    "maxLength": $max
                })
            }
        }
    };
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct SearchText(String);

impl SearchText {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, DescriptorError> {
        let value = value.into();
        if !(1..=256).contains(&value.len()) {
            return Err(DescriptorError::InvalidBoundedString);
        }
        super::super::search::validate_query_text(&value)?;
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SearchText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(|error| {
            let message = match error {
                DescriptorError::SearchTextNotEnglish => {
                    "search query must be normalized English; translate non-English text first"
                }
                DescriptorError::SearchTextHasNoMeaningfulTerms => {
                    "search query must contain at least one meaningful English term"
                }
                _ => "search query must contain between 1 and 256 bytes",
            };
            serde::de::Error::custom(message)
        })
    }
}

impl JsonSchema for SearchText {
    fn schema_name() -> Cow<'static, str> {
        "SearchText".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "maxLength": 256,
            "description": "One independent outcome in normalized English. Translate non-English intent, split unrelated actions into separate searches, include named toolkits, and keep execution values out. An exact registry entry ID is also accepted."
        })
    }
}

bounded_string!(DomainId, "DomainId", 1, 64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct SearchLimit(u8);

impl<'de> Deserialize<'de> for SearchLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        (1..=6)
            .contains(&value)
            .then_some(Self(value))
            .ok_or_else(|| serde::de::Error::custom("search limit must be between 1 and 6"))
    }
}

impl JsonSchema for SearchLimit {
    fn schema_name() -> Cow<'static, str> {
        "SearchLimit".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type": "integer", "minimum": 1, "maximum": 6})
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogEntrySummary {
    pub(crate) id: EntryId,
    pub(crate) kind: EntryKind,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) toolkit: ToolkitId,
    pub(crate) match_role: CatalogMatchRole,
    pub(crate) effects: Vec<EffectClass>,
    pub(crate) connection_requirements: Vec<ConnectionRequirement>,
    pub(crate) safety: Option<OperationSafetySummary>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CatalogMatchRole {
    Primary,
    Related,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlaybookSummary {
    pub(crate) id: EntryId,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) use_case: String,
    pub(crate) toolkit_ids: Vec<ToolkitId>,
    pub(crate) primary_operation_ids: Vec<EntryId>,
    pub(crate) supporting_operation_ids: Vec<EntryId>,
    pub(crate) recommended_steps: Vec<PlaybookStep>,
    pub(crate) known_pitfalls: Vec<String>,
    pub(crate) difficulty: PlaybookDifficulty,
}

pub(in crate::operation_registry) fn deserialize_effect_filters<'de, D>(
    deserializer: D,
) -> Result<Vec<EffectClass>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_unique_filter(deserializer, 7, "effect")
}
