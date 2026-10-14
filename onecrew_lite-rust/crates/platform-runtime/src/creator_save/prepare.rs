use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use onecrew_creator_runtime::{
    AccountRefId, CanonicalCreatorSaveInput, CreatorCandidateOrigin, CreatorSaveProvenance,
    DiscoveryId,
};
use uuid::Uuid;

use super::{
    CreatorSaveError, CreatorSavePreparationContext, PrepareCreatorSaveInput, PreparedSaveAction,
    projection, reads,
};
use crate::{
    CallDisposition, CreatorSaveBatchOperation, CreatorSaveBatchOutcome, CreatorSaveBatchReason,
    CreatorSaveBatchRowStatus, CreatorSaveCallResult, CreatorSaveConnection,
    CreatorSaveProvenanceInput, CreatorSaveResultSummary, CreatorSaveReviewFailureSummary,
    CreatorSaveReviewRowSummary, CreatorSaveReviewSummary, CreatorSaveSearchOrigin,
    CreatorSaveTarget, ExecutedBatch, ExecutedCallOutcome, ExecutedResultInput, InitialResultState,
    LocalPlatform, OwnerReviewContext, PreparedOwnerNoop, PreparedOwnerReview,
    PresentationProjection, PresentationSection, ResultFailure, ResultRef, ResultResolution,
    ResultResourceRef, ResultUnknown, ResultViewKind, TerminalResult,
};

// A reviewed profile has short-lived confirmation authority.
pub(super) const PREPARED_TTL: Duration = Duration::from_secs(15 * 60);

const READ_SKIPPED: &str = "These selections are not in this review because their profile could not be read again just now.";
const PREPARE_SKIPPED: &str = "These selections are not in this review because their details could not be prepared for saving.";
const ROUTES_TRIMMED: &str = "Some optional contact routes are not shown because these Creators offered more than one review can carry.";
// The card already lists every prepared row. Naming every affected Creator in a
// warning as well would reintroduce, as a byte and section overrun, the same
// whole-card loss this change exists to remove.
const MAX_NAMED_WARNING_SUBJECTS: usize = 10;

/// Warnings grouped by message and named by Creator. Deduplicating on the whole
/// message collapsed forty-eight identical warnings into one line that named
/// nobody, so a reader could not tell which selections it was about.
#[derive(Default)]
pub(super) struct PrepareWarnings {
    entries: Vec<(String, Vec<String>)>,
}

impl PrepareWarnings {
    pub(super) fn add(&mut self, message: &str, subject: Option<&str>) {
        let index = match self
            .entries
            .iter()
            .position(|(existing, _)| existing == message)
        {
            Some(index) => index,
            None => {
                self.entries.push((message.to_owned(), Vec::new()));
                self.entries.len() - 1
            }
        };
        let subjects = &mut self.entries[index].1;
        if let Some(subject) = subject
            && !subjects.iter().any(|existing| existing == subject)
        {
            subjects.push(subject.to_owned());
        }
    }

    pub(super) fn render(self) -> Vec<String> {
        self.entries
            .into_iter()
            .map(|(message, subjects)| {
                if subjects.is_empty() {
                    message
                } else {
                    format!("{message} {}", named_subjects(&subjects))
                }
            })
            .collect()
    }
}

fn named_subjects(subjects: &[String]) -> String {
    let shown = subjects.len().min(MAX_NAMED_WARNING_SUBJECTS);
    let listed = subjects[..shown].join(", ");
    if subjects.len() > shown {
        format!("Affected: {listed} and {} more.", subjects.len() - shown)
    } else {
        format!("Affected: {listed}.")
    }
}

/// Whether a failure belongs to one candidate's own provider facts. Request
/// shape and infrastructure stay fatal for the batch: they are not this
/// Creator's problem, and reporting them per row would claim fifty unusable
/// Creators where one shared precondition did not hold.
pub(super) fn is_row_failure(error: &CreatorSaveError) -> bool {
    matches!(
        error,
        CreatorSaveError::ProviderRead { .. }
            | CreatorSaveError::IdentityMismatch
            | CreatorSaveError::SourceMismatch
    )
}

/// Name a candidate the way its own selection named it. "A Creator" is
/// indistinguishable from the other forty-nine in the batch.
fn candidate_label(candidate: &crate::CreatorSaveCandidateInput) -> String {
    match &candidate.target {
        CreatorSaveTarget::X { handle, .. } => format!("@{handle} on X"),
        CreatorSaveTarget::Instagram { handle, .. } => format!("@{handle} on Instagram"),
        CreatorSaveTarget::Youtube { channel_id } => format!("YouTube channel {channel_id}"),
        CreatorSaveTarget::TelegramAdministrator { user_id, .. } => {
            format!("Telegram user {user_id}")
        }
    }
}

