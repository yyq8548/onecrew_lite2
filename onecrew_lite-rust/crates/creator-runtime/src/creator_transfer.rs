use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    CREATOR_BUNDLE_FORMAT, CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR, CREATOR_BUNDLE_MAX_BYTES,
    CREATOR_BUNDLE_MAX_CREATORS, CREATOR_BUNDLE_MAX_SAFE_INTEGER, Confidence, CreatorBundle,
    CreatorBundleProfile, CreatorBundleSnapshotKind, CreatorStage, CreatorTransferAccount,
    CreatorTransferConfidence, CreatorTransferContactRoute, CreatorTransferCreator,
    CreatorTransferLinkState, CreatorTransferPlatform, CreatorTransferProfile,
    CreatorTransferStage, CreatorTransferTargetKind, LinkState, Platform, TargetKind,
};

const SHA256_KEY_BYTES: usize = 71;
const MAX_DISPLAY_NAME_BYTES: usize = 512;
const MAX_EXTERNAL_ID_BYTES: usize = 512;
const MAX_PROFILE_TEXT_BYTES: usize = 512;
const MAX_PUBLIC_VALUE_BYTES: usize = 2_048;
const MAX_PROFILE_URL_BYTES: usize = 2_048;

impl From<Platform> for CreatorTransferPlatform {
    fn from(value: Platform) -> Self {
        match value {
            Platform::X => Self::X,
            Platform::Instagram => Self::Instagram,
            Platform::Telegram => Self::Telegram,
            Platform::YouTube => Self::YouTube,
            Platform::TikTok => Self::TikTok,
        }
    }
}

impl From<CreatorStage> for CreatorTransferStage {
    fn from(value: CreatorStage) -> Self {
        match value {
            CreatorStage::Prospect => Self::Prospect,
            CreatorStage::Engaged => Self::Engaged,
            CreatorStage::Collaborating => Self::Collaborating,
            CreatorStage::Archived => Self::Archived,
        }
    }
}

impl From<TargetKind> for CreatorTransferTargetKind {
    fn from(value: TargetKind) -> Self {
        match value {
            TargetKind::User => Self::User,
            TargetKind::Bot => Self::Bot,
            TargetKind::Channel => Self::Channel,
            TargetKind::Supergroup => Self::Supergroup,
            TargetKind::Group => Self::Group,
            TargetKind::Creator => Self::Creator,
            TargetKind::Business => Self::Business,
        }
    }
}

impl From<LinkState> for CreatorTransferLinkState {
    fn from(value: LinkState) -> Self {
        match value {
            LinkState::Claimed => Self::Claimed,
            LinkState::Confirmed => Self::Confirmed,
            LinkState::Rejected => Self::Rejected,
        }
    }
}

impl From<Confidence> for CreatorTransferConfidence {
    fn from(value: Confidence) -> Self {
        match value {
            Confidence::Exact => Self::Exact,
            Confidence::Asserted => Self::Asserted,
            Confidence::Possible => Self::Possible,
        }
    }
}

pub(crate) fn build_creator_bundle(
    mut creators: Vec<CreatorTransferCreator>,
) -> Result<CreatorBundle, String> {
    populate_creator_keys(&mut creators)?;
    creators.sort_by(|left, right| left.record_key.cmp(&right.record_key));
    finish_creator_bundle(creators)
}

pub(crate) fn build_creator_workbook_export(
    mut rows: Vec<(CreatorTransferCreator, crate::CreatorWorkbookDisplay)>,
) -> Result<crate::CreatorWorkbookExport, String> {
    populate_creator_keys(rows.iter_mut().map(|(creator, _)| creator))?;
    rows.sort_by(|left, right| left.0.record_key.cmp(&right.0.record_key));
    let mut display_by_record_key = BTreeMap::new();
    let mut creators = Vec::with_capacity(rows.len());
    for (creator, display) in rows {
        display_by_record_key.insert(creator.record_key.clone(), display);
        creators.push(creator);
    }
    let bundle = finish_creator_bundle(creators)?;
    Ok(crate::CreatorWorkbookExport {
        bundle,
        display_by_record_key,
    })
}

fn populate_creator_keys<'a>(
    creators: impl IntoIterator<Item = &'a mut CreatorTransferCreator>,
) -> Result<(), String> {
    let mut creators = creators.into_iter().collect::<Vec<_>>();
    let version = if creators.iter().any(|c| requires_v4(c)) {
        4
    } else {
        3
    };
    if creators.len() > CREATOR_BUNDLE_MAX_CREATORS {
        return Err(format!(
            "creator bundle exceeds the maximum of {CREATOR_BUNDLE_MAX_CREATORS} Creators"
        ));
    }
    if let Some(creator) = creators
        .iter()
        .find(|creator| creator.accounts.len() > CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR)
    {
        return Err(format!(
            "creator {} exceeds the maximum of {CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR} transferable accounts",
            creator.display_name
        ));
    }
    let mut semantic_occurrences = BTreeMap::<String, u32>::new();
    for creator in &mut creators {
        creator.accounts.sort_by(account_order);
        creator.contact_routes.sort_by(contact_route_order);
        // Both digests hash the same canonical projection, so it is built once
        // per Creator here instead of once per digest.
        let canonical = canonical_creator_version(creator, version);
        creator.semantic_sha256 = creator_semantic_sha256(&canonical)?;
        let occurrence = semantic_occurrences
            .entry(creator.semantic_sha256.clone())
            .or_default();
        creator.record_key = creator_record_key(&canonical, *occurrence)?;
        *occurrence = occurrence
            .checked_add(1)
            .ok_or_else(|| "creator record occurrence cannot be represented".to_string())?;
    }
    Ok(())
}

fn finish_creator_bundle(creators: Vec<CreatorTransferCreator>) -> Result<CreatorBundle, String> {
    let creator_count = u32::try_from(creators.len())
        .map_err(|_| "creator bundle count cannot be represented".to_string())?;
    let version = if creators.iter().any(requires_v4) {
        4
    } else {
        3
    };
    let content_sha256 = creator_bundle_content_sha256_version(creator_count, &creators, version)?;
    let bundle = CreatorBundle {
        format: CREATOR_BUNDLE_FORMAT.into(),
        format_version: version,
        profile: CreatorBundleProfile::PortablePublic,
        snapshot_kind: CreatorBundleSnapshotKind::Full,
        creator_count,
        content_sha256,
        creators,
    };
    // Field validation only: serializing the whole artifact here would be a
    // discarded pass over every byte, and each consumer that needs bytes
    // (`to_json_bytes`, XLSX) still applies its own size limit.
    validate_creator_bundle(&bundle)?;
    Ok(bundle)
}

