//! Explicit Creator save candidates and validation before requested platform reads.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveCandidateInput {
    pub intent: onecrew_creator_runtime::CreatorSaveIntent,
    pub target: CreatorSaveTarget,
    pub connection: CreatorSaveConnection,
    pub provenance: CreatorSaveProvenanceInput,
    pub notes: Option<String>,
    #[schemars(schema_with = "onecrew_creator_runtime::creator_tags_schema")]
    pub tags: Option<Vec<String>>,
    pub quality: onecrew_creator_runtime::CreatorQualityInput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreatorSaveInputError {
    InvalidTarget,
    InvalidConnection,
    InvalidProvenance,
    Tags(onecrew_creator_runtime::CreatorTagsError),
}

impl std::fmt::Display for CreatorSaveInputError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(match self {
            Self::InvalidTarget => "Creator save target is invalid",
            Self::InvalidConnection => "Creator save connection does not match its platform",
            Self::InvalidProvenance => {
                "Creator save source is invalid or does not match its target"
            }
            Self::Tags(error) => return error.fmt(out),
        })
    }
}
impl std::error::Error for CreatorSaveInputError {}

impl CreatorSaveCandidateInput {
    /// Validate explicit selection inputs before any account or provider read.
    pub fn validate(&self) -> Result<(), CreatorSaveInputError> {
        use CreatorSaveInputError as Error;
        if let Some(tags) = &self.tags {
            onecrew_creator_runtime::validate_creator_tags(tags).map_err(Error::Tags)?;
        }
        let account = match (&self.target, &self.connection) {
            (CreatorSaveTarget::X { .. }, CreatorSaveConnection::X { account_ref_id })
            | (
                CreatorSaveTarget::Instagram { .. },
                CreatorSaveConnection::Instagram { account_ref_id },
            )
            | (
                CreatorSaveTarget::TelegramAdministrator { .. },
                CreatorSaveConnection::Telegram { account_ref_id },
            ) => account_ref_id,
            (
                CreatorSaveTarget::Youtube { .. },
                CreatorSaveConnection::Youtube {
                    service_credential_ref_id,
                },
            ) => service_credential_ref_id,
            _ => return Err(Error::InvalidConnection),
        };
        if account.is_nil() {
            return Err(Error::InvalidConnection);
        }
        let mut text = Vec::new();
        match &self.target {
            CreatorSaveTarget::X { user_id, handle }
            | CreatorSaveTarget::Instagram { user_id, handle } => {
                let (max, dot) = if matches!(self.target, CreatorSaveTarget::X { .. }) {
                    (15, false)
                } else {
                    (30, true)
                };
                if !positive_id(user_id) || !valid_handle(handle, max, dot) {
                    return Err(Error::InvalidTarget);
                }
                text.extend([user_id.as_str(), handle.as_str()]);
            }
            CreatorSaveTarget::Youtube { channel_id } => {
                if !onecrew_youtube::accepts_channel_id(channel_id) {
                    return Err(Error::InvalidTarget);
                }
                text.push(channel_id);
            }
            CreatorSaveTarget::TelegramAdministrator { user_id, community } => {
                if !positive_id(user_id) {
                    return Err(Error::InvalidTarget);
                }
                text.push(user_id);
                match community {
                    TelegramCommunityLocator::PublicUsername { username } => {
                        if !valid_handle(username, 32, false) {
                            return Err(Error::InvalidTarget);
                        }
                        text.push(username);
                    }
                    TelegramCommunityLocator::StableCommunity { external_id, .. } => {
                        if !positive_id(external_id) {
                            return Err(Error::InvalidTarget);
                        }
                        text.push(external_id);
                    }
                    TelegramCommunityLocator::SavedCommunityRoute {
                        creator_id,
                        route_id,
                    } => text.extend([creator_id.as_str(), route_id.as_str()]),
                }
            }
        }
        match &self.provenance {
            CreatorSaveProvenanceInput::SearchSelection {
                origin,
                source_url,
                description,
            } => {
                if matches!(self.target, CreatorSaveTarget::TelegramAdministrator { .. }) {
                    return Err(Error::InvalidProvenance);
                }
                match origin {
                    CreatorSaveSearchOrigin::Discovery {
                        discovery_id,
                        candidate_id,
                        query,
                        search_kind,
                        operation_id,
                        source_ref,
                    } => text.extend([
                        discovery_id.as_str(),
                        candidate_id.as_str(),
                        query.as_str(),
                        search_kind.as_str(),
                        operation_id.as_str(),
                        source_ref.as_str(),
                    ]),
                    CreatorSaveSearchOrigin::ProviderRead {
                        operation_id,
                        source_ref,
                    } => text.extend([operation_id.as_str(), source_ref.as_str()]),
                }
                if let Some(url) = source_url {
                    validate_url(url)?;
                    text.push(url);
                }
                if let Some(description) = description
                    && !description.is_empty()
                {
                    text.push(description);
                }
            }
            CreatorSaveProvenanceInput::DirectSelection {
                source_url,
                description,
            } => {
                if matches!(self.target, CreatorSaveTarget::TelegramAdministrator { .. }) {
                    return Err(Error::InvalidProvenance);
                }
                validate_url(source_url)?;
                text.push(source_url);
                if let Some(description) = description
                    && !description.is_empty()
                {
                    text.push(description);
                }
            }
            CreatorSaveProvenanceInput::AdministratorSelection {
                community_id,
                community_kind,
                source,
            } => {
                let CreatorSaveTarget::TelegramAdministrator { community, .. } = &self.target
                else {
                    return Err(Error::InvalidProvenance);
                };
                if !positive_id(community_id) {
                    return Err(Error::InvalidProvenance);
                }
                text.push(community_id);
                let matches = match (community, source) {
                    (
                        TelegramCommunityLocator::PublicUsername { .. },
                        AdministratorSelectionSource::OperatorCommunity,
                    ) => true,
                    (
                        TelegramCommunityLocator::StableCommunity { external_id, kind },
                        AdministratorSelectionSource::OperatorCommunity,
                    ) => external_id == community_id && kind == community_kind,
                    (
                        TelegramCommunityLocator::SavedCommunityRoute {
                            creator_id,
                            route_id,
                        },
                        AdministratorSelectionSource::SavedCommunityRoute {
                            creator_id: owner,
                            route_id: route,
                        },
                    ) => {
                        text.extend([owner.as_str(), route.as_str()]);
                        creator_id == owner && route_id == route
                    }
                    _ => false,
                };
                if !matches {
                    return Err(Error::InvalidProvenance);
                }
            }
        }
        if text.iter().any(|value| {
            value.is_empty() || value.trim() != *value || value.chars().any(char::is_control)
        }) {
            return Err(Error::InvalidProvenance);
        }
        Ok(())
    }
}

