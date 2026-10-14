use std::time::{Duration, Instant};

use onecrew_creator_runtime::{
    ManualCreatorBatchCreateInput, ManualCreatorBatchValidation, ManualCreatorCreateInput,
};

use super::models::PreparedCreatorBatchAction;
use crate::{
    LocalPlatform, OwnerReviewContext, PreparedOwnerReview, PresentationField, PresentationItem,
    PresentationProjection, PresentationSection, ResultPresentationKind, ResultStoreError,
};

/// A review is a decision the user is making now. It expires on the same budget
/// as email and outreach, so a stale card can never write an hour-old batch.
const PREPARE_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// A row that exists to name a Creator. With no name there is nothing to show,
/// and the card must still list the row: confirming a batch means seeing how
/// many records it creates, including one that arrived unnamed.
const UNNAMED_CREATOR: &str = "(creator has no name)";

/// A Creator with no initial contact route is a real thing to confirm, so the
/// row says so rather than rendering an empty description the store rejects.
const NO_CONTACT_ROUTES: &str = "no initial contact routes";

/// Per-row text bounds for the review card.
///
/// These follow the *shared projection* budget, not the store's per-string cap.
/// A confirmation projection is validated whole against `MAX_PROJECTION_BYTES`
/// (64 KiB), and every one of the batch's up-to-50 rows spends from that single
/// budget. A row that only respected the 8 KiB per-string cap
/// (`MAX_PRESENTATION_TEXT_BYTES`) would still lose the entire card once fifty
/// rows shared one projection — the whole batch would become unconfirmable
/// because one Creator had a long name. Changing either bound changes how many
/// rows fit: keep `MAX_MANUAL_CREATOR_BATCH_ITEMS * (name + detail)` comfortably
/// under the projection cap.
const MAX_ROW_NAME_BYTES: usize = 256;
const MAX_ROW_DETAIL_BYTES: usize = 512;

impl LocalPlatform {
    /// Freeze one manual Creator batch for owner review.
    ///
    /// Nothing is claimed and nothing is written here. The Creators are created
    /// only when the grant this mints is consumed, and only from the exact items
    /// sealed now.
    ///
    /// Preparation deliberately re-checks nothing the Creator owner already
    /// owns: the batch size, per-Creator route count, and item shape are the
    /// input schema's and the runtime's rules, and restating them here would
    /// create a second copy of the same business limits. Only the idempotency
    /// key is checked, because it is the one field this review seals whose
    /// rejection must precede minting a card.
    ///
    /// Preparation performs no I/O: it validates the sealed key and builds the
    /// card from the request it was handed. It is `async` because it is an owner
    /// call on a handler's async path, which the blocking-dispatch contract
    /// requires be awaited rather than run inline on a runtime worker.
    ///
    /// # Errors
    /// Rejects an invalid idempotency key and review copy the result store
    /// refuses. Both precede the claim, the fingerprint, and every per-item
    /// write, so no Creator can have been created.
    pub async fn prepare_manual_creator_batch(
        &self,
        input: ManualCreatorBatchCreateInput,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, String> {
        if input.idempotency_key.trim().is_empty() || input.idempotency_key.len() > 128 {
            return Err("idempotency_key must contain 1-128 characters".into());
        }
        let presentation = PresentationProjection {
            kind: ResultPresentationKind::new("creator_batch_confirmation")
                .map_err(store_rejection)?,
            title: "Review Creator batch".to_owned(),
            summary: Some(
                "Confirm the exact Creator records listed below before they are created."
                    .to_owned(),
            ),
            sections: review_sections(&input),
        };
        PreparedOwnerReview::creator_batch(
            PreparedCreatorBatchAction {
                intent: input,
                expires: Instant::now() + PREPARE_LIFETIME,
            },
            review_context,
            presentation,
            "Create these Creator records from the submitted batch?".to_owned(),
            "Create Creators".to_owned(),
        )
        .map_err(store_rejection)
    }
}

/// Every rejection here happens before the review is even minted, so no claim
/// exists and no Creator was written.
fn store_rejection(error: ResultStoreError) -> String {
    error.to_string()
}

/// The card's sections: what the batch does as a whole, and one row naming each
/// Creator it will create. The whole batch is listed — the Creator owner already
/// bounds it at `MAX_MANUAL_CREATOR_BATCH_ITEMS`, which is well inside one
/// presentation section, so a reader confirming a batch sees all of it rather
/// than a truncated prefix.
fn review_sections(input: &ManualCreatorBatchCreateInput) -> Vec<PresentationSection> {
    let fields = vec![
        field("Validation", validation_label(input.validation)),
        field("Creators", input.items.len().to_string()),
    ];
    let mut sections = vec![PresentationSection::Fields {
        heading: None,
        fields,
    }];
    let rows = input
        .items
        .iter()
        .map(|item| PresentationItem {
            title: creator_label(item),
            description: Some(contact_route_label(item)),
        })
        .collect::<Vec<_>>();
    if !rows.is_empty() {
        sections.push(PresentationSection::Items {
            heading: Some("Creators to create".to_owned()),
            items: rows,
        });
    }
    sections
}

/// One reviewed fact. Both values this card builds are non-empty by
/// construction; the guard keeps a blank value from costing the whole card if
/// that ever stops being true.
fn field(label: &str, value: impl Into<String>) -> PresentationField {
    let value = value.into();
    PresentationField {
        label: label.to_owned(),
        value: if value.trim().is_empty() {
            "(empty)".to_owned()
        } else {
            value
        },
    }
}

fn validation_label(validation: ManualCreatorBatchValidation) -> &'static str {
    match validation {
        ManualCreatorBatchValidation::Strict => "strict",
        ManualCreatorBatchValidation::Permissive => "permissive",
    }
}