impl LocalPlatform {
    /// Freshly read explicit candidates, prepare canonical rows without Creator
    /// writes, then mint exactly one result holding that prepared authority.
    pub async fn prepare_creator_save(
        &self,
        input: PrepareCreatorSaveInput,
        context: CreatorSavePreparationContext,
    ) -> Result<ResultRef, CreatorSaveError> {
        let context =
            OwnerReviewContext::new(context.origin, context.call_id, context.operation_id, None)?;
        let review = self.prepare_creator_save_review(input, context).await?;
        if review.is_noop() {
            let origin = review.context().origin().clone();
            let noop = review.into_noop()?;
            let initial_state = match noop.terminal() {
                TerminalResult::Resolved(value) => InitialResultState::Resolved(value.clone()),
                TerminalResult::Failed(value) => InitialResultState::Failed(value.clone()),
                TerminalResult::Unknown(value) => InitialResultState::Unknown(value.clone()),
            };
            return self
                .mint_execution_result(ExecutedResultInput {
                    origin,
                    view_kind: ResultViewKind::ReadOnly,
                    batch: ExecutedBatch {
                        outcomes: vec![noop.outcome().clone()],
                    },
                    presentation: noop.presentation().clone(),
                    pending_action: None,
                    initial_state,
                })
                .map_err(CreatorSaveError::from);
        }
        let frame = singleton_frame(review.context());
        Ok(self.mint_owner_review(review, frame, None, Vec::new())?)
    }