fn positive_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && !value.starts_with('0')
        && value.bytes().all(|c| c.is_ascii_digit())
}
fn valid_handle(value: &str, max: usize, dot: bool) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || (dot && c == b'.'))
}
fn validate_url(value: &str) -> Result<(), CreatorSaveInputError> {
    let url = url::Url::parse(value).map_err(|_| CreatorSaveInputError::InvalidProvenance)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(CreatorSaveInputError::InvalidProvenance);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_tags_reject_the_entire_youtube_batch_before_provider_admission() {
        let directory = tempfile::tempdir().unwrap();
        let platform = crate::LocalPlatform::open(directory.path()).unwrap();
        for (tags, expected) in [
            (
                vec!["AI", "YouTube", "近期活跃", "AI编程", "中文"],
                onecrew_creator_runtime::CreatorTagsError::TooMany,
            ),
            (
                vec!["AI", " AI "],
                onecrew_creator_runtime::CreatorTagsError::Duplicate,
            ),
        ] {
            let mut candidates = vec![candidate(); 5];
            for selected in &mut candidates {
                selected.target = CreatorSaveTarget::Youtube {
                    channel_id: "UCbIeBdHsfTpHsICZy59ZzhA".into(),
                };
                selected.connection = CreatorSaveConnection::Youtube {
                    service_credential_ref_id: Uuid::from_u128(1),
                };
                selected.provenance = CreatorSaveProvenanceInput::DirectSelection {
                    source_url: "https://www.youtube.com/channel/UCbIeBdHsfTpHsICZy59ZzhA".into(),
                    description: None,
                };
            }
            candidates[4].tags = Some(tags.into_iter().map(str::to_owned).collect());
            let result = platform
                .prepare_creator_save(
                    crate::PrepareCreatorSaveInput { candidates },
                    crate::CreatorSavePreparationContext {
                        origin: crate::ResultOrigin::new("tag-validation").unwrap(),
                        call_id: "save".into(),
                        operation_id: "onecrew.creator.candidate.batch.save".into(),
                    },
                )
                .await;
            // The first credential does not exist. Getting an input error for
            // the last row proves validation finished before any provider read.
            let error = result.unwrap_err();
            assert!(matches!(error, crate::CreatorSaveError::Input {
                candidate_index: 4,
                error: CreatorSaveInputError::Tags(actual),
            } if actual == expected));
            assert_eq!(error.effect(), crate::CreatorSaveEffect::NoMutation);
        }
    }

    fn candidate() -> CreatorSaveCandidateInput {
        CreatorSaveCandidateInput {
            intent: onecrew_creator_runtime::CreatorSaveIntent::SaveOrUpdate,
            target: CreatorSaveTarget::X {
                user_id: "123".into(),
                handle: "person".into(),
            },
            connection: CreatorSaveConnection::X {
                account_ref_id: Uuid::from_u128(1),
            },
            provenance: CreatorSaveProvenanceInput::DirectSelection {
                source_url: "https://x.com/person".into(),
                description: None,
            },
            notes: None,
            tags: None,
            quality: Default::default(),
        }
    }

    #[test]
    fn explicit_candidate_validation_preserves_platform_and_real_source() {
        let mut value = candidate();
        assert!(value.validate().is_ok());
        value.notes = Some("汉".repeat(1_365) + "a");
        assert!(value.validate().is_ok());
        value.notes.as_mut().unwrap().push('a');
        assert!(value.validate().is_ok());
        value.notes = None;
        value.connection = CreatorSaveConnection::Telegram {
            account_ref_id: Uuid::from_u128(1),
        };
        assert_eq!(
            value.validate(),
            Err(CreatorSaveInputError::InvalidConnection)
        );
        value = candidate();
        value.provenance = CreatorSaveProvenanceInput::DirectSelection {
            source_url: "https://user:secret@example.com".into(),
            description: None,
        };
        assert_eq!(
            value.validate(),
            Err(CreatorSaveInputError::InvalidProvenance)
        );
        value.provenance = CreatorSaveProvenanceInput::DirectSelection {
            source_url: "https://x.com/person".into(),
            description: Some("x".repeat(8192)),
        };
        assert!(value.validate().is_ok());
    }

    #[test]
    fn creator_save_requires_an_explicit_intent_before_reading_a_candidate() {
        let value = serde_json::json!({
            "target": {"platform": "x", "user_id": "123", "handle": "person"},
            "connection": {"platform": "x", "account_ref_id": "00000000-0000-0000-0000-000000000001"},
            "provenance": {"kind": "direct_selection", "source_url": "https://x.com/person", "description": null},
            "notes": null,
            "tags": null,
            "quality": {}
        });
        assert!(
            serde_json::from_value::<CreatorSaveCandidateInput>(value).is_err(),
            "save intent must not default to add or update"
        );
    }

    #[tokio::test]
    async fn mixed_batch_intents_are_rejected_before_provider_reads() {
        let directory = tempfile::tempdir().unwrap();
        let platform = crate::LocalPlatform::open(directory.path()).unwrap();
        let first = candidate();
        let mut second = candidate();
        second.intent = onecrew_creator_runtime::CreatorSaveIntent::AddNew;
        let origin = crate::ResultOrigin::new("mixed-intent").unwrap();
        let context = crate::OwnerReviewContext::new(
            origin,
            "mixed-intent-call",
            "onecrew.creator.candidate.batch.save",
            None,
        )
        .unwrap();
        let error = platform
            .prepare_creator_save_review(
                crate::PrepareCreatorSaveInput {
                    candidates: vec![first, second],
                },
                context,
            )
            .await
            .expect_err("mixed intents must be rejected");
        assert!(matches!(error, crate::CreatorSaveError::ConflictingIntents));
        assert_eq!(error.effect(), crate::CreatorSaveEffect::NoMutation);
    }

    #[test]
    fn telegram_source_must_name_the_actual_community_domain() {
        let mut value = candidate();
        value.target = CreatorSaveTarget::TelegramAdministrator {
            user_id: "123".into(),
            community: TelegramCommunityLocator::StableCommunity {
                external_id: "42".into(),
                kind: CreatorSaveCommunityKind::Channel,
            },
        };
        value.connection = CreatorSaveConnection::Telegram {
            account_ref_id: Uuid::from_u128(1),
        };
        value.provenance = CreatorSaveProvenanceInput::AdministratorSelection {
            community_id: "42".into(),
            community_kind: CreatorSaveCommunityKind::Supergroup,
            source: AdministratorSelectionSource::OperatorCommunity,
        };
        assert_eq!(
            value.validate(),
            Err(CreatorSaveInputError::InvalidProvenance)
        );
        if let CreatorSaveProvenanceInput::AdministratorSelection { community_kind, .. } =
            &mut value.provenance
        {
            *community_kind = CreatorSaveCommunityKind::Channel;
        }
        assert!(value.validate().is_ok());
    }

    #[test]
    fn a_youtube_target_is_a_channel_id_youtube_accepts_as_given() {
        let mut value = candidate();
        value.connection = CreatorSaveConnection::Youtube {
            service_credential_ref_id: Uuid::from_u128(1),
        };
        value.provenance = CreatorSaveProvenanceInput::DirectSelection {
            source_url: "https://www.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw".into(),
            description: None,
        };
        for channel_id in ["UC_x5XG1OV2P6uZZ5FSM9Ttw", "UC-x5XG1OV2P6uZZ5FSM9Ttw"] {
            value.target = CreatorSaveTarget::Youtube {
                channel_id: channel_id.into(),
            };
            assert_eq!(value.validate(), Ok(()), "{channel_id:?}");
        }
        // A channel ID is checked as given: padding is not trimmed away.
        for channel_id in [
            "@creator",
            "UC_too_short",
            "uc_x5XG1OV2P6uZZ5FSM9Ttw",
            "UC_x5XG1OV2P6uZZ5FSM9Ttwx",
            "UC_x5XG1OV2P6uZZ5FSM9Tt ",
            " UC_x5XG1OV2P6uZZ5FSM9Ttw",
        ] {
            value.target = CreatorSaveTarget::Youtube {
                channel_id: channel_id.into(),
            };
            assert_eq!(
                value.validate(),
                Err(CreatorSaveInputError::InvalidTarget),
                "{channel_id:?}"
            );
        }
    }

    #[tokio::test]
    async fn validates_the_entire_request_before_any_provider_admission() {
        let directory = tempfile::tempdir().unwrap();
        let platform = crate::LocalPlatform::open(directory.path()).unwrap();
        let first = candidate();
        let mut invalid = candidate();
        invalid.target = CreatorSaveTarget::X {
            user_id: "456".into(),
            handle: String::new(),
        };
        let result = platform
            .prepare_creator_save(
                crate::PrepareCreatorSaveInput {
                    candidates: vec![first, invalid],
                },
                crate::CreatorSavePreparationContext {
                    origin: crate::ResultOrigin::new("input-validation").unwrap(),
                    call_id: "save".into(),
                    operation_id: "onecrew.creator.candidate.batch.save".into(),
                },
            )
            .await;
        // The first connection is absent; reading it early would return ProviderRead.
        assert!(matches!(
            result,
            Err(crate::CreatorSaveError::Input {
                candidate_index: 1,
                error: CreatorSaveInputError::InvalidTarget,
            })
        ));
    }
}