/// One Creator as the reader will recognize them.
fn creator_label(item: &ManualCreatorCreateInput) -> String {
    bounded(item.display_name.trim(), MAX_ROW_NAME_BYTES)
        .unwrap_or_else(|| UNNAMED_CREATOR.to_owned())
}

/// The routes a new Creator arrives with, as `channel: value`. These are what a
/// reader is really approving: a contact route is how this Creator will later be
/// reached, so the card names each one rather than counting them.
fn contact_route_label(item: &ManualCreatorCreateInput) -> String {
    let routes = item
        .contact_routes
        .iter()
        .map(|route| format!("{}: {}", route.channel.as_str(), route.value.trim()))
        .collect::<Vec<_>>()
        .join(", ");
    bounded(routes.trim(), MAX_ROW_DETAIL_BYTES).unwrap_or_else(|| NO_CONTACT_ROUTES.to_owned())
}

/// Bound one rendered value on a UTF-8 boundary. `None` for a value with
/// nothing to show, so the caller names the row rather than emitting a blank
/// string the store rejects.
fn bounded(value: &str, max_bytes: usize) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    if value.len() <= max_bytes {
        return Some(value.to_owned());
    }
    let mut split = max_bytes;
    while !value.is_char_boundary(split) {
        split -= 1;
    }
    Some(value[..split].to_owned())
}

#[cfg(test)]
mod tests {
    use onecrew_creator_runtime::{ContactChannel, ManualCreatorInitialContactRouteInput};

    use super::*;

    fn item(display_name: &str, routes: Vec<(ContactChannel, &str)>) -> ManualCreatorCreateInput {
        ManualCreatorCreateInput {
            tags: None,
            display_name: display_name.to_owned(),
            notes: None,
            contact_routes: routes
                .into_iter()
                .map(|(channel, value)| ManualCreatorInitialContactRouteInput {
                    channel,
                    value: value.to_owned(),
                    source_url: None,
                })
                .collect(),
        }
    }

    fn batch(items: Vec<ManualCreatorCreateInput>) -> ManualCreatorBatchCreateInput {
        ManualCreatorBatchCreateInput {
            idempotency_key: "batch-1".to_owned(),
            validation: ManualCreatorBatchValidation::Strict,
            items,
        }
    }

    fn listed(sections: &[PresentationSection]) -> Vec<PresentationItem> {
        sections
            .iter()
            .find_map(|section| match section {
                PresentationSection::Items { items, .. } => Some(items.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn fields(sections: &[PresentationSection]) -> Vec<PresentationField> {
        sections
            .iter()
            .find_map(|section| match section {
                PresentationSection::Fields { fields, .. } => Some(fields.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn every_creator_in_the_batch_is_named_with_the_routes_it_arrives_with() {
        let sections = review_sections(&batch(vec![
            item(
                "Nova Lin",
                vec![(ContactChannel::Email, "nova@example.com")],
            ),
            item("Rin Ito", vec![]),
        ]));

        let rows = listed(&sections);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "Nova Lin");
        assert_eq!(
            rows[0].description.as_deref(),
            Some("email: nova@example.com")
        );
        assert_eq!(rows[1].title, "Rin Ito");
        assert_eq!(rows[1].description.as_deref(), Some(NO_CONTACT_ROUTES));
        assert_eq!(
            fields(&sections)
                .iter()
                .find(|field| field.label == "Creators")
                .map(|field| field.value.as_str()),
            Some("2")
        );
    }

    #[test]
    fn an_unnamed_creator_still_occupies_a_row_the_reader_can_count() {
        let sections = review_sections(&batch(vec![item("   ", vec![])]));

        let rows = listed(&sections);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, UNNAMED_CREATOR);
    }

    /// A full batch of long names must still fit one confirmation projection.
    /// Without per-row bounds the card is rejected whole and nothing can be
    /// confirmed, which is the failure these bounds exist to prevent.
    #[test]
    fn a_full_batch_of_oversized_rows_stays_inside_one_confirmation_projection() {
        let items = (0..onecrew_creator_runtime::MAX_MANUAL_CREATOR_BATCH_ITEMS)
            .map(|index| {
                let address = format!("{}{index}@example.com", "x".repeat(4 * 1024));
                item(
                    &"名".repeat(4 * 1024),
                    vec![(ContactChannel::Email, &address)],
                )
            })
            .collect::<Vec<_>>();
        let sections = review_sections(&batch(items));

        let rows = listed(&sections);
        assert_eq!(
            rows.len(),
            onecrew_creator_runtime::MAX_MANUAL_CREATOR_BATCH_ITEMS
        );
        for row in &rows {
            assert!(row.title.len() <= MAX_ROW_NAME_BYTES, "{}", row.title.len());
            assert!(
                row.description
                    .as_ref()
                    .is_some_and(|value| value.len() <= MAX_ROW_DETAIL_BYTES)
            );
        }
        let projection = PresentationProjection {
            kind: ResultPresentationKind::new("creator_batch_confirmation").unwrap(),
            title: "Review Creator batch".to_owned(),
            summary: Some(
                "Confirm the exact Creator records listed below before they are created."
                    .to_owned(),
            ),
            sections,
        };
        let bytes = serde_json::to_vec(&projection).unwrap().len();
        assert!(
            bytes < 64 * 1024,
            "confirmation projection uses {bytes} bytes"
        );
    }
}
