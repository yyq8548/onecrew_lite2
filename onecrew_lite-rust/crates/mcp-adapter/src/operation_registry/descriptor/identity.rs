use std::{borrow::Cow, collections::BTreeSet, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::DescriptorError;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub(crate) struct EntryId(String);

impl EntryId {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, DescriptorError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            });
        valid
            .then_some(Self(value))
            .ok_or(DescriptorError::InvalidId)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EntryId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(|_| serde::de::Error::custom("invalid entry ID"))
    }
}

impl JsonSchema for EntryId {
    fn schema_name() -> Cow<'static, str> {
        "EntryId".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "maxLength": 128,
            "pattern": "^[a-z0-9._-]+$"
        })
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SurfaceId {
    Full,
    Compact,
    Accounts,
    Creators,
    Outreach,
    Mail,
    Skills,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceSet(BTreeSet<SurfaceId>);

impl SurfaceSet {
    pub(crate) fn new(
        surfaces: impl IntoIterator<Item = SurfaceId>,
    ) -> Result<Self, DescriptorError> {
        let surfaces = surfaces.into_iter().collect::<BTreeSet<_>>();
        (!surfaces.is_empty())
            .then_some(Self(surfaces))
            .ok_or(DescriptorError::EmptySurfaces)
    }

    pub(crate) fn allows(&self, surface: SurfaceId) -> bool {
        self.0.contains(&surface)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = SurfaceId> + '_ {
        self.0.iter().copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EffectClass {
    ReadLocal,
    ReadOpenWorld,
    WriteLocal,
    DeleteLocal,
    SyncOpenWorld,
    ExternalWrite,
    ExternalStep,
}

impl EffectClass {
    pub(crate) fn may_have_effect(self) -> bool {
        !matches!(self, Self::ReadLocal | Self::ReadOpenWorld)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SecretClass {
    None,
    Transient,
    VaultBound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApprovalClass {
    None,
    OwnerGuarded,
    AttendedPrompt,
}
