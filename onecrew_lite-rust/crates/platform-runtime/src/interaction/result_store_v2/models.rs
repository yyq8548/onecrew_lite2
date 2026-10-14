#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutedBatch {
    #[schemars(length(min = 1, max = 1000))]
    pub outcomes: Vec<ExecutedCallOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutedCallOutcome {
    #[schemars(length(min = 1, max = 128))]
    pub call_id: String,
    #[schemars(length(min = 1, max = 128))]
    pub operation_id: String,
    pub disposition: CallDisposition,
    #[schemars(length(min = 1, max = 8192))]
    pub summary: String,
    // One outcome yields at most one saved Creator per reviewed row, but this
    // frame is shared with ordinary calls that keep the generic 64-ref budget.
    // The wire schema must cover the largest frame the store accepts; a
    // narrower bound would reject a generic outcome the runtime allows.
    #[schemars(length(max = super::MAX_OWNER_FRAME_RESOURCE_REFS))]
    pub resource_refs: Vec<ResultResourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CallDisposition {
    Succeeded,
    Failed {
        #[schemars(length(min = 1, max = 128))]
        code: String,
    },
    Unknown {
        #[schemars(length(min = 1, max = 128))]
        code: String,
    },
    NotRun {
        #[schemars(length(min = 1, max = 8192))]
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultResourceRef {
    pub kind: ResultResourceKind,
    #[schemars(length(min = 1, max = 256))]
    pub id: String,
    #[schemars(length(min = 1, max = 8192))]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ResultResourceKind {
    Creator,
    Conversation,
    Connection,
    ProviderObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationProjection {
    pub kind: ResultPresentationKind,
    #[schemars(length(min = 1, max = 8192))]
    pub title: String,
    #[schemars(length(min = 1, max = 8192))]
    pub summary: Option<String>,
    // The wire schema covers the largest supported projection; contextual
    // validation keeps ordinary projections at the smaller generic limit.
    #[schemars(length(max = super::MAX_OWNER_BATCH_SECTIONS))]
    pub sections: Vec<PresentationSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresentationSection {
    CreatorSaveRows {
        rows: Vec<CreatorSaveBatchOutcome>,
    },
    Fields {
        #[schemars(length(min = 1, max = 8192))]
        heading: Option<String>,
        #[schemars(length(min = 1, max = 128))]
        fields: Vec<PresentationField>,
    },
    Items {
        #[schemars(length(min = 1, max = 8192))]
        heading: Option<String>,
        #[schemars(length(min = 1, max = 128))]
        items: Vec<PresentationItem>,
    },
    Notice {
        #[schemars(length(min = 1, max = 8192))]
        heading: Option<String>,
        #[schemars(length(min = 1, max = 8192))]
        body: String,
        tone: NoticeTone,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationField {
    #[schemars(length(min = 1, max = 8192))]
    pub label: String,
    #[schemars(length(min = 1, max = 8192))]
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationItem {
    #[schemars(length(min = 1, max = 8192))]
    pub title: String,
    #[schemars(length(min = 1, max = 8192))]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum NoticeTone {
    Neutral,
    Success,
    Warning,
    Critical,
}

pub struct OwnerActionPayload {
    bytes: Vec<u8>,
    authority: OwnerAuthority,
    review_context: Option<super::OwnerReviewContext>,
    peer: Option<super::PeerResumePayload>,
}

enum OwnerAuthority {
    Generic,
    PreparedCreatorSave(Box<crate::creator_save::PreparedSaveAction>),
    SelectedCreatorSave(Box<crate::creator_save::SelectedSaveAction>),
    PreparedOutreach(Box<crate::outreach::PreparedOutreachAction>),
    PreparedEmail(Box<crate::email::PreparedEmailAction>),
    PreparedCreatorBatch(Box<crate::creator_batch::PreparedCreatorBatchAction>),
}

impl PartialEq for OwnerActionPayload {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
            && std::mem::discriminant(&self.authority) == std::mem::discriminant(&other.authority)
            && self.review_context == other.review_context
            && self.peer == other.peer
    }
}
impl Eq for OwnerActionPayload {}

impl OwnerActionPayload {
    #[cfg(feature = "test-support")]
    pub(super) fn expire_creator_save_fixture(&mut self) -> Result<(), ResultStoreError> {
        match &mut self.authority {
            OwnerAuthority::PreparedCreatorSave(action) => {
                action.deadline = std::time::Instant::now();
                Ok(())
            }
            _ => Err(ResultStoreError::InvalidState(
                "creator_save_review_required",
            )),
        }
    }

    /// Pin the exact review deadline an in-crate test asked for. Production
    /// sealing derives its deadline from a lifetime, so a test asserting
    /// behaviour exactly at an absolute instant needs that instant rather than
    /// one a few microseconds later. This can only move the deadline; it
    /// changes no reviewed content, target or receipt.
    #[cfg(test)]
    pub(super) fn set_creator_save_deadline_for_test(
        &mut self,
        deadline: std::time::Instant,
    ) -> Result<(), ResultStoreError> {
        match &mut self.authority {
            OwnerAuthority::PreparedCreatorSave(action) => {
                action.deadline = deadline;
                Ok(())
            }
            _ => Err(ResultStoreError::InvalidState(
                "creator_save_review_required",
            )),
        }
    }

    #[cfg(test)]
    pub(super) fn into_prepared_save_for_test(self) -> crate::creator_save::PreparedSaveAction {
        match self.authority {
            OwnerAuthority::PreparedCreatorSave(action) => *action,
            _ => panic!("real Creator save fixture authority required"),
        }
    }

    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, ResultStoreError> {
        let bytes = bytes.into();
        if bytes.is_empty() || bytes.len() > MAX_OWNER_ACTION_BYTES {
            return Err(ResultStoreError::InvalidInput("owner_action_payload"));
        }
        Ok(Self {
            bytes,
            authority: OwnerAuthority::Generic,
            review_context: None,
            peer: None,
        })
    }

    /// Carry a complete bounded peer batch for a retained generic attended
    /// operation. This creates no typed business authority and cannot be used
    /// to manufacture a submitted Creator or Outreach grant.
    pub fn from_peer_resume(peer: super::PeerResumePayload) -> Self {
        Self {
            bytes: Vec::new(),
            authority: OwnerAuthority::Generic,
            review_context: None,
            peer: Some(peer),
        }
    }

    /// Only typed, validated search plans may use the larger payload budget.
    pub fn from_search_plan(
        draft: &crate::SearchPlanEditorDraft,
    ) -> Result<Self, ResultStoreError> {
        let bytes = serde_json::to_vec(draft)
            .map_err(|_| ResultStoreError::InvalidInput("search_plan_payload"))?;
        if bytes.len() > crate::SEARCH_PLAN_MAX_BYTES {
            return Err(ResultStoreError::InvalidInput(
                "search_plan_payload_too_large",
            ));
        }
        draft
            .validate()
            .map_err(|_| ResultStoreError::InvalidInput("search_plan"))?;
        Ok(Self {
            bytes,
            authority: OwnerAuthority::Generic,
            review_context: None,
            peer: None,
        })
    }

    /// Generic continuations cannot consume business authority as bytes.
    pub fn into_generic_bytes(self) -> Result<Vec<u8>, ResultStoreError> {
        match self.authority {
            OwnerAuthority::Generic => Ok(self.peer.map_or(self.bytes, |peer| peer.into_bytes())),
            _ => Err(ResultStoreError::InvalidState(
                "typed_owner_review_required",
            )),
        }
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn digest(&self) -> String {
        // Retain generic/search-plan identity. Business authority includes its
        // tag, exact typed encoding, trusted call context and opaque peer bytes.
        if matches!(self.authority, OwnerAuthority::Generic) {
            let bytes = self
                .peer
                .as_ref()
                .map_or(self.bytes.as_slice(), |peer| peer.as_bytes());
            return format!("sha256:{:x}", Sha256::digest(bytes));
        }
        let mut hash = Sha256::new();
        let tag: &[u8] = match self.authority {
            OwnerAuthority::PreparedCreatorSave(_) | OwnerAuthority::SelectedCreatorSave(_) => {
                b"creator_save_v1"
            }
            OwnerAuthority::PreparedOutreach(_) => b"outreach_v1",
            OwnerAuthority::PreparedEmail(_) => b"email_v1",
            OwnerAuthority::PreparedCreatorBatch(_) => b"creator_batch_v1",
            OwnerAuthority::Generic => unreachable!(),
        };
        for bytes in [tag, self.bytes.as_slice()] {
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
        // Context is a fixed-field typed structure containing only identifiers.
        let context =
            serde_json::to_vec(&self.review_context).expect("owner context serialization");
        hash.update((context.len() as u64).to_be_bytes());
        hash.update(context);
        let peer = self.peer.as_ref().map_or(&[][..], |peer| peer.as_bytes());
        hash.update((peer.len() as u64).to_be_bytes());
        hash.update(peer);
        format!("sha256:{:x}", hash.finalize())
    }

    pub(crate) fn from_prepared_creator_save(
        action: crate::creator_save::PreparedSaveAction,
    ) -> Result<Self, ResultStoreError> {
        let mut bytes = serde_json::to_vec(&(
            &action.review,
            &action.context.origin,
            &action.context.call_id,
            &action.context.operation_id,
        ))
        .map_err(|_| ResultStoreError::InvalidInput("creator_save_payload"))?;
        bytes.push(b'\n');
        bytes.extend(
            action
                .prepared
                .encoded_payload()
                .map_err(|_| ResultStoreError::InvalidInput("creator_save_payload"))?,
        );
        if bytes.len() > super::MAX_CREATOR_SAVE_BATCH_OWNER_ACTION_BYTES {
            return Err(ResultStoreError::InvalidInput(
                "creator_save_payload_too_large",
            ));
        }
        Ok(Self {
            bytes,
            authority: OwnerAuthority::PreparedCreatorSave(Box::new(action)),
            review_context: None,
            peer: None,
        })
    }

    pub(super) fn validate_creator_save_review(
        &self,
        review: &CreatorSaveBatchReview,
    ) -> Result<(), ResultStoreError> {
        match &self.authority {
            OwnerAuthority::PreparedCreatorSave(action) if &action.review == review => Ok(()),
            _ => Err(ResultStoreError::InvalidInput(
                "trusted_creator_save_payload_required",
            )),
        }
    }

    pub(super) fn validate_creator_save_context(
        &self,
        origin: &ResultOrigin,
        batch: &ExecutedBatch,
    ) -> Result<(), ResultStoreError> {
        let OwnerAuthority::PreparedCreatorSave(action) = &self.authority else {
            return Err(ResultStoreError::InvalidInput(
                "trusted_creator_save_payload_required",
            ));
        };
        let call_matches = self.review_context.as_ref().is_some_and(|context| {
            context.validate_frame(batch).is_ok()
                && context.origin() == origin
                && context.call_id() == action.context.call_id
                && context.operation_id() == action.context.operation_id
        });
        if &action.context.origin != origin || !call_matches {
            return Err(ResultStoreError::InvalidInput(
                "creator_save_context_mismatch",
            ));
        }
        Ok(())
    }

    pub(super) fn prepared_deadline(&self) -> Option<std::time::Instant> {
        match &self.authority {
            OwnerAuthority::PreparedCreatorSave(action) => Some(action.deadline),
            OwnerAuthority::PreparedOutreach(action) => Some(action.deadline()),
            OwnerAuthority::PreparedEmail(action) => Some(action.deadline()),
            OwnerAuthority::PreparedCreatorBatch(action) => Some(action.deadline()),
            _ => None,
        }
    }

    pub(super) fn select_creator_save(
        &self,
        selections: &[CreatorSaveBatchSelection],
    ) -> Result<crate::creator_save::SelectedSaveAction, ResultStoreError> {
        let OwnerAuthority::PreparedCreatorSave(prepared) = &self.authority else {
            return Err(ResultStoreError::InvalidState(
                "trusted_creator_save_payload_required",
            ));
        };
        prepared.select(selections)
    }

    /// Selection was validated while authority was still in the locked record.
    /// Move its potentially large peer envelope into the grant without cloning.
    pub(super) fn with_selected_creator_save(
        self,
        selected: crate::creator_save::SelectedSaveAction,
    ) -> Self {
        debug_assert!(matches!(
            self.authority,
            OwnerAuthority::PreparedCreatorSave(_)
        ));
        Self {
            bytes: self.bytes,
            authority: OwnerAuthority::SelectedCreatorSave(Box::new(selected)),
            review_context: self.review_context,
            peer: self.peer,
        }
    }

    pub(crate) fn into_selected_creator_save(
        self,
    ) -> Result<crate::creator_save::SelectedSaveAction, ResultStoreError> {
        match self.authority {
            OwnerAuthority::SelectedCreatorSave(action) => Ok(*action),
            _ => Err(ResultStoreError::InvalidState(
                "submitted_creator_save_required",
            )),
        }
    }

    pub(crate) fn from_prepared_outreach(
        action: crate::outreach::PreparedOutreachAction,
    ) -> Result<Self, ResultStoreError> {
        let bytes = action
            .encoded_payload()
            .map_err(|_| ResultStoreError::InvalidInput("outreach_payload_encoding"))?;
        if bytes.is_empty() || bytes.len() > super::MAX_OUTREACH_OWNER_ACTION_BYTES {
            return Err(ResultStoreError::InvalidInput("outreach_payload_size"));
        }
        Ok(Self {
            bytes,
            authority: OwnerAuthority::PreparedOutreach(Box::new(action)),
            review_context: None,
            peer: None,
        })
    }

    pub(super) fn into_prepared_outreach(
        self,
    ) -> Result<crate::outreach::PreparedOutreachAction, ResultStoreError> {
        match self.authority {
            OwnerAuthority::PreparedOutreach(action) => Ok(*action),
            _ => Err(ResultStoreError::InvalidState(
                "submitted_outreach_required",
            )),
        }
    }

    pub(crate) fn from_prepared_email(
        action: crate::email::PreparedEmailAction,
    ) -> Result<Self, ResultStoreError> {
        let bytes = action
            .encoded_payload()
            .map_err(|_| ResultStoreError::InvalidInput("email_payload_encoding"))?;
        if bytes.is_empty() || bytes.len() > super::MAX_EMAIL_OWNER_ACTION_BYTES {
            return Err(ResultStoreError::InvalidInput("email_payload_size"));
        }
        Ok(Self {
            bytes,
            authority: OwnerAuthority::PreparedEmail(Box::new(action)),
            review_context: None,
            peer: None,
        })
    }

    pub(super) fn into_prepared_email(
        self,
    ) -> Result<crate::email::PreparedEmailAction, ResultStoreError> {
        match self.authority {
            OwnerAuthority::PreparedEmail(action) => Ok(*action),
            _ => Err(ResultStoreError::InvalidState("submitted_email_required")),
        }
    }

    pub(crate) fn from_prepared_creator_batch(
        action: crate::creator_batch::PreparedCreatorBatchAction,
    ) -> Result<Self, ResultStoreError> {
        let bytes = action
            .encoded_payload()
            .map_err(|_| ResultStoreError::InvalidInput("creator_batch_payload_encoding"))?;
        if bytes.is_empty() || bytes.len() > super::MAX_CREATOR_BATCH_OWNER_ACTION_BYTES {
            return Err(ResultStoreError::InvalidInput("creator_batch_payload_size"));
        }
        Ok(Self {
            bytes,
            authority: OwnerAuthority::PreparedCreatorBatch(Box::new(action)),
            review_context: None,
            peer: None,
        })
    }

    pub(super) fn into_prepared_creator_batch(
        self,
    ) -> Result<crate::creator_batch::PreparedCreatorBatchAction, ResultStoreError> {
        match self.authority {
            OwnerAuthority::PreparedCreatorBatch(action) => Ok(*action),
            _ => Err(ResultStoreError::InvalidState(
                "submitted_creator_batch_required",
            )),
        }
    }

    pub(super) fn owner_review_kind(&self) -> Option<super::OwnerReviewKind> {
        match self.authority {
            OwnerAuthority::PreparedCreatorSave(_) | OwnerAuthority::SelectedCreatorSave(_) => {
                Some(super::OwnerReviewKind::CreatorSave)
            }
            OwnerAuthority::PreparedOutreach(_) => Some(super::OwnerReviewKind::Outreach),
            OwnerAuthority::PreparedEmail(_) => Some(super::OwnerReviewKind::Email),
            OwnerAuthority::PreparedCreatorBatch(_) => Some(super::OwnerReviewKind::CreatorBatch),
            OwnerAuthority::Generic => None,
        }
    }

    pub(super) fn owner_review_context(&self) -> Option<&super::OwnerReviewContext> {
        self.review_context.as_ref()
    }

    pub(super) fn peer_resume(&self) -> Option<&super::PeerResumePayload> {
        self.peer.as_ref()
    }

    pub(super) fn bind_owner_review(
        &mut self,
        context: super::OwnerReviewContext,
        peer: Option<super::PeerResumePayload>,
    ) -> Result<(), ResultStoreError> {
        if self.owner_review_kind().is_none() || self.review_context.is_some() {
            return Err(ResultStoreError::InvalidState(
                "prepared_owner_review_required",
            ));
        }
        if self
            .prepared_deadline()
            .is_none_or(|deadline| std::time::Instant::now() >= deadline)
        {
            return Err(ResultStoreError::Expired);
        }
        self.review_context = Some(context);
        self.peer = peer;
        Ok(())
    }

    pub(super) fn validate_owner_review_context(
        &self,
        origin: &ResultOrigin,
        batch: &ExecutedBatch,
    ) -> Result<(), ResultStoreError> {
        if let Some(context) = &self.review_context {
            if context.origin() != origin || context.call_digest().is_some() != self.peer.is_some()
            {
                return Err(ResultStoreError::InvalidInput(
                    "owner_review_context_mismatch",
                ));
            }
            context.validate_frame(batch)?;
        } else if matches!(self.authority, OwnerAuthority::PreparedOutreach(_)) {
            return Err(ResultStoreError::InvalidInput(
                "outreach_review_context_required",
            ));
        } else if matches!(self.authority, OwnerAuthority::PreparedEmail(_)) {
            return Err(ResultStoreError::InvalidInput(
                "email_review_context_required",
            ));
        } else if matches!(self.authority, OwnerAuthority::PreparedCreatorBatch(_)) {
            return Err(ResultStoreError::InvalidInput(
                "creator_batch_review_context_required",
            ));
        }
        Ok(())
    }

    pub(super) fn held_bytes(&self) -> Result<usize, ResultStoreError> {
        let context = self
            .review_context
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|_| ResultStoreError::Unavailable("owner_context_size"))?;
        self.bytes
            .len()
            .checked_add(context.as_ref().map_or(0, Vec::len))
            .and_then(|bytes| {
                bytes.checked_add(self.peer.as_ref().map_or(0, |peer| peer.as_bytes().len()))
            })
            .ok_or(ResultStoreError::Capacity("owner_payload_bytes"))
    }

    pub(super) fn confirmation_payload_limit(&self) -> usize {
        if matches!(self.authority, OwnerAuthority::Generic) && self.peer.is_some() {
            super::MAX_PEER_RESUME_BYTES
        } else if matches!(self.authority, OwnerAuthority::PreparedOutreach(_)) {
            super::MAX_OUTREACH_OWNER_ACTION_BYTES
        } else if matches!(self.authority, OwnerAuthority::PreparedEmail(_)) {
            super::MAX_EMAIL_OWNER_ACTION_BYTES
        } else if matches!(self.authority, OwnerAuthority::PreparedCreatorBatch(_)) {
            super::MAX_CREATOR_BATCH_OWNER_ACTION_BYTES
        } else {
            super::MAX_OWNER_ACTION_BYTES
        }
    }

    pub(super) fn action_bytes_len(&self) -> usize {
        if matches!(self.authority, OwnerAuthority::Generic) {
            self.peer
                .as_ref()
                .map_or(self.bytes.len(), |peer| peer.as_bytes().len())
        } else {
            self.bytes.len()
        }
    }
}

impl fmt::Debug for OwnerActionPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OwnerActionPayload")
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSavePreview {
    pub display_name: Option<String>,
    pub platform: onecrew_creator_runtime::Platform,
    pub handle: Option<String>,
    pub profile_url: Option<String>,
    /// Public content evidence carried by the prepared selection, not authority.
    pub source_url: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub identity_status: String,
    pub identity_creator_ids: Vec<String>,
    pub intent: onecrew_creator_runtime::CreatorSaveIntent,
    pub status: onecrew_creator_runtime::CreatorSavePreparedStatus,
    pub reason: Option<onecrew_creator_runtime::CreatorSavePreparedReason>,
    pub notes: Option<String>,
    pub score: Option<u8>,
    pub assessment: Option<String>,
    pub follower_counts: std::collections::BTreeMap<String, u64>,
    #[schemars(length(min = 1, max = 128))]
    pub routes: Vec<CreatorSaveRoute>,
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveRoute {
    pub route_id: String,
    pub channel: String,
    pub kind: String,
    pub display_value: String,
    pub normalized_value: String,
    pub sources: Vec<String>,
    pub required: bool,
    pub collision: CreatorSaveCollision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveCollision {
    pub status: CreatorSaveCollisionStatus,
    pub creator_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSaveCollisionStatus {
    None,
    Possible,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveBatchReview {
    #[schemars(length(min = 1, max = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS))]
    pub rows: Vec<CreatorSaveBatchRow>,
    #[schemars(length(max = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS))]
    pub failures: Vec<CreatorSavePreparedFailure>,
    pub intent: onecrew_creator_runtime::CreatorSaveIntent,
    pub summary: onecrew_creator_runtime::CreatorSaveBatchSummary,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveBatchRow {
    pub row_id: String,
    pub preview: Box<CreatorSavePreview>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSavePreparedFailure {
    #[schemars(length(min = 1, max = 128))]
    pub row_id: String,
    #[schemars(length(max = 8192))]
    pub display_name: Option<String>,
    pub platform: onecrew_creator_runtime::Platform,
    #[schemars(length(max = 8192))]
    pub handle: Option<String>,
    #[schemars(length(max = 8192))]
    pub profile_url: Option<String>,
    pub reason: onecrew_creator_runtime::CreatorSavePreparedReason,
}

/// A bounded summary safe for model-visible review receipts. It intentionally
/// contains no contact values, Creator IDs, or provider payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveReviewSummary {
    pub summary: onecrew_creator_runtime::CreatorSaveBatchSummary,
    #[schemars(length(max = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS))]
    pub rows: Vec<CreatorSaveReviewRowSummary>,
    #[schemars(length(max = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS))]
    pub failures: Vec<CreatorSaveReviewFailureSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveReviewRowSummary {
    #[schemars(length(min = 1, max = 128))]
    pub row_id: String,
    pub status: onecrew_creator_runtime::CreatorSavePreparedStatus,
    pub reason: Option<onecrew_creator_runtime::CreatorSavePreparedReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveReviewFailureSummary {
    #[schemars(length(min = 1, max = 128))]
    pub row_id: String,
    pub reason: onecrew_creator_runtime::CreatorSavePreparedReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveBatchSelection {
    pub row_id: String,
    #[schemars(length(min = 1, max = 128))]
    pub confirmed_route_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSaveBatchRowStatus {
    Saved,
    Failed,
    Unknown,
    NotRun,
    NotSelected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// The durable effect classification for one confirmed Creator row.
pub enum CreatorSaveBatchOperation {
    Added,
    Updated,
    ExistingSkipped,
    DuplicateSkipped,
    NotSelected,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// A bounded, secret-free explanation for a row outcome.
pub enum CreatorSaveBatchReason {
    ExistingCreator,
    DuplicateSelection,
    IdentityAmbiguous,
    RequiredContactRouteConflict,
    ConflictingUpdate,
    PreparationFailed,
    UserNotSelected,
    RowFailed,
    CommitUnknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// Counts for all rows represented by one Creator save completion.
pub struct CreatorSaveResultSummary {
    pub total: usize,
    pub added: usize,
    pub updated: usize,
    pub existing_skipped: usize,
    pub duplicate_skipped: usize,
    pub not_selected: usize,
    pub failed: usize,
    pub unknown: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveBatchOutcome {
    pub row_id: String,
    pub display_name: Option<String>,
    pub platform: onecrew_creator_runtime::Platform,
    pub handle: Option<String>,
    pub profile_url: Option<String>,
    pub avatar_url: Option<String>,
    pub status: CreatorSaveBatchRowStatus,
    pub operation: CreatorSaveBatchOperation,
    pub reason: Option<CreatorSaveBatchReason>,
    pub creator_id: Option<String>,
    pub failure_code: Option<String>,
    pub confirmed_route_ids: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PendingUserAction {
    CreatorSaveBatch {
        review: Box<CreatorSaveBatchReview>,
        payload: OwnerActionPayload,
    },
    SearchPlan {
        draft: crate::SearchPlanEditorDraft,
        payload: OwnerActionPayload,
    },
    Confirm {
        prompt: String,
        label: String,
        payload: OwnerActionPayload,
    },
    Choice {
        questions: Vec<ResultChoiceQuestion>,
        payload: OwnerActionPayload,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResultChoiceSelectionMode {
    #[default]
    Single,
    Multiple,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultChoiceQuestion {
    #[serde(default)]
    pub selection_mode: ResultChoiceSelectionMode,
    #[schemars(length(min = 1, max = 128))]
    pub id: String,
    #[schemars(length(min = 1, max = 8192))]
    pub header: String,
    #[schemars(length(min = 1, max = 8192))]
    pub prompt: String,
    #[schemars(length(min = 1, max = 32))]
    pub options: Vec<ResultChoiceOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultChoiceOption {
    #[schemars(length(min = 1, max = 128))]
    pub id: String,
    #[schemars(length(min = 1, max = 8192))]
    pub label: String,
    #[schemars(length(min = 1, max = 8192))]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum ResultChoiceAnswer {
    Options {
        #[schemars(length(min = 1, max = 128))]
        question_id: String,
        #[schemars(length(min = 1, max = 32))]
        option_ids: Vec<String>,
    },
    Option {
        #[schemars(length(min = 1, max = 128))]
        question_id: String,
        #[schemars(length(min = 1, max = 128))]
        option_id: String,
    },
    Custom {
        #[schemars(length(min = 1, max = 128))]
        question_id: String,
        #[schemars(length(min = 1, max = 2048))]
        text: String,
    },
}

impl ResultChoiceAnswer {
    pub fn question_id(&self) -> &str {
        match self {
            Self::Options { question_id, .. }
            | Self::Option { question_id, .. }
            | Self::Custom { question_id, .. } => question_id,
        }
    }

    /// Shared shape validation for selection, continuation, and peer decoding.
    pub fn is_well_formed(&self) -> bool {
        super::validation::validate_choice_answer(self).is_ok()
    }

    /// Custom input is a clarification answer, never authority for another operation.
    pub fn matches_question(&self, question: &ResultChoiceQuestion) -> bool {
        self.question_id() == question.id
            && self.is_well_formed()
            && match self {
                Self::Options { option_ids, .. } => {
                    question.selection_mode == ResultChoiceSelectionMode::Multiple
                        && option_ids.len() <= question.options.len()
                        && option_ids
                            .iter()
                            .all(|id| question.options.iter().any(|option| &option.id == id))
                }
                Self::Option { option_id, .. } => {
                    question.selection_mode == ResultChoiceSelectionMode::Single
                        && question
                            .options
                            .iter()
                            .any(|option| option.id == *option_id)
                }
                Self::Custom { .. } => true,
            }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PendingActionView {
    CreatorSaveBatch {
        review: Box<CreatorSaveBatchReview>,
    },
    SearchPlan {
        draft: crate::SearchPlanEditorDraft,
        draft_digest: String,
    },
    Confirm {
        #[schemars(length(min = 1, max = 8192))]
        prompt: String,
        #[schemars(length(min = 1, max = 8192))]
        label: String,
    },
    Choice {
        #[schemars(length(min = 1, max = 32))]
        questions: Vec<ResultChoiceQuestion>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InitialResultState {
    Available,
    Resolved(ResultResolution),
    Failed(ResultFailure),
    Unknown(ResultUnknown),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResultState {
    Available,
    Submitted { action: SubmittedActionView },
    Resolved(ResultResolution),
    Failed(ResultFailure),
    Unknown(ResultUnknown),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SubmittedActionView {
    Confirmed {},
    Selected {
        #[schemars(length(min = 1, max = 32))]
        answers: Vec<ResultChoiceAnswer>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultResolution {
    #[schemars(length(min = 1, max = 128))]
    pub code: String,
    #[schemars(length(min = 1, max = 8192))]
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultFailure {
    #[schemars(length(min = 1, max = 128))]
    pub code: String,
    #[schemars(length(min = 1, max = 8192))]
    pub summary: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultUnknown {
    #[schemars(length(min = 1, max = 128))]
    pub code: String,
    #[schemars(length(min = 1, max = 8192))]
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalResult {
    Resolved(ResultResolution),
    Failed(ResultFailure),
    Unknown(ResultUnknown),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultTerminalUpdate {
    pub terminal: TerminalResult,
    pub batch: ExecutedBatch,
    pub presentation: PresentationProjection,
    pub continuation: Option<ResultContinuationRequest>,
}

#[derive(Debug)]
pub struct ExecutedResultInput {
    pub origin: ResultOrigin,
    pub view_kind: ResultViewKind,
    pub batch: ExecutedBatch,
    pub presentation: PresentationProjection,
    pub pending_action: Option<PendingUserAction>,
    pub initial_state: InitialResultState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultSnapshot {
    /// Display-only preference; excluded from result identity and user authority.
    pub locale: crate::PresentationLocale,
    pub binding: ResultBinding,
    pub presented: bool,
    pub batch: ExecutedBatch,
    pub presentation: PresentationProjection,
    /// Exact completed prefix inside presentation.sections; the current review
    /// follows it. This is display state and never grants execution authority.
    #[schemars(range(min = 0, max = super::MAX_OWNER_BATCH_SECTIONS))]
    pub completed_sections_count: usize,
    pub pending_action: Option<PendingActionView>,
    pub state: ResultState,
    pub continuation: ResultContinuationSnapshot,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SubmittedOwnerAction {
    pub(crate) binding: ResultBinding,
    pub(crate) action: SubmittedActionView,
    pub(crate) owner_payload: OwnerActionPayload,
}

impl SubmittedOwnerAction {
    pub fn binding(&self) -> &ResultBinding {
        &self.binding
    }
    pub fn action(&self) -> &SubmittedActionView {
        &self.action
    }
    pub fn into_generic_parts(
        self,
    ) -> Result<(ResultBinding, SubmittedActionView, Vec<u8>), ResultStoreError> {
        Ok((
            self.binding,
            self.action,
            self.owner_payload.into_generic_bytes()?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultStoreError {
    Absent,
    Expired,
    ViewMismatch,
    OriginMismatch,
    FingerprintMismatch,
    RevisionConflict { current_revision: u64 },
    ContinuationRevisionConflict { current_revision: u64 },
    ContinuationLeaseMismatch,
    InvalidInput(&'static str),
    InvalidState(&'static str),
    Replay(&'static str),
    Capacity(&'static str),
    Unavailable(&'static str),
}

impl fmt::Display for ResultStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => formatter.write_str("result state is absent"),
            Self::Expired => formatter.write_str("result state has expired"),
            Self::ViewMismatch => formatter.write_str("result view binding mismatch"),
            Self::OriginMismatch => formatter.write_str("result origin binding mismatch"),
            Self::FingerprintMismatch => formatter.write_str("result fingerprint binding mismatch"),
            Self::RevisionConflict { current_revision } => {
                write!(formatter, "result revision conflict at {current_revision}")
            }
            Self::ContinuationRevisionConflict { current_revision } => write!(
                formatter,
                "result continuation revision conflict at {current_revision}"
            ),
            Self::ContinuationLeaseMismatch => {
                formatter.write_str("result continuation lease mismatch")
            }
            Self::InvalidInput(code) => write!(formatter, "result input rejected ({code})"),
            Self::InvalidState(code) => write!(formatter, "result state rejected ({code})"),
            Self::Replay(code) => write!(formatter, "result action replay rejected ({code})"),
            Self::Capacity(code) => write!(formatter, "result capacity rejected ({code})"),
            Self::Unavailable(code) => write!(formatter, "result owner unavailable ({code})"),
        }
    }
}

impl std::error::Error for ResultStoreError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultCleanupMetrics {
    pub removed: usize,
    pub active: usize,
    pub payload_bytes: usize,
}
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    MAX_OWNER_ACTION_BYTES, ResultBinding, ResultContinuationRequest, ResultContinuationSnapshot,
    ResultOrigin, ResultPresentationKind, ResultViewKind,
};
