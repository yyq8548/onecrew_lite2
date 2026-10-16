//! One validated source for directory order, lookup and the public topic schema.
use super::{
    contract::Category,
    definition::{GuideDefinition, GuideGroup},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

const GROUPS: &[GuideGroup] = &[
    super::find::GROUP,
    super::enrich::GROUP,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GuideRegistrationError {
    UnknownCategory(&'static str),
    DuplicateCategory(&'static str),
    DuplicateTopic(&'static str),
    MissingMetadata(&'static str),
    InvalidTopic(&'static str),
}

#[derive(Debug)]
pub(super) struct RegisteredGroup {
    pub(super) category: Category,
    pub(super) title: &'static str,
    pub(super) definitions: &'static [GuideDefinition],
}

#[derive(Debug)]
pub(super) struct GuideRegistry {
    groups: Vec<RegisteredGroup>,
    by_topic: BTreeMap<&'static str, &'static GuideDefinition>,
}

impl GuideRegistry {
    pub(super) fn build(groups: &[GuideGroup]) -> Result<Self, GuideRegistrationError> {
        let mut registered = Self {
            groups: Vec::new(),
            by_topic: BTreeMap::new(),
        };
        let mut categories = BTreeSet::new();
        for group in groups {
            let category = Category::parse(group.category)
                .ok_or(GuideRegistrationError::UnknownCategory(group.category))?;
            if !categories.insert(group.category) {
                return Err(GuideRegistrationError::DuplicateCategory(group.category));
            }
            if group.title.trim().is_empty() || group.definitions.is_empty() {
                return Err(GuideRegistrationError::MissingMetadata(group.category));
            }
            for definition in group.definitions {
                if definition.topic.is_empty()
                    || !definition.topic.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
                {
                    return Err(GuideRegistrationError::InvalidTopic(definition.topic));
                }
                if [
                    definition.title,
                    definition.workflow,
                    definition.expected,
                    definition.example,
                ]
                .iter()
                .any(|value| value.trim().is_empty())
                    || [definition.questions, definition.queries]
                        .iter()
                        .any(|values| {
                            values.is_empty() || values.iter().any(|value| value.trim().is_empty())
                        })
                {
                    return Err(GuideRegistrationError::MissingMetadata(definition.topic));
                }
                if registered
                    .by_topic
                    .insert(definition.topic, definition)
                    .is_some()
                {
                    return Err(GuideRegistrationError::DuplicateTopic(definition.topic));
                }
            }
            registered.groups.push(RegisteredGroup {
                category,
                title: group.title,
                definitions: group.definitions,
            });
        }
        if registered.groups.is_empty() {
            return Err(GuideRegistrationError::MissingMetadata("groups"));
        }
        Ok(registered)
    }

    pub(super) fn groups(&self) -> &[RegisteredGroup] {
        &self.groups
    }

    pub(super) fn definitions(&self) -> impl Iterator<Item = &'static GuideDefinition> + '_ {
        self.groups
            .iter()
            .flat_map(|group| group.definitions.iter())
    }

    pub(super) fn lookup(&self, topic: &str) -> Option<&'static GuideDefinition> {
        self.by_topic.get(topic).copied()
    }
}

pub(super) fn registered() -> Result<&'static GuideRegistry, &'static GuideRegistrationError> {
    static REGISTRY: OnceLock<Result<GuideRegistry, GuideRegistrationError>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| GuideRegistry::build(GROUPS))
        .as_ref()
}