fn requires_v4(c: &CreatorTransferCreator) -> bool {
    c.notes.is_some()
        || c.contact_routes
            .iter()
            .any(|r| r.channel == crate::ContactChannel::Wechat)
}

/// Frozen v1 excludes TikTok in every platform/channel position.
pub(crate) fn reject_v1_tiktok(creators: &[CreatorTransferCreator]) -> Result<(), String> {
    if creators.iter().any(|creator| {
        creator
            .follower_counts
            .contains_key(&CreatorTransferPlatform::TikTok)
            || creator
                .accounts
                .iter()
                .any(|account| account.platform == CreatorTransferPlatform::TikTok)
            || creator
                .contact_routes
                .iter()
                .any(|route| route.channel == crate::ContactChannel::TikTok)
    }) {
        return Err("creator bundle v1 cannot contain TikTok facts".into());
    }
    Ok(())
}

impl CreatorBundle {
    /// Explicit import evolution. Sync/current persistence must use the strict
    /// `from_json_bytes` reader; a legacy artifact is validated before conversion.
    pub fn from_import_json_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > CREATOR_BUNDLE_MAX_BYTES {
            return Err("creator bundle exceeds maximum artifact size".into());
        }
        let wire: Value = serde_json::from_slice(bytes)
            .map_err(|_| "creator bundle is not valid canonical JSON")?;
        match wire.get("format_version").and_then(Value::as_u64) {
            Some(4) => Self::from_json_bytes(bytes),
            Some(1..=3) => {
                let bundle = Self::from_staged_import_json_bytes(bytes)?;
                build_creator_bundle(bundle.creators)
            }
            _ => Err("creator bundle version has no supported import conversion".into()),
        }
    }

    /// Decode a released staged import without changing its digest or confirmation identity.
    pub(crate) fn from_staged_import_json_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > CREATOR_BUNDLE_MAX_BYTES {
            return Err("creator bundle exceeds maximum artifact size".into());
        }
        let mut wire: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let version = wire.get("format_version").and_then(Value::as_u64);
        if version == Some(4) {
            return Self::from_json_bytes(bytes);
        }
        if !matches!(version, Some(1..=3)) {
            return Err("unsupported staged Creator bundle version".into());
        }
        for creator in wire
            .get_mut("creators")
            .and_then(Value::as_array_mut)
            .ok_or("legacy Creator bundle is missing creators")?
        {
            let object = creator
                .as_object_mut()
                .ok_or("legacy Creator must be an object")?;
            if version != Some(3) && object.insert("tags".into(), json!([])).is_some() {
                return Err("legacy Creator bundle cannot contain tags".into());
            }
            if object.insert("notes".into(), Value::Null).is_some() {
                return Err("historical Creator bundle cannot contain notes".into());
            }
        }
        let bundle: Self = serde_json::from_value(wire).map_err(|error| error.to_string())?;
        if bundle.format_version == 1 {
            reject_v1_tiktok(&bundle.creators)?;
        }
        let canonical = bundle.to_staged_import_json_bytes()?;
        if canonical != bytes {
            return Err("creator bundle artifact is not in canonical byte form".into());
        }
        Ok(bundle)
    }

    pub(crate) fn to_staged_import_json_bytes(&self) -> Result<Vec<u8>, String> {
        if self.format_version == 4 {
            return self.to_json_bytes();
        }
        if !matches!(self.format_version, 1..=3)
            || self.creators.iter().any(|creator| creator.notes.is_some())
            || (self.format_version < 3
                && self.creators.iter().any(|creator| !creator.tags.is_empty()))
        {
            return Err("invalid historical Creator bundle".into());
        }
        if self.format_version == 1 {
            reject_v1_tiktok(&self.creators)?;
        }
        validate_creator_bundle_version(self, self.format_version)?;
        let json = serde_json::to_string(self)
            .map_err(|error| error.to_string())?
            .replace(",\"notes\":null", "");
        Ok(format!(
            "{}\n",
            if self.format_version < 3 {
                json.replace(",\"tags\":[]", "")
            } else {
                json
            }
        )
        .into_bytes())
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > CREATOR_BUNDLE_MAX_BYTES {
            return Err(format!(
                "creator bundle exceeds the maximum artifact size of {CREATOR_BUNDLE_MAX_BYTES} bytes"
            ));
        }
        let bundle = serde_json::from_slice::<Self>(bytes)
            .map_err(|_| "creator bundle is not valid current canonical JSON".to_string())?;
        let canonical = bundle.to_json_bytes()?;
        if canonical != bytes {
            return Err("creator bundle artifact is not in canonical byte form".into());
        }
        Ok(bundle)
    }

    /// Canonical remote/persistence and full-fidelity backup representation:
    /// compact UTF-8 JSON plus one LF.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, String> {
        encode_creator_bundle_with_limit(self, CREATOR_BUNDLE_MAX_BYTES)
    }

    /// Selects canonical Creator items by strictly ordered bundle-local key.
    ///
    /// The returned values are the exact items from this full bundle. No
    /// transfer facts are recomputed or projected into another model.
    pub fn select_creators(
        &self,
        record_keys: &[String],
    ) -> Result<Vec<CreatorTransferCreator>, String> {
        self.to_json_bytes()?;
        if record_keys.len() > CREATOR_BUNDLE_MAX_CREATORS {
            return Err("creator subset exceeds the bundle Creator limit".into());
        }
        let mut selected = Vec::with_capacity(record_keys.len());
        let mut previous: Option<&str> = None;
        for record_key in record_keys {
            if previous.is_some_and(|previous| previous >= record_key.as_str()) {
                return Err("creator subset record keys are not in canonical order".into());
            }
            let index = self
                .creators
                .binary_search_by(|creator| creator.record_key.as_str().cmp(record_key.as_str()))
                .map_err(|_| "creator subset contains a record key outside the source bundle")?;
            selected.push(self.creators[index].clone());
            previous = Some(record_key);
        }
        Ok(selected)
    }
}

fn encode_creator_bundle_with_limit(
    bundle: &CreatorBundle,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    validate_creator_bundle(bundle)?;
    let mut encoded = serde_json::to_vec(bundle).map_err(|error| error.to_string())?;
    encoded.push(b'\n');
    if encoded.len() > max_bytes {
        return Err(format!(
            "creator bundle exceeds the maximum artifact size of {max_bytes} bytes"
        ));
    }
    Ok(encoded)
}

fn validate_creator_bundle(bundle: &CreatorBundle) -> Result<(), String> {
    if !matches!(bundle.format_version, 3 | 4) {
        return Err("unsupported current Creator bundle".into());
    }
    validate_creator_bundle_version(bundle, bundle.format_version)
}