/// Stable provider identity plus the lookup material required to re-read it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorSaveTarget {
    X {
        user_id: String,
        handle: String,
    },
    Instagram {
        user_id: String,
        handle: String,
    },
    Youtube {
        channel_id: String,
    },
    TelegramAdministrator {
        user_id: String,
        community: TelegramCommunityLocator,
    },
}

/// Telegram re-location never carries access hashes, sessions or admin objects.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TelegramCommunityLocator {
    PublicUsername {
        username: String,
    },
    StableCommunity {
        external_id: String,
        #[serde(rename = "community_kind")]
        kind: CreatorSaveCommunityKind,
    },
    SavedCommunityRoute {
        creator_id: String,
        route_id: String,
    },
}

/// Separate Telegram identity domains, independent of any provider response DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSaveCommunityKind {
    Channel,
    Supergroup,
}

impl CreatorSaveCommunityKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Supergroup => "supergroup",
        }
    }
}

/// References only; resolving account admission belongs to the caller workflow.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorSaveConnection {
    X { account_ref_id: Uuid },
    Instagram { account_ref_id: Uuid },
    Youtube { service_credential_ref_id: Uuid },
    Telegram { account_ref_id: Uuid },
}

/// Original search identities are retained as provenance, never invented.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorSaveSearchOrigin {
    Discovery {
        discovery_id: String,
        candidate_id: String,
        query: String,
        search_kind: String,
        operation_id: String,
        source_ref: String,
    },
    ProviderRead {
        operation_id: String,
        source_ref: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdministratorSelectionSource {
    OperatorCommunity,
    SavedCommunityRoute {
        creator_id: String,
        route_id: String,
    },
}

/// Source metadata only, without provider profiles or arbitrary JSON.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorSaveProvenanceInput {
    SearchSelection {
        origin: CreatorSaveSearchOrigin,
        source_url: Option<String>,
        description: Option<String>,
    },
    DirectSelection {
        source_url: String,
        description: Option<String>,
    },
    AdministratorSelection {
        community_id: String,
        community_kind: CreatorSaveCommunityKind,
        source: AdministratorSelectionSource,
    },
}
