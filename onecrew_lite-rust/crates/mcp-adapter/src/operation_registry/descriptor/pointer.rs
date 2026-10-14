use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::DescriptorError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct InstancePointer(String);

impl InstancePointer {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, DescriptorError> {
        let value = value.into();
        (!value.is_empty() && value.len() <= 256 && value.starts_with('/'))
            .then_some(Self(value))
            .ok_or(DescriptorError::InvalidInstancePointer)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for InstancePointer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(|_| serde::de::Error::custom("invalid instance pointer"))
    }
}

impl JsonSchema for InstancePointer {
    fn schema_name() -> Cow<'static, str> {
        "InstancePointer".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "maxLength": 256,
            "pattern": "^/"
        })
    }
}