fn validate_creator_bundle_version(bundle: &CreatorBundle, version: u16) -> Result<(), String> {
    if version < 4 && bundle.creators.iter().any(requires_v4) {
        return Err("historical Creator bundle cannot contain WeChat contacts".into());
    }
    if bundle.format != CREATOR_BUNDLE_FORMAT
        || bundle.format_version != version
        || bundle.profile != CreatorBundleProfile::PortablePublic
        || bundle.snapshot_kind != CreatorBundleSnapshotKind::Full
    {
        return Err("creator bundle identity is not the supported current contract".into());
    }
    if bundle.creators.len() > CREATOR_BUNDLE_MAX_CREATORS {
        return Err(format!(
            "creator bundle exceeds the maximum of {CREATOR_BUNDLE_MAX_CREATORS} Creators"
        ));
    }
    if usize::try_from(bundle.creator_count).ok() != Some(bundle.creators.len()) {
        return Err("creator bundle creator_count does not match creators".into());
    }
    require_strictly_sorted_by(
        &bundle.creators,
        |creator| creator.record_key.clone(),
        "Creators",
    )?;
    // One canonical projection per Creator serves both the per-Creator digest
    // check and the record-key check below.
    let canonicals = bundle
        .creators
        .iter()
        .map(|creator| canonical_creator_version(creator, version))
        .collect::<Vec<_>>();
    for (creator, canonical) in bundle.creators.iter().zip(&canonicals) {
        validate_creator(creator, canonical)?;
    }
    validate_record_keys(bundle, &canonicals)?;
    validate_global_identity(bundle)?;
    let expected =
        creator_bundle_content_sha256_version(bundle.creator_count, &bundle.creators, version)?;
    if bundle.content_sha256 != expected {
        return Err("creator bundle content_sha256 does not match canonical content".into());
    }
    Ok(())
}

fn validate_creator(creator: &CreatorTransferCreator, canonical: &Value) -> Result<(), String> {
    if let Some(notes) = &creator.notes {
        validate_required_text("creator.notes", notes, 32_000)?;
    }
    if crate::creators::normalize_creator_tags(creator.tags.clone())
        .map_err(|error| error.to_string())?
        != creator.tags
    {
        return Err("Creator bundle tags must already be normalized".into());
    }
    validate_sha256_key("record_key", &creator.record_key)?;
    validate_required_text(
        "display_name",
        &creator.display_name,
        MAX_DISPLAY_NAME_BYTES,
    )?;
    if creator.score.is_some_and(|score| score > 100) {
        return Err("creator bundle score must be between 0 and 100".into());
    }
    for count in creator.follower_counts.values() {
        validate_safe_integer("follower_count", *count)?;
    }
    if creator.accounts.len() > CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR {
        return Err(format!(
            "creator {} exceeds the maximum of {CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR} transferable accounts",
            creator.record_key
        ));
    }
    require_strictly_sorted_by(
        &creator.accounts,
        |account| {
            (
                account.capture_order,
                account.platform,
                account.target_kind,
                account.external_id.clone(),
            )
        },
        "Creator accounts",
    )?;
    for account in &creator.accounts {
        validate_stable_ascii_id("external_id", &account.external_id, MAX_EXTERNAL_ID_BYTES)?;
        if account.platform == CreatorTransferPlatform::TikTok
            && !account.external_id.strip_prefix("tto:").is_some_and(|id| {
                !id.is_empty()
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            })
        {
            return Err(
                "TikTok transfer identity must be an explicit tto:<creator_id> identity".into(),
            );
        }
        validate_safe_integer("account.capture_order", u64::from(account.capture_order))?;
        if let Some(profile) = account.latest_profile.as_ref() {
            validate_optional_text(
                "profile.handle",
                profile.handle.as_deref(),
                MAX_PROFILE_TEXT_BYTES,
            )?;
            validate_optional_text(
                "profile.display_name",
                profile.display_name.as_deref(),
                MAX_PROFILE_TEXT_BYTES,
            )?;
            if let Some(profile_url) = profile.profile_url.as_deref() {
                validate_public_url("profile.profile_url", profile_url)?;
            }
            if let Some(avatar_url) = profile.avatar_url.as_deref() {
                validate_public_url("profile.avatar_url", avatar_url)?;
            }
        }
    }
    require_strictly_sorted_by(
        &creator.contact_routes,
        |route| {
            (
                route.save_order,
                route.channel.as_str().to_owned(),
                route.locator_kind.as_str().to_owned(),
                route.normalized_value.clone(),
            )
        },
        "Creator contact routes",
    )?;
    for route in &creator.contact_routes {
        validate_safe_integer("contact_route.save_order", u64::from(route.save_order))?;
        validate_required_text(
            "locator.normalized_value",
            &route.normalized_value,
            MAX_PUBLIC_VALUE_BYTES,
        )?;
        validate_required_text(
            "locator.display_value",
            &route.display_value,
            MAX_PUBLIC_VALUE_BYTES,
        )?;
    }
    let expected = creator_semantic_sha256(canonical)?;
    if creator.semantic_sha256 != expected {
        return Err("creator semantic_sha256 does not match canonical facts".into());
    }
    Ok(())
}

fn validate_global_identity(bundle: &CreatorBundle) -> Result<(), String> {
    let mut account_facts =
        BTreeMap::<(CreatorTransferPlatform, CreatorTransferTargetKind, &str), Value>::new();
    let mut account_links = BTreeSet::new();
    let mut confirmed_owner = BTreeMap::new();
    let mut locator_owner = BTreeMap::new();
    let mut locator_claims = BTreeSet::new();
    for creator in &bundle.creators {
        for account in &creator.accounts {
            let account_value = json!([
                account.capture_order,
                account.platform.as_str(),
                account.target_kind.as_str(),
                account.external_id,
                account.source_kind.as_str(),
                account.latest_profile,
            ]);
            let natural_key = (
                account.platform,
                account.target_kind,
                account.external_id.as_str(),
            );
            if let Some(existing) = account_facts.insert(natural_key, account_value.clone())
                && existing != account_value
            {
                return Err(
                    "creator bundle account natural key has conflicting public facts".into(),
                );
            }
            if !account_links.insert((creator.record_key.as_str(), natural_key)) {
                return Err("creator bundle duplicates an account claim for one Creator".into());
            }
            if account.link_state == CreatorTransferLinkState::Confirmed
                && let Some(existing_owner) =
                    confirmed_owner.insert(natural_key, creator.record_key.as_str())
                && existing_owner != creator.record_key.as_str()
            {
                return Err("creator bundle confirms one account for multiple Creators".into());
            }
        }
        for route in &creator.contact_routes {
            let locator_key = (
                route.channel.as_str(),
                route.locator_kind.as_str(),
                route.normalized_value.as_str(),
            );
            if !locator_claims.insert((creator.record_key.as_str(), locator_key)) {
                return Err("creator bundle duplicates a contact claim for one Creator".into());
            }
            if let Some(existing_owner) =
                locator_owner.insert(locator_key, creator.record_key.as_str())
                && existing_owner != creator.record_key.as_str()
            {
                return Err("creator bundle shares a contact locator across Creators".into());
            }
        }
    }
    Ok(())
}