    /// Prepare the same canonical review for an adapter-owned batch. Only the
    /// result owner attaches its frame and continuation, without an extra row.
    pub async fn prepare_creator_save_review(
        &self,
        input: PrepareCreatorSaveInput,
        context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, CreatorSaveError> {
        if !super::valid_save_batch_size(input.candidates.len()) {
            return Err(CreatorSaveError::InvalidBatchSize);
        }
        for (candidate_index, candidate) in input.candidates.iter().enumerate() {
            candidate
                .validate()
                .map_err(|error| CreatorSaveError::Input {
                    candidate_index,
                    error,
                })?;
        }
        let intent = input
            .candidates
            .first()
            .map(|candidate| candidate.intent)
            .ok_or(CreatorSaveError::InvalidBatchSize)?;
        if input
            .candidates
            .iter()
            .any(|candidate| candidate.intent != intent)
        {
            return Err(CreatorSaveError::ConflictingIntents);
        }
        let preparation_id = Uuid::new_v4();
        let preparation_started = Instant::now();
        let candidate_count = input.candidates.len();
        let mut warnings = PrepareWarnings::default();
        let resolved = reads::resolve_many(self, input.candidates).await;
        log::debug!(
            "creator preparation id={} phase=fresh_reads candidates={} failed={} elapsed_ms={}",
            preparation_id,
            candidate_count,
            resolved
                .iter()
                .filter(|(_, result)| result.is_err())
                .count(),
            preparation_started.elapsed().as_millis(),
        );
        let (canonical, labels, failures) = canonicalize(resolved, &mut warnings)?;
        if canonical.is_empty() {
            if failures.is_empty() {
                return Err(CreatorSaveError::InvalidBatchSize);
            }
            let failed_count = failures.len();
            let review = crate::CreatorSaveBatchReview {
                rows: Vec::new(),
                failures,
                intent,
                summary: onecrew_creator_runtime::CreatorSaveBatchSummary {
                    total: failed_count,
                    failed: failed_count,
                    ..Default::default()
                },
            };
            let result = no_op_review(context, review, true);
            log_preparation_completion(preparation_id, preparation_started, &result);
            return result;
        }
        let canonical_started = Instant::now();
        let prepared = self
            .creators
            .prepare_creator_save_batch_async(canonical, intent)
            .await;
        log::debug!(
            "creator preparation id={} phase=canonical_prepare outcome={} elapsed_ms={}",
            preparation_id,
            if prepared.is_ok() {
                "completed"
            } else {
                "failed"
            },
            canonical_started.elapsed().as_millis(),
        );
        let prepared = prepared.map_err(CreatorSaveError::from)?;
        // Canonical preparation now reports its own per-row rejections, exactly
        // as execution already reports per-row save failures.
        for failure in prepared.failures() {
            warnings.add(
                PREPARE_SKIPPED,
                labels.get(&failure.row_id).map(String::as_str),
            );
        }
        let result = seal_prepared_review(
            prepared,
            context,
            &warnings.render(),
            &failures,
            true,
            PREPARED_TTL,
        );
        log_preparation_completion(preparation_id, preparation_started, &result);
        result
    }
}

fn log_preparation_completion(
    preparation_id: Uuid,
    started: Instant,
    result: &Result<PreparedOwnerReview, CreatorSaveError>,
) {
    let outcome = match result {
        Ok(review) if review.is_noop() => "terminal",
        Ok(_) => "review_required",
        Err(_) => "failed",
    };
    log::debug!(
        "creator preparation id={} phase=total outcome={} elapsed_ms={}",
        preparation_id,
        outcome,
        started.elapsed().as_millis(),
    );
}

type CanonicalizedCreatorSaveBatch = (
    Vec<CanonicalCreatorSaveInput>,
    BTreeMap<String, String>,
    Vec<crate::CreatorSavePreparedFailure>,
);

/// Turn fresh reads into canonical rows, keeping every selection whose own read
/// succeeded. One unreadable profile used to discard the whole batch, including
/// the forty-nine provider reads that had already succeeded.
pub(super) fn canonicalize(
    resolved: Vec<(
        crate::CreatorSaveCandidateInput,
        Result<reads::ReadCreatorProfile, CreatorSaveError>,
    )>,
    warnings: &mut PrepareWarnings,
) -> Result<CanonicalizedCreatorSaveBatch, CreatorSaveError> {
    let mut canonical = Vec::with_capacity(resolved.len());
    let mut labels = BTreeMap::new();
    let mut failures = Vec::new();
    for (candidate, outcome) in resolved {
        let row_id = format!("row-{}", Uuid::new_v4().simple());
        let (platform, handle, profile_url) = candidate_identity(&candidate);
        let mut read = match outcome {
            Ok(read) => read,
            Err(error) if is_row_failure(&error) => {
                warnings.add(READ_SKIPPED, Some(&candidate_label(&candidate)));
                failures.push(crate::CreatorSavePreparedFailure {
                    row_id,
                    display_name: Some(candidate_label(&candidate)),
                    platform,
                    handle,
                    profile_url,
                    reason: onecrew_creator_runtime::CreatorSavePreparedReason::PreparationFailed,
                });
                continue;
            }
            Err(error) => return Err(error),
        };
        let name = projection::bounded(&read.account.display_name)
            .unwrap_or_else(|| candidate_label(&candidate));
        for message in std::mem::take(&mut read.warnings) {
            warnings.add(&message, Some(&name));
        }
        let provenance = match provenance(&candidate, &mut read) {
            Ok(provenance) => provenance,
            Err(error) if is_row_failure(&error) => {
                warnings.add(READ_SKIPPED, Some(&name));
                failures.push(crate::CreatorSavePreparedFailure {
                    row_id,
                    display_name: Some(name),
                    platform,
                    handle,
                    profile_url,
                    reason: onecrew_creator_runtime::CreatorSavePreparedReason::PreparationFailed,
                });
                continue;
            }
            Err(error) => return Err(error),
        };
        let mut quality = candidate.quality;
        // Counts are observed facts, never a client-restated profile field.
        quality.follower_counts.clear();
        if let Some(count) = read.profile.follower_count {
            quality.follower_counts.insert(read.account.platform, count);
        }
        labels.insert(row_id.clone(), name);
        canonical.push(CanonicalCreatorSaveInput {
            profile_observed_at: read
                .observed_at
                .duration_since(UNIX_EPOCH)
                .map_err(|_| CreatorSaveError::WorkerUnavailable)?
                .as_secs(),
            row_id,
            account: read.account,
            profile: read.profile,
            provenance,
            contact_evidence: read.evidence,
            notes: candidate.notes,
            tags: candidate.tags,
            quality,
        });
    }
    Ok((canonical, labels, failures))
}

fn candidate_identity(
    candidate: &crate::CreatorSaveCandidateInput,
) -> (
    onecrew_creator_runtime::Platform,
    Option<String>,
    Option<String>,
) {
    match &candidate.target {
        crate::CreatorSaveTarget::X { handle, .. }
        | crate::CreatorSaveTarget::Instagram { handle, .. } => (
            match candidate.target {
                crate::CreatorSaveTarget::X { .. } => onecrew_creator_runtime::Platform::X,
                _ => onecrew_creator_runtime::Platform::Instagram,
            },
            Some(handle.clone()),
            None,
        ),
        crate::CreatorSaveTarget::Youtube { .. } => {
            (onecrew_creator_runtime::Platform::YouTube, None, None)
        }
        crate::CreatorSaveTarget::TelegramAdministrator { .. } => {
            (onecrew_creator_runtime::Platform::Telegram, None, None)
        }
    }
}

/// One owner-held result constructor shared by production and offline test fixtures.
/// This remains inside creator_save; protocol adapters cannot call it.
#[cfg(feature = "test-support")]
pub(super) fn mint_prepared_review(
    platform: &LocalPlatform,
    prepared: onecrew_creator_runtime::PreparedCreatorSaveBatch,
    context: CreatorSavePreparationContext,
    warnings: &[String],
    lifetime: Duration,
) -> Result<ResultRef, CreatorSaveError> {
    let context =
        OwnerReviewContext::new(context.origin, context.call_id, context.operation_id, None)?;
    let frame = singleton_frame(&context);
    let review = seal_prepared_review(prepared, context, warnings, &[], false, lifetime)?;
    Ok(platform.mint_owner_review(review, frame, None, Vec::new())?)
}

pub(super) fn seal_prepared_review(
    prepared: onecrew_creator_runtime::PreparedCreatorSaveBatch,
    context: OwnerReviewContext,
    warnings: &[String],
    failures: &[crate::CreatorSavePreparedFailure],
    uncertain_effect: bool,
    lifetime: Duration,
) -> Result<PreparedOwnerReview, CreatorSaveError> {
    let rows = prepared.rows();
    let mut all_failures = failures.to_vec();
    all_failures.extend(prepared.failures().iter().map(|failure| {
        crate::CreatorSavePreparedFailure {
            row_id: failure.row_id.clone(),
            display_name: projection::bounded(&failure.display_name),
            platform: failure.platform,
            handle: failure.handle.clone(),
            profile_url: failure.profile_url.clone(),
            reason: onecrew_creator_runtime::CreatorSavePreparedReason::PreparationFailed,
        }
    }));
    let deadline = Instant::now() + lifetime;
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CreatorSaveError::WorkerUnavailable)?
        .as_secs()
        + lifetime.as_secs();
    let mut summary = prepared.summary();
    let read_failure_count = failures.len();
    summary.total += read_failure_count;
    summary.failed += read_failure_count;
    let (review, trimmed) =
        projection::review(&rows, prepared.intent(), summary, &all_failures, expires_at);
    let mut warnings = warnings.to_vec();
    if !trimmed.is_empty() {
        warnings.push(format!("{ROUTES_TRIMMED} {}", named_subjects(&trimmed)));
    }
    let mut presentation = projection::prepared(&review, &warnings)?;
    presentation.sections.extend(projection::details(&rows));
    if review.summary.eligible_new == 0 && review.summary.eligible_update == 0 {
        return no_op_review(context, review, uncertain_effect);
    }
    let save_context = CreatorSavePreparationContext {
        origin: context.origin().clone(),
        call_id: context.call_id().to_owned(),
        operation_id: context.operation_id().to_owned(),
    };
    Ok(PreparedOwnerReview::creator_save(
        PreparedSaveAction {
            prepared,
            review,
            deadline,
            context: save_context,
            failures: all_failures,
        },
        presentation,
        context,
    )?)
}

