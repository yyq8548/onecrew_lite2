const RESULT_REF_PREFIX: &str = "rr_";

/// Opaque process-local reference to one governed execution result.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ResultRef(String);

impl ResultRef {
    pub(super) fn mint() -> Self {
        Self(format!(
            "{RESULT_REF_PREFIX}{}",
            uuid::Uuid::new_v4().simple()
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: String) -> Result<Self, &'static str> {
        validate_prefixed_uuid(&value, RESULT_REF_PREFIX)?;
        Ok(Self(value))
    }
}

impl FromStr for ResultRef {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value.to_owned())
    }
}

impl<'de> Deserialize<'de> for ResultRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::validate(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl JsonSchema for ResultRef {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ResultRef".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let pattern = format!("^{RESULT_REF_PREFIX}[0-9a-f]{{32}}$");
        schemars::json_schema!({
            "type": "string",
            "pattern": pattern
        })
    }
}

impl fmt::Display for ResultRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ResultOrigin(String);

impl ResultOrigin {
    pub fn new(scope_id: impl Into<String>) -> Result<Self, ResultStoreError> {
        let scope_id = scope_id.into();
        Self::validate(scope_id).map_err(|_| ResultStoreError::InvalidInput("origin_scope"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(scope_id: String) -> Result<Self, &'static str> {
        super::validation::validate_identifier(&scope_id, MAX_ORIGIN_SCOPE_BYTES, "origin_scope")
            .map_err(|_| "invalid result origin")?;
        Ok(Self(scope_id))
    }
}

impl FromStr for ResultOrigin {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value.to_owned())
    }
}

impl<'de> Deserialize<'de> for ResultOrigin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::validate(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl JsonSchema for ResultOrigin {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ResultOrigin".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[A-Za-z0-9._:-]{1,128}$",
            "maxLength": 128
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ResultPresentationKind(String);

impl ResultPresentationKind {
    pub fn new(value: impl Into<String>) -> Result<Self, ResultStoreError> {
        let value = value.into();
        Self::validate(value).map_err(|_| ResultStoreError::InvalidInput("presentation_kind"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: String) -> Result<Self, &'static str> {
        super::validation::validate_identifier(
            &value,
            MAX_PRESENTATION_KIND_BYTES,
            "presentation_kind",
        )
        .map_err(|_| "invalid result presentation kind")?;
        Ok(Self(value))
    }
}

impl FromStr for ResultPresentationKind {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value.to_owned())
    }
}

impl<'de> Deserialize<'de> for ResultPresentationKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::validate(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl JsonSchema for ResultPresentationKind {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ResultPresentationKind".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[A-Za-z0-9._:-]{1,128}$",
            "maxLength": 128
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ResultViewIdentity(String);

impl ResultViewIdentity {
    pub(super) fn mint() -> Self {
        Self(format!("rv_{}", uuid::Uuid::new_v4().simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: String) -> Result<Self, &'static str> {
        validate_prefixed_uuid(&value, "rv_")?;
        Ok(Self(value))
    }
}

impl FromStr for ResultViewIdentity {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value.to_owned())
    }
}

impl<'de> Deserialize<'de> for ResultViewIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::validate(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl JsonSchema for ResultViewIdentity {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ResultViewIdentity".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^rv_[0-9a-f]{32}$"
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ResultViewKind {
    ReadOnly,
    Confirmation,
    Choice,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultViewBinding {
    pub identity: ResultViewIdentity,
    pub kind: ResultViewKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ResultFingerprint(String);

impl ResultFingerprint {
    pub(super) fn digest(bytes: &[u8]) -> Self {
        Self(format!("sha256:{:x}", Sha256::digest(bytes)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: String) -> Result<Self, &'static str> {
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err("result fingerprint must use sha256");
        };
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("invalid result fingerprint");
        }
        Ok(Self(value))
    }
}

impl FromStr for ResultFingerprint {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value.to_owned())
    }
}

impl<'de> Deserialize<'de> for ResultFingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::validate(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl JsonSchema for ResultFingerprint {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ResultFingerprint".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^sha256:[0-9a-f]{64}$"
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ResultContinuationLeaseId(String);

impl ResultContinuationLeaseId {
    pub(super) fn mint() -> Self {
        Self(format!("rcl_{}", uuid::Uuid::new_v4().simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: String) -> Result<Self, &'static str> {
        validate_prefixed_uuid(&value, "rcl_")?;
        Ok(Self(value))
    }
}

impl FromStr for ResultContinuationLeaseId {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::validate(value.to_owned())
    }
}

impl<'de> Deserialize<'de> for ResultContinuationLeaseId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::validate(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl JsonSchema for ResultContinuationLeaseId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ResultContinuationLeaseId".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^rcl_[0-9a-f]{32}$"
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultBinding {
    pub result_ref: ResultRef,
    pub view: ResultViewBinding,
    pub origin: ResultOrigin,
    pub fingerprint: ResultFingerprint,
    #[serde(deserialize_with = "deserialize_revision")]
    #[schemars(range(min = 1))]
    pub revision: u64,
}

fn deserialize_revision<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let revision = u64::deserialize(deserializer)?;
    if revision == 0 {
        return Err(D::Error::custom("result revision must be positive"));
    }
    Ok(revision)
}
mod strict;

use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use sha2::{Digest, Sha256};

use super::{MAX_ORIGIN_SCOPE_BYTES, MAX_PRESENTATION_KIND_BYTES, ResultStoreError};
use strict::validate_prefixed_uuid;