/// Checks that the record keys inside each semantic group are exactly the
/// canonical occurrence sequence `0..n`. One pass groups the keys and keeps
/// the group's canonical projection, so the check costs one record-key hash
/// per Creator instead of a linear search per group.
fn validate_record_keys(bundle: &CreatorBundle, canonicals: &[Value]) -> Result<(), String> {
    let mut by_semantic = BTreeMap::<&str, (&Value, Vec<&str>)>::new();
    for (creator, canonical) in bundle.creators.iter().zip(canonicals) {
        by_semantic
            .entry(&creator.semantic_sha256)
            .or_insert_with(|| (canonical, Vec::new()))
            .1
            .push(&creator.record_key);
    }
    for (canonical, mut actual) in by_semantic.into_values() {
        actual.sort_unstable();
        let mut expected = (0..actual.len())
            .map(|occurrence| {
                let occurrence = u32::try_from(occurrence)
                    .map_err(|_| "creator record occurrence cannot be represented".to_string())?;
                creator_record_key(canonical, occurrence)
            })
            .collect::<Result<Vec<_>, _>>()?;
        expected.sort_unstable();
        if actual != expected.iter().map(String::as_str).collect::<Vec<_>>() {
            return Err("creator bundle record_key does not match canonical projection".into());
        }
    }
    Ok(())
}

fn creator_bundle_content_sha256_version(
    creator_count: u32,
    creators: &[CreatorTransferCreator],
    version: u16,
) -> Result<String, String> {
    let canonical = json!([
        CREATOR_BUNDLE_FORMAT,
        version,
        CreatorBundleProfile::PortablePublic.as_str(),
        CreatorBundleSnapshotKind::Full.as_str(),
        creator_count,
        creators
            .iter()
            .map(|creator| json!([creator.record_key, creator.semantic_sha256]))
            .collect::<Vec<_>>(),
    ]);
    let bytes = serde_json::to_vec(&canonical).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

/// `canonical` is the `canonical_creator` projection of the Creator.
fn creator_semantic_sha256(canonical: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(canonical).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

/// `canonical` is the `canonical_creator` projection of the Creator. The
/// tuple serializes to the same JSON array as the former `json!` literal,
/// so the key bytes are unchanged while the projection is borrowed, not
/// cloned.
fn creator_record_key(canonical: &Value, occurrence: u32) -> Result<String, String> {
    let bytes = serde_json::to_vec(&("reacher.creator_record", canonical, occurrence))
        .map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

/// Exercise frozen v1/v2 hashes against the same exported facts in regression fixtures.
#[cfg(test)]
pub(crate) fn legacy_v2_projection_for_test(mut bundle: CreatorBundle) -> CreatorBundle {
    project_legacy_import_bundle(&mut bundle).unwrap();
    bundle
}

fn project_legacy_import_bundle(bundle: &mut CreatorBundle) -> Result<(), String> {
    project_import_version(bundle, 2)
}

fn project_import_version(bundle: &mut CreatorBundle, version: u16) -> Result<(), String> {
    let mut occurrences = BTreeMap::<String, u32>::new();
    for creator in &mut bundle.creators {
        if (version < 3 && !creator.tags.is_empty()) || creator.notes.is_some() {
            return Err("tagged Creator facts have no legacy import identity".into());
        }
        let canonical = canonical_creator_version(creator, version);
        creator.semantic_sha256 = creator_semantic_sha256(&canonical)?;
        let occurrence = occurrences
            .entry(creator.semantic_sha256.clone())
            .or_default();
        creator.record_key = creator_record_key(&canonical, *occurrence)?;
        *occurrence += 1;
    }
    bundle
        .creators
        .sort_by(|left, right| left.record_key.cmp(&right.record_key));
    bundle.format_version = version;
    bundle.content_sha256 =
        creator_bundle_content_sha256_version(bundle.creator_count, &bundle.creators, version)?;
    validate_creator_bundle_version(bundle, version)
}

/// Released import receipts and pending operations remain authoritative for exactly
/// equivalent tag-empty facts. No historical identity exists for a tagged bundle.
pub(crate) fn historical_import_digests(bundle: &CreatorBundle) -> Result<Vec<String>, String> {
    if bundle.creators.iter().any(requires_v4) {
        return Ok(Vec::new());
    }
    let mut digests = Vec::new();
    if bundle.format_version == 4 {
        let mut v3 = bundle.clone();
        project_import_version(&mut v3, 3)?;
        digests.push(v3.content_sha256);
    }
    if bundle
        .creators
        .iter()
        .any(|creator| !creator.tags.is_empty())
    {
        return Ok(digests);
    }
    let mut legacy = bundle.clone();
    project_legacy_import_bundle(&mut legacy)?;
    digests.push(legacy.content_sha256);
    if reject_v1_tiktok(&legacy.creators).is_ok() {
        digests.push(creator_bundle_content_sha256_version(
            legacy.creator_count,
            &legacy.creators,
            1,
        )?);
    }
    Ok(digests)
}

fn canonical_creator_version(creator: &CreatorTransferCreator, version: u16) -> Value {
    let mut projection = json!([
        creator.display_name,
        creator.score,
        creator
            .follower_counts
            .iter()
            .map(|(platform, count)| json!([platform.as_str(), count]))
            .collect::<Vec<_>>(),
        creator.pinned,
        creator.relationship.stage.as_str(),
        creator
            .accounts
            .iter()
            .map(canonical_account)
            .collect::<Vec<_>>(),
        creator
            .contact_routes
            .iter()
            .map(canonical_contact_route)
            .collect::<Vec<_>>(),
    ]);
    if version >= 3 {
        projection
            .as_array_mut()
            .expect("canonical projection is an array")
            .push(json!(creator.tags));
    }
    if version >= 4 {
        projection
            .as_array_mut()
            .expect("canonical array")
            .push(json!(creator.notes));
    }
    projection
}

fn canonical_account(account: &CreatorTransferAccount) -> Value {
    json!([
        account.capture_order,
        account.platform.as_str(),
        account.target_kind.as_str(),
        account.external_id,
        account.link_state.as_str(),
        account.confidence.as_str(),
        account.source_kind.as_str(),
        account.latest_profile.as_ref().map(canonical_profile),
    ])
}

fn canonical_profile(profile: &CreatorTransferProfile) -> Value {
    json!([
        profile.handle,
        profile.display_name,
        profile.profile_url,
        profile.avatar_url,
    ])
}

fn canonical_contact_route(route: &CreatorTransferContactRoute) -> Value {
    json!([
        route.save_order,
        route.channel.as_str(),
        route.locator_kind.as_str(),
        route.normalized_value,
        route.display_value,
        route.confidence.as_str(),
        route
            .source_kinds
            .iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>(),
    ])
}

fn account_order(
    left: &CreatorTransferAccount,
    right: &CreatorTransferAccount,
) -> std::cmp::Ordering {
    (
        left.capture_order,
        left.platform,
        left.target_kind,
        left.external_id.as_str(),
    )
        .cmp(&(
            right.capture_order,
            right.platform,
            right.target_kind,
            right.external_id.as_str(),
        ))
}

fn contact_route_order(
    left: &CreatorTransferContactRoute,
    right: &CreatorTransferContactRoute,
) -> std::cmp::Ordering {
    (
        left.save_order,
        left.channel.as_str(),
        left.locator_kind.as_str(),
        left.normalized_value.as_str(),
    )
        .cmp(&(
            right.save_order,
            right.channel.as_str(),
            right.locator_kind.as_str(),
            right.normalized_value.as_str(),
        ))
}

fn require_strictly_sorted_by<T, K: Ord>(
    values: &[T],
    key: impl Fn(&T) -> K,
    label: &str,
) -> Result<(), String> {
    if values.windows(2).any(|pair| key(&pair[0]) >= key(&pair[1])) {
        return Err(format!("creator bundle {label} are not strictly sorted"));
    }
    Ok(())
}

fn validate_optional_text(
    field: &str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), String> {
    if let Some(value) = value {
        validate_required_text(field, value, max_bytes)?;
    }
    Ok(())
}

fn validate_required_text(field: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("creator bundle field {field} cannot be empty"));
    }
    if value.len() > max_bytes {
        return Err(format!(
            "creator bundle field {field} exceeds {max_bytes} bytes"
        ));
    }
    Ok(())
}

fn validate_stable_ascii_id(field: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    validate_required_text(field, value, max_bytes)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\\'))
    {
        return Err(format!(
            "creator bundle field {field} must be a visible ASCII stable ID"
        ));
    }
    Ok(())
}

fn validate_sha256_key(field: &str, value: &str) -> Result<(), String> {
    if value.len() != SHA256_KEY_BYTES
        || !value.starts_with("sha256:")
        || !value["sha256:".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "creator bundle field {field} must be a lowercase sha256 key"
        ));
    }
    Ok(())
}