fn no_op_review(
    context: OwnerReviewContext,
    review: crate::CreatorSaveBatchReview,
    uncertain_effect: bool,
) -> Result<PreparedOwnerReview, CreatorSaveError> {
    let review_summary = CreatorSaveReviewSummary {
        summary: review.summary,
        rows: review
            .rows
            .iter()
            .map(|row| CreatorSaveReviewRowSummary {
                row_id: row.row_id.clone(),
                status: row.preview.status,
                reason: row.preview.reason,
            })
            .collect(),
        failures: review
            .failures
            .iter()
            .map(|failure| CreatorSaveReviewFailureSummary {
                row_id: failure.row_id.clone(),
                reason: failure.reason,
            })
            .collect(),
    };
    let mut rows = review
        .rows
        .iter()
        .map(|row| {
            let (operation, reason) = match row.preview.status {
                onecrew_creator_runtime::CreatorSavePreparedStatus::ExistingSkipped => (
                    CreatorSaveBatchOperation::ExistingSkipped,
                    Some(CreatorSaveBatchReason::ExistingCreator),
                ),
                onecrew_creator_runtime::CreatorSavePreparedStatus::DuplicateSkipped => (
                    CreatorSaveBatchOperation::DuplicateSkipped,
                    Some(CreatorSaveBatchReason::DuplicateSelection),
                ),
                onecrew_creator_runtime::CreatorSavePreparedStatus::IdentityPending => (
                    CreatorSaveBatchOperation::Failed,
                    Some(match row.preview.reason {
                        Some(
                            onecrew_creator_runtime::CreatorSavePreparedReason::ConflictingUpdate,
                        ) => CreatorSaveBatchReason::ConflictingUpdate,
                        _ => CreatorSaveBatchReason::IdentityAmbiguous,
                    }),
                ),
                onecrew_creator_runtime::CreatorSavePreparedStatus::ContactConflict => (
                    CreatorSaveBatchOperation::Failed,
                    Some(CreatorSaveBatchReason::RequiredContactRouteConflict),
                ),
                onecrew_creator_runtime::CreatorSavePreparedStatus::Failed => (
                    CreatorSaveBatchOperation::Failed,
                    Some(CreatorSaveBatchReason::PreparationFailed),
                ),
                onecrew_creator_runtime::CreatorSavePreparedStatus::EligibleNew
                | onecrew_creator_runtime::CreatorSavePreparedStatus::EligibleUpdate => {
                    return Err(CreatorSaveError::CreatorUnavailable);
                }
            };
            Ok(CreatorSaveBatchOutcome {
                row_id: row.row_id.clone(),
                display_name: row.preview.display_name.clone(),
                platform: row.preview.platform,
                handle: row.preview.handle.clone(),
                profile_url: row.preview.profile_url.clone(),
                avatar_url: row.preview.avatar_url.clone(),
                status: CreatorSaveBatchRowStatus::NotRun,
                operation,
                reason,
                creator_id: None,
                failure_code: None,
                confirmed_route_ids: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, CreatorSaveError>>()?;
    rows.extend(
        review
            .failures
            .iter()
            .map(|failure| CreatorSaveBatchOutcome {
                row_id: failure.row_id.clone(),
                display_name: failure.display_name.clone(),
                platform: failure.platform,
                handle: failure.handle.clone(),
                profile_url: failure.profile_url.clone(),
                avatar_url: None,
                status: CreatorSaveBatchRowStatus::NotRun,
                operation: CreatorSaveBatchOperation::Failed,
                reason: Some(CreatorSaveBatchReason::PreparationFailed),
                creator_id: None,
                failure_code: Some("creator_save_preparation_failed".into()),
                confirmed_route_ids: Vec::new(),
            }),
    );
    let mut summary = CreatorSaveResultSummary {
        total: rows.len(),
        ..CreatorSaveResultSummary::default()
    };
    for row in &rows {
        count_result_operation(&mut summary, row.operation);
    }
    let (title, message) =
        super::projection::outcome_text(crate::PresentationLocale::English, &rows);
    let presentation = PresentationProjection {
        kind: crate::ResultPresentationKind::new("creator_save_batch_outcome")?,
        title,
        summary: Some(message.clone()),
        sections: vec![PresentationSection::CreatorSaveRows { rows: rows.clone() }],
    };
    let terminal = if summary.failed > 0 && (uncertain_effect || !review.failures.is_empty()) {
        TerminalResult::Unknown(ResultUnknown {
            code: "creator_save_noop_unknown".into(),
            summary: message.clone(),
        })
    } else if summary.failed > 0 {
        TerminalResult::Failed(ResultFailure {
            code: "creator_save_noop_blocked".into(),
            summary: message.clone(),
            retryable: false,
        })
    } else {
        TerminalResult::Resolved(ResultResolution {
            code: "creator_save_noop".into(),
            summary: message.clone(),
        })
    };
    let disposition = if summary.failed > 0 && (uncertain_effect || !review.failures.is_empty()) {
        CallDisposition::Unknown {
            code: "creator_save_noop_unknown".into(),
        }
    } else if summary.failed > 0 {
        CallDisposition::Failed {
            code: "creator_save_noop_blocked".into(),
        }
    } else {
        CallDisposition::Succeeded
    };
    let outcome = ExecutedCallOutcome {
        call_id: context.call_id().to_owned(),
        operation_id: context.operation_id().to_owned(),
        disposition,
        summary: message.clone(),
        resource_refs: Vec::<ResultResourceRef>::new(),
    };
    let output = serde_json::to_value(CreatorSaveCallResult { rows, summary })
        .map_err(|_| CreatorSaveError::WorkerUnavailable)?;
    Ok(PreparedOwnerReview::noop(
        context,
        PreparedOwnerNoop::new(
            output,
            presentation,
            terminal,
            outcome,
            None,
            Some(review_summary),
        ),
    )?)
}

fn count_result_operation(
    summary: &mut CreatorSaveResultSummary,
    operation: CreatorSaveBatchOperation,
) {
    match operation {
        CreatorSaveBatchOperation::Added => summary.added += 1,
        CreatorSaveBatchOperation::Updated => summary.updated += 1,
        CreatorSaveBatchOperation::ExistingSkipped => summary.existing_skipped += 1,
        CreatorSaveBatchOperation::DuplicateSkipped => summary.duplicate_skipped += 1,
        CreatorSaveBatchOperation::NotSelected => summary.not_selected += 1,
        CreatorSaveBatchOperation::Failed => summary.failed += 1,
        CreatorSaveBatchOperation::Unknown => summary.unknown += 1,
    }
}

fn singleton_frame(context: &OwnerReviewContext) -> ExecutedBatch {
    ExecutedBatch {
        outcomes: vec![ExecutedCallOutcome {
            call_id: context.call_id().to_owned(),
            operation_id: context.operation_id().to_owned(),
            disposition: CallDisposition::NotRun {
                reason: "confirmation_required".into(),
            },
            summary: "Prepared fresh Creator details for review; confirmation is required to save."
                .into(),
            resource_refs: Vec::new(),
        }],
    }
}

fn provenance(
    candidate: &crate::CreatorSaveCandidateInput,
    resolved: &mut reads::ReadCreatorProfile,
) -> Result<CreatorSaveProvenance, CreatorSaveError> {
    let connection = match candidate.connection {
        CreatorSaveConnection::X { account_ref_id }
        | CreatorSaveConnection::Instagram { account_ref_id }
        | CreatorSaveConnection::Telegram { account_ref_id } => {
            AccountRefId::new(account_ref_id.to_string())
        }
        CreatorSaveConnection::Youtube {
            service_credential_ref_id,
        } => AccountRefId::new(service_credential_ref_id.to_string()),
    };
    Ok(match &candidate.provenance {
        CreatorSaveProvenanceInput::SearchSelection {
            origin,
            source_url,
            description,
        } => match origin {
            CreatorSaveSearchOrigin::Discovery {
                discovery_id,
                candidate_id,
                query,
                search_kind,
                operation_id,
                source_ref,
            } => CreatorSaveProvenance::Discovery {
                candidate_id: candidate_id.as_str().to_owned(),
                selection_context: Some(
                    onecrew_creator_runtime::CreatorDiscoverySelectionContext {
                        operation_id: operation_id.clone(),
                        source_ref: source_ref.clone(),
                        source_url: source_url.clone(),
                        description: description.clone(),
                    },
                ),
                origin: CreatorCandidateOrigin::PlatformSearch {
                    discovery_id: DiscoveryId::new(discovery_id.as_str()),
                    account_ref_id: connection,
                    query: query.as_str().to_owned(),
                    search_kind: search_kind.as_str().to_owned(),
                },
            },
            CreatorSaveSearchOrigin::ProviderRead {
                operation_id,
                source_ref,
            } => CreatorSaveProvenance::ProviderRead {
                source_ref: source_ref.as_str().to_owned(),
                operation_id: operation_id.as_str().to_owned(),
                source_url: source_url.as_ref().map(|value| value.as_str().to_owned()),
                description: description.clone(),
                operator_account_ref_id: Some(connection),
            },
        },
        CreatorSaveProvenanceInput::DirectSelection {
            source_url,
            description,
        } => CreatorSaveProvenance::DirectSelection {
            source_ref: source_url.as_str().to_owned(),
            source_url: Some(source_url.as_str().to_owned()),
            description: description.clone(),
            operator_account_ref_id: Some(connection),
        },
        CreatorSaveProvenanceInput::AdministratorSelection {
            community_id,
            community_kind,
            ..
        } => {
            let (role, community, source) = resolved
                .administrator
                .take()
                .ok_or(CreatorSaveError::SourceMismatch)?;
            CreatorSaveProvenance::AdministratorSelection {
                source_ref: format!(
                    "telegram:{}:{}",
                    community_kind.as_str(),
                    community_id.as_str()
                ),
                role,
                community,
                source,
            }
        }
    })
}

#[cfg(test)]
mod tests;