fn validate_safe_integer(field: &str, value: u64) -> Result<(), String> {
    if value > CREATOR_BUNDLE_MAX_SAFE_INTEGER {
        return Err(format!(
            "creator bundle field {field} exceeds the cross-language safe integer maximum"
        ));
    }
    Ok(())
}

fn validate_public_url(field: &str, value: &str) -> Result<(), String> {
    validate_required_text(field, value, MAX_PROFILE_URL_BYTES)?;
    let parsed = url::Url::parse(value)
        .map_err(|_| format!("creator bundle field {field} is not a valid absolute URL"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(format!(
            "creator bundle field {field} must be an HTTP(S) URL without userinfo"
        ));
    }
    Ok(())
}

impl CreatorBundleProfile {
    const fn as_str(self) -> &'static str {
        match self {
            Self::PortablePublic => "portable_public",
        }
    }
}

impl CreatorBundleSnapshotKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
        }
    }
}

impl crate::CreatorRuntime {
    /// Build a Desktop workbook with separate snapshot and CPU admission.
    /// No repository lease or read permit survives into XLSX encoding.
    pub async fn export_creator_workbook_xlsx_async(
        &self,
    ) -> Result<crate::CreatorWorkbookXlsx, crate::CreatorRepositoryCallError<String>> {
        let repository = std::sync::Arc::clone(&self.repository);
        let snapshot = self
            .repository_read_segment(move || repository.export_creator_workbook_snapshot())
            .await?;
        self.repository_cpu_segment(move || {
            let workbook = build_creator_workbook_export(snapshot)?;
            let bytes = workbook.to_xlsx_bytes()?;
            Ok(crate::CreatorWorkbookXlsx {
                artifact_sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes,
                creator_count: workbook.bundle.creator_count,
                content_sha256: workbook.bundle.content_sha256,
            })
        })
        .await
    }

    pub fn export_creator_bundle(&self) -> Result<CreatorBundle, String> {
        self.repository.export_creator_bundle()
    }

    pub fn export_creator_workbook(&self) -> Result<crate::CreatorWorkbookExport, String> {
        self.repository.export_creator_workbook()
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::prepare_creator_import_xlsx_bytes_async`] to own admission and dispatch.
    pub fn prepare_creator_import_xlsx_bytes(
        &self,
        bytes: &[u8],
    ) -> Result<crate::PrepareCreatorImportResult, String> {
        self.repository
            .prepare_creator_import(CreatorBundle::from_xlsx_bytes(bytes)?)
    }

    /// Parse, snapshot, plan, and prepare an import without keeping either a
    /// SQLite lease or CPU-heavy validation inside the writer segment.
    pub async fn prepare_creator_import_xlsx_bytes_async(
        &self,
        bytes: Vec<u8>,
    ) -> Result<crate::PrepareCreatorImportResult, crate::CreatorRepositoryCallError<String>> {
        self.prepare_creator_import_xlsx_preview_async(bytes)
            .await
            .map(|(result, _)| result)
    }

    /// Preview facts come from the same validated artifact as the immutable confirmation.
    pub async fn prepare_creator_import_xlsx_preview_async(
        &self,
        bytes: Vec<u8>,
    ) -> Result<
        (
            crate::PrepareCreatorImportResult,
            Vec<CreatorTransferCreator>,
        ),
        crate::CreatorRepositoryCallError<String>,
    > {
        let artifact = self
            .repository_cpu_segment(move || {
                crate::creator_import::validated_artifact_from_xlsx(bytes).map(std::sync::Arc::new)
            })
            .await?;
        for _ in 0..=2 {
            let repository = std::sync::Arc::clone(&self.repository);
            let artifact_for_read = std::sync::Arc::clone(&artifact);
            let basis = self
                .repository_read_segment(move || {
                    repository.load_creator_import_basis(&artifact_for_read)
                })
                .await?;
            let artifact_for_plan = std::sync::Arc::clone(&artifact);
            let work = self
                .repository_cpu_segment(move || {
                    crate::creator_import::build_prepare_work(artifact_for_plan, basis)
                })
                .await?;
            let repository = std::sync::Arc::clone(&self.repository);
            let outcome = self
                .repository_write_segment(move || repository.write_creator_import_prepare(work))
                .await?;
            if matches!(
                outcome,
                crate::repository::PrepareImportWriteOutcome::BasisDrift
            ) {
                continue;
            }
            return self
                .repository_cpu_segment(move || {
                    crate::creator_import::decode_prepare_outcome(outcome)
                        .map(|result| (result, artifact.bundle.creators.clone()))
                })
                .await;
        }
        Err(crate::CreatorRepositoryCallError::Owner(
            "Creator import basis kept changing; prepare the bundle again".into(),
        ))
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::confirm_creator_import_async`] to own admission and dispatch.
    pub fn confirm_creator_import(
        &self,
        input: crate::ConfirmCreatorImportInput,
    ) -> Result<crate::ConfirmCreatorImportResult, String> {
        let (result, changed) = self.repository.confirm_creator_import(input)?;
        if changed {
            self.publish_changes([crate::CreatorChangeScope::Creators]);
        }
        Ok(result)
    }

    /// Confirm a prepared import through read/CPU/read/CPU/writer segments.
    /// The commit and its Creators invalidation share the supervised writer
    /// closure, so dropping the caller after dispatch cannot separate them.
    pub async fn confirm_creator_import_async(
        &self,
        input: crate::ConfirmCreatorImportInput,
    ) -> Result<crate::ConfirmCreatorImportResult, crate::CreatorRepositoryCallError<String>> {
        let repository = std::sync::Arc::clone(&self.repository);
        let input_for_read = input.clone();
        let operation = self
            .repository_read_segment(move || {
                repository.load_creator_import_operation(&input_for_read)
            })
            .await?;
        let (operation, terminal, artifact) = self
            .repository_cpu_segment(move || {
                let mut operation = operation;
                let terminal = operation.completed_result()?;
                let artifact = terminal
                    .is_none()
                    .then(|| operation.take_artifact())
                    .transpose()?
                    .map(std::sync::Arc::new);
                Ok((operation, terminal, artifact))
            })
            .await?;
        if let Some(result) = terminal {
            return Ok(result);
        }
        let artifact = artifact.ok_or_else(|| {
            crate::CreatorRepositoryCallError::Owner(
                "prepared Creator import has no staged bundle".to_string(),
            )
        })?;
        let repository = std::sync::Arc::clone(&self.repository);
        let artifact_for_read = std::sync::Arc::clone(&artifact);
        let basis = self
            .repository_read_segment(move || {
                repository.load_creator_import_basis(&artifact_for_read)
            })
            .await?;
        let work = self
            .repository_cpu_segment(move || {
                crate::creator_import::build_confirm_work(operation, artifact, basis)
            })
            .await?;
        let repository = std::sync::Arc::clone(&self.repository);
        let changes = self.changes.clone();
        let outcome = self
            .repository_write_segment(move || {
                let outcome = repository.write_creator_import_confirm(work);
                if outcome
                    .as_ref()
                    .is_ok_and(crate::repository::ConfirmImportWriteOutcome::should_publish)
                {
                    changes.publish([crate::CreatorChangeScope::Creators]);
                }
                outcome
            })
            .await?;
        let (result, _, _) = self
            .repository_cpu_segment(move || outcome.decode())
            .await?;
        Ok(result)
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::cancel_creator_import_async`] to own admission and dispatch.
    pub fn cancel_creator_import(
        &self,
        input: crate::ConfirmCreatorImportInput,
    ) -> Result<crate::ConfirmCreatorImportResult, String> {
        self.repository.cancel_creator_import(input)
    }

    pub async fn cancel_creator_import_async(
        &self,
        input: crate::ConfirmCreatorImportInput,
    ) -> Result<crate::ConfirmCreatorImportResult, crate::CreatorRepositoryCallError<String>> {
        let repository = std::sync::Arc::clone(&self.repository);
        self.repository_write_segment(move || repository.cancel_creator_import(input))
            .await
    }
}

#[cfg(test)]
mod import_tests;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        CREATOR_BUNDLE_MAX_SAFE_INTEGER, ContactChannel, CreatorBundle, CreatorTransferAccount,
        CreatorTransferConfidence, CreatorTransferContactRoute, CreatorTransferCreator,
        CreatorTransferLinkState, CreatorTransferPlatform, CreatorTransferProfile,
        CreatorTransferRelationship, CreatorTransferStage, CreatorTransferTargetKind, LocatorKind,
        SourceKind,
    };
    use serde_json::json;

    use super::{build_creator_bundle, encode_creator_bundle_with_limit, reject_v1_tiktok};

    fn creator(id: &str, reverse_accounts: bool) -> CreatorTransferCreator {
        let mut accounts = vec![
            CreatorTransferAccount {
                capture_order: 1,
                platform: CreatorTransferPlatform::Instagram,
                target_kind: CreatorTransferTargetKind::Creator,
                external_id: format!("ig-{id}"),
                link_state: CreatorTransferLinkState::Rejected,
                confidence: CreatorTransferConfidence::Possible,
                source_kind: SourceKind::ProfileBio,
                latest_profile: None,
            },
            CreatorTransferAccount {
                capture_order: 0,
                platform: CreatorTransferPlatform::X,
                target_kind: CreatorTransferTargetKind::User,
                external_id: format!("x-{id}"),
                link_state: CreatorTransferLinkState::Confirmed,
                confidence: CreatorTransferConfidence::Exact,
                source_kind: SourceKind::InitialPlatformAccount,
                latest_profile: Some(CreatorTransferProfile {
                    handle: Some(format!("{id}_new")),
                    display_name: Some(format!("Newest {id}")),
                    profile_url: Some(format!("https://x.com/{id}_new")),
                    avatar_url: Some(format!("https://cdn.example/{id}.png")),
                }),
            },
        ];
        if reverse_accounts {
            accounts.reverse();
        }
        CreatorTransferCreator {
            notes: None,
            tags: Vec::new(),
            semantic_sha256: String::new(),
            record_key: String::new(),
            display_name: format!("Creator {id}"),
            score: Some(87),
            follower_counts: BTreeMap::from([(CreatorTransferPlatform::X, 42)]),
            pinned: true,
            relationship: CreatorTransferRelationship {
                stage: CreatorTransferStage::Prospect,
            },
            accounts,
            contact_routes: vec![CreatorTransferContactRoute {
                save_order: 0,
                channel: ContactChannel::Email,
                locator_kind: LocatorKind::EmailAddress,
                normalized_value: format!("{id}@example.com"),
                display_value: format!("{id}@example.com"),
                confidence: CreatorTransferConfidence::Exact,
                source_kinds: vec![SourceKind::ProfileBio, SourceKind::ManualEntry],
            }],
        }
    }

    fn complete_fixture_bundle() -> CreatorBundle {
        build_creator_bundle(vec![creator("alpha", true)]).unwrap()
    }

    #[test]
    fn bundle_is_deterministic_and_has_the_exact_public_field_set() {
        let first = build_creator_bundle(vec![creator("b", false), creator("a", true)]).unwrap();
        let second = build_creator_bundle(vec![creator("a", false), creator("b", true)]).unwrap();
        assert_eq!(first, second);
        let value = serde_json::to_value(&first).unwrap();
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            [
                "content_sha256",
                "creator_count",
                "creators",
                "format",
                "format_version",
                "profile",
                "snapshot_kind",
            ]
        );
        assert_eq!(
            value["creators"][0]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            [
                "accounts",
                "contact_routes",
                "display_name",
                "follower_counts",
                "pinned",
                "record_key",
                "relationship",
                "score",
                "semantic_sha256",
                "tags",
            ]
        );
        assert_eq!(
            value["creators"][0]["accounts"][0]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            [
                "capture_order",
                "confidence",
                "external_id",
                "latest_profile",
                "link_state",
                "platform",
                "source_kind",
                "target_kind",
            ]
        );
        assert_eq!(
            value["creators"][0]["contact_routes"][0]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            [
                "channel",
                "confidence",
                "display_value",
                "locator_kind",
                "normalized_value",
                "save_order",
                "source_kinds",
            ]
        );
    }

    #[test]
    fn semantically_identical_manual_only_records_get_unique_bundle_local_keys() {
        let mut first = creator("manual-a", false);
        first.accounts.clear();
        first.contact_routes.clear();
        let mut second = first.clone();
        second.record_key.clear();
        second.semantic_sha256.clear();

        let bundle = build_creator_bundle(vec![first, second]).unwrap();
        assert_eq!(bundle.creators.len(), 2);
        assert_eq!(
            bundle.creators[0].semantic_sha256,
            bundle.creators[1].semantic_sha256
        );
        assert_ne!(bundle.creators[0].record_key, bundle.creators[1].record_key);
    }

    #[test]
    fn bundle_rejects_non_http_profile_urls_and_unsafe_integers() {
        let mut unsafe_url = creator("unsafe-url", false);
        unsafe_url.accounts[1]
            .latest_profile
            .as_mut()
            .unwrap()
            .profile_url = Some("file:///tmp/private".into());
        assert_eq!(
            build_creator_bundle(vec![unsafe_url]).unwrap_err(),
            "creator bundle field profile.profile_url must be an HTTP(S) URL without userinfo"
        );
        let mut unsafe_integer = creator("unsafe-integer", false);
        unsafe_integer.follower_counts.insert(
            CreatorTransferPlatform::YouTube,
            CREATOR_BUNDLE_MAX_SAFE_INTEGER + 1,
        );
        assert_eq!(
            build_creator_bundle(vec![unsafe_integer]).unwrap_err(),
            "creator bundle field follower_count exceeds the cross-language safe integer maximum"
        );
    }

    #[test]
    fn bundle_rejects_out_of_range_scores_invalid_record_keys_and_unsafe_profile_urls() {
        let mut invalid_score = creator("score", false);
        invalid_score.score = Some(101);
        assert_eq!(
            build_creator_bundle(vec![invalid_score]).unwrap_err(),
            "creator bundle score must be between 0 and 100"
        );
        let mut invalid_key = build_creator_bundle(vec![creator("ascii", false)]).unwrap();
        invalid_key.creators[0].record_key = "not-a-sha256-key".into();
        assert_eq!(
            invalid_key.to_json_bytes().unwrap_err(),
            "creator bundle field record_key must be a lowercase sha256 key"
        );
        let mut unsafe_avatar = creator("avatar", false);
        unsafe_avatar.accounts[1]
            .latest_profile
            .as_mut()
            .unwrap()
            .avatar_url = Some("https://user:secret@example.com/avatar.png".into());
        assert_eq!(
            build_creator_bundle(vec![unsafe_avatar]).unwrap_err(),
            "creator bundle field profile.avatar_url must be an HTTP(S) URL without userinfo"
        );
    }

    #[test]
    fn transfer_enums_have_a_frozen_closed_wire_set() {
        assert_eq!(
            serde_json::to_value([
                CreatorTransferPlatform::X,
                CreatorTransferPlatform::Instagram,
                CreatorTransferPlatform::Telegram,
                CreatorTransferPlatform::YouTube,
                CreatorTransferPlatform::TikTok,
            ])
            .unwrap(),
            json!(["x", "instagram", "telegram", "youtube", "tiktok"])
        );
        assert_eq!(
            serde_json::to_value([
                CreatorTransferLinkState::Claimed,
                CreatorTransferLinkState::Confirmed,
                CreatorTransferLinkState::Rejected,
            ])
            .unwrap(),
            json!(["claimed", "confirmed", "rejected"])
        );
    }

    #[test]
    fn account_sort_preserves_semantic_capture_order() {
        let mut value = creator("order", false);
        value.accounts.reverse();
        let bundle = build_creator_bundle(vec![value]).unwrap();
        assert_eq!(
            bundle.creators[0]
                .accounts
                .iter()
                .map(|account| account.capture_order)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }

    #[test]
    fn semantic_fact_changes_update_creator_and_bundle_digests() {
        let original = complete_fixture_bundle();
        let mut creators = original.creators.clone();
        creators[0].pinned = false;
        let changed = build_creator_bundle(creators).unwrap();
        assert_ne!(
            changed.creators[0].semantic_sha256,
            original.creators[0].semantic_sha256
        );
        assert_ne!(changed.content_sha256, original.content_sha256);
    }

    #[test]
    fn canonical_subset_reuses_exact_items_and_rejects_open_sets() {
        let bundle = build_creator_bundle(vec![creator("b", false), creator("a", true)]).unwrap();
        let keys = bundle
            .creators
            .iter()
            .map(|creator| creator.record_key.clone())
            .collect::<Vec<_>>();
        let selected = bundle.select_creators(&keys[..1]).unwrap();
        assert_eq!(selected, vec![bundle.creators[0].clone()]);
        assert_eq!(
            bundle
                .select_creators(&[keys[1].clone(), keys[0].clone()])
                .unwrap_err(),
            "creator subset record keys are not in canonical order"
        );
        assert_eq!(
            bundle
                .select_creators(&[keys[0].clone(), keys[0].clone()])
                .unwrap_err(),
            "creator subset record keys are not in canonical order"
        );
        assert_eq!(
            bundle
                .select_creators(&[format!("sha256:{}", "0".repeat(64))])
                .unwrap_err(),
            "creator subset contains a record key outside the source bundle"
        );
    }

    #[test]
    fn obsolete_reduced_shape_is_strictly_rejected() {
        let obsolete = br#"{"format":"reacher.creator_bundle","format_version":1,"profile":"portable_public","snapshot_kind":"full","creator_count":0,"content_sha256":"sha256:obsolete","creators":[]}
"#;
        assert!(CreatorBundle::from_json_bytes(obsolete).is_err());
    }

    #[test]
    fn portable_profile_requires_explicit_nulls_and_rejects_raw_provider_json() {
        let bundle = complete_fixture_bundle();
        let mut missing_avatar = serde_json::to_value(&bundle).unwrap();
        missing_avatar["creators"][0]["accounts"][0]["latest_profile"]
            .as_object_mut()
            .unwrap()
            .remove("avatar_url");
        assert!(
            CreatorBundle::from_json_bytes(&serde_json::to_vec(&missing_avatar).unwrap()).is_err()
        );

        let mut raw_provider_json = serde_json::to_value(&bundle).unwrap();
        raw_provider_json["creators"][0]["accounts"][0]["latest_profile"]
            .as_object_mut()
            .unwrap()
            .insert(
                "public_data".into(),
                json!({"accessToken": "must-not-export"}),
            );
        assert!(
            CreatorBundle::from_json_bytes(&serde_json::to_vec(&raw_provider_json).unwrap())
                .is_err()
        );
    }

    #[test]
    fn size_limit_applies_to_the_exact_artifact_bytes() {
        let bundle = complete_fixture_bundle();
        let exact = bundle.to_json_bytes().unwrap();
        assert_eq!(
            encode_creator_bundle_with_limit(&bundle, exact.len() - 1).unwrap_err(),
            format!(
                "creator bundle exceeds the maximum artifact size of {} bytes",
                exact.len() - 1
            )
        );
        assert_eq!(
            encode_creator_bundle_with_limit(&bundle, exact.len()).unwrap(),
            exact
        );
    }

    #[test]
    fn golden_bundle_and_digest_are_stable() {
        let fixture = include_str!("../tests/fixtures/creator_bundle_v3.json");
        let bundle = CreatorBundle::from_staged_import_json_bytes(fixture.as_bytes()).unwrap();
        assert_eq!(
            CreatorBundle::from_import_json_bytes(fixture.as_bytes()).unwrap(),
            complete_fixture_bundle()
        );
        assert_eq!(
            String::from_utf8(bundle.to_staged_import_json_bytes().unwrap()).unwrap(),
            fixture
        );
    }

    #[test]
    fn tagged_v3_golden_pins_unicode_tags_and_hashes() {
        let bytes = include_bytes!("../tests/fixtures/creator_bundle_tags_v3.json");
        let fixture = CreatorBundle::from_import_json_bytes(bytes).unwrap();
        let mut creators = complete_fixture_bundle().creators;
        for creator in &mut creators {
            creator.tags = vec!["美妆".into(), "𠮷".repeat(12), "Beauty".into()];
        }
        let bundle = build_creator_bundle(creators).unwrap();
        assert_eq!(bundle, fixture);
        assert_eq!(
            CreatorBundle::from_staged_import_json_bytes(bytes)
                .unwrap()
                .to_staged_import_json_bytes()
                .unwrap(),
            bytes
        );
        assert_eq!(super::historical_import_digests(&bundle).unwrap().len(), 0);
    }

    #[test]
    fn public_source_notes_require_v4_without_changing_released_v3() {
        let old = complete_fixture_bundle();
        assert_eq!(old.format_version, 3);
        let mut creators = old.creators;
        creators[0].notes = Some("Library 原始评分说明".into());
        let bundle = build_creator_bundle(creators).unwrap();
        assert_eq!(bundle.format_version, 4);
        let bytes = bundle.to_json_bytes().unwrap();
        assert_eq!(
            CreatorBundle::from_import_json_bytes(&bytes).unwrap(),
            bundle
        );
        assert_eq!(
            CreatorBundle::from_staged_import_json_bytes(&bytes).unwrap(),
            bundle
        );
        assert!(
            super::historical_import_digests(&bundle)
                .unwrap()
                .is_empty()
        );
        let mut mislabeled = bundle;
        mislabeled.format_version = 3;
        assert!(mislabeled.to_json_bytes().is_err());
    }

    #[test]
    fn frozen_v2_bundle_is_verified_before_explicit_conversion() {
        let bytes = include_bytes!("../tests/fixtures/creator_bundle_v2.json");
        assert!(CreatorBundle::from_json_bytes(bytes).is_err());
        let legacy = CreatorBundle::from_staged_import_json_bytes(bytes).unwrap();
        assert_eq!(legacy.to_staged_import_json_bytes().unwrap(), bytes);
        assert_eq!(
            CreatorBundle::from_import_json_bytes(bytes).unwrap(),
            complete_fixture_bundle()
        );
    }
    #[test]
    fn legacy_v1_is_verified_and_explicitly_converted() {
        let bytes = include_bytes!("../tests/fixtures/creator_bundle.json");
        assert!(CreatorBundle::from_json_bytes(bytes).is_err());
        let converted = CreatorBundle::from_import_json_bytes(bytes).unwrap();
        assert_eq!(converted, complete_fixture_bundle());
        let mut corrupted = CreatorBundle::from_staged_import_json_bytes(bytes).unwrap();
        corrupted.content_sha256 = format!("sha256:{}", "0".repeat(64));
        let mut bytes = serde_json::to_string(&corrupted)
            .unwrap()
            .replace(",\"tags\":[]", "")
            .into_bytes();
        bytes.push(b'\n');
        assert!(CreatorBundle::from_import_json_bytes(&bytes).is_err());
        corrupted.creators[0]
            .follower_counts
            .insert(CreatorTransferPlatform::TikTok, 1);
        assert!(reject_v1_tiktok(&corrupted.creators).is_err());
    }

    #[test]
    fn tiktok_bundle_preserves_scoped_identity_and_followers() {
        let mut creator = creator("TikTok", false);
        creator.accounts[0].platform = CreatorTransferPlatform::TikTok;
        creator.accounts[0].external_id = "tto:123456".into();
        creator
            .follower_counts
            .insert(CreatorTransferPlatform::TikTok, 500);
        let bundle = build_creator_bundle(vec![creator.clone()]).unwrap();
        assert_eq!(
            CreatorBundle::from_json_bytes(&bundle.to_json_bytes().unwrap()).unwrap(),
            bundle
        );
        creator.accounts[0].external_id = "123456".into();
        assert!(build_creator_bundle(vec![creator]).is_err());
    }
}
