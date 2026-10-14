//! A confirmed plan admits exactly its approved scopes until it expires or is
//! cancelled. Validation consumes and records nothing, so an approved scope is
//! admitted any number of times.
use super::*;
use crate::{BoundSearchAuthorization, SearchPlanPolicy};
use std::sync::Barrier;

const FIRST: &str = "sha256:first";
const SECOND: &str = "sha256:second";

fn confirmed(scopes: &[&str]) -> (SearchAuthority, String) {
    let authority = SearchAuthority::default();
    let reference = authority
        .confirm(scopes.iter().map(|scope| (*scope).to_owned()).collect())
        .expect("distinct scopes confirm");
    (authority, reference)
}

/// A grant for `token` without the mint-time validation, so a test can hold
/// a grant for a scope the plan does not approve.
fn raw_grant(authority: &SearchAuthority, reference: &str, token: &str) -> ApprovalGrant {
    ApprovalGrant {
        store: Arc::clone(&authority.approvals),
        reference: reference.to_owned(),
        token: token.to_owned(),
    }
}

#[test]
fn a_confirmed_plan_lives_for_the_ttl_from_confirmation() {
    assert_eq!(SEARCH_PLAN_TTL_SECONDS, 3600);
    let ttl = Duration::from_secs(SEARCH_PLAN_TTL_SECONDS);
    let before = Instant::now();
    let (authority, reference) = confirmed(&[FIRST]);
    let after = Instant::now();
    let expires = authority.approvals.lock().unwrap()[&reference].expires;
    assert!(
        expires >= before + ttl && expires <= after + ttl,
        "the expiry is the TTL from confirmation"
    );
}

#[test]
fn every_approved_scope_is_admitted_any_number_of_times() {
    let (authority, reference) = confirmed(&[FIRST, SECOND]);
    for token in [FIRST, SECOND] {
        let grant = raw_grant(&authority, &reference, token);
        for _ in 0..100 {
            grant.validate().expect("validation consumes nothing");
            authority
                .authorize(Some(&reference), token)
                .expect("minting consumes nothing");
        }
    }
}

#[test]
fn validation_checks_run_in_contract_order() {
    // A missing or expired approval beats an unapproved scope.
    let (authority, reference) = confirmed(&[FIRST]);
    assert_eq!(
        raw_grant(&authority, "missing-plan", SECOND).validate(),
        Err(SearchPlanApprovalError::InvalidOrExpired)
    );
    assert_eq!(
        raw_grant(&authority, &reference, SECOND).validate(),
        Err(SearchPlanApprovalError::ScopeMismatch)
    );
    assert_eq!(raw_grant(&authority, &reference, FIRST).validate(), Ok(()));
}

#[test]
fn expiry_or_cancel_ends_the_approval() {
    for cancel in [false, true] {
        let (authority, reference) = confirmed(&[FIRST]);
        let grant = raw_grant(&authority, &reference, FIRST);
        let bound = BoundSearchAuthorization::bind(
            &"request",
            SearchPlanPolicy::RequireConfirmation,
            Some(&reference),
            Some(authority.authorize(Some(&reference), FIRST).unwrap()),
        )
        .unwrap();
        grant
            .validate()
            .expect("the live approval admits its scope");
        if cancel {
            authority.cancel(&reference).unwrap();
        } else {
            authority
                .approvals
                .lock()
                .unwrap()
                .get_mut(&reference)
                .unwrap()
                .expires = Instant::now();
        }
        assert_eq!(
            grant.validate(),
            Err(SearchPlanApprovalError::InvalidOrExpired)
        );
        assert_eq!(
            bound.validate_request(&"request"),
            Err(SearchPlanApprovalError::InvalidOrExpired),
        );
        assert_eq!(
            authority.authorize(Some(&reference), FIRST).unwrap_err(),
            SearchPlanApprovalError::InvalidOrExpired
        );
    }
}

#[test]
fn a_poisoned_store_is_unavailable() {
    let (authority, reference) = confirmed(&[FIRST]);
    let grant = raw_grant(&authority, &reference, FIRST);
    let bound = BoundSearchAuthorization::bind(
        &"request",
        SearchPlanPolicy::RequireConfirmation,
        Some(&reference),
        Some(authority.authorize(Some(&reference), FIRST).unwrap()),
    )
    .unwrap();
    let store = Arc::clone(&authority.approvals);
    let poisoner = std::thread::spawn(move || {
        let _held = store.lock().unwrap();
        panic!("poison the approval store");
    });
    assert!(poisoner.join().is_err());
    assert_eq!(grant.validate(), Err(SearchPlanApprovalError::Unavailable));
    assert_eq!(
        bound.validate_request(&"request"),
        Err(SearchPlanApprovalError::Unavailable)
    );
    assert_eq!(
        authority.authorize(Some(&reference), FIRST).unwrap_err(),
        SearchPlanApprovalError::Unavailable
    );
}

#[test]
fn sixty_four_racing_threads_are_all_admitted() {
    const THREADS: usize = 64;
    const ATTEMPTS: usize = 4;
    let scopes = ["sha256:a", "sha256:b", "sha256:c"];
    let (authority, reference) = confirmed(&scopes);
    let barrier = Arc::new(Barrier::new(THREADS));
    let workers = (0..THREADS)
        .map(|index| {
            let grant = raw_grant(&authority, &reference, scopes[index % scopes.len()]);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                (0..ATTEMPTS).map(|_| grant.validate()).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    let results = workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("a validating thread never panics"))
        .collect::<Vec<_>>();
    assert_eq!(results.len(), THREADS * ATTEMPTS);
    assert!(
        results.iter().all(|result| result.is_ok()),
        "no plan-level count refuses a repeated scope: {results:?}"
    );
}

#[test]
fn minting_through_the_authority_validates_the_reference_and_scope() {
    let (authority, reference) = confirmed(&[FIRST]);
    authority
        .authorize(Some(&reference), FIRST)
        .expect("the approved scope mints");
    assert_eq!(
        authority.authorize(None, FIRST).unwrap_err(),
        SearchPlanApprovalError::ConfirmationRequired
    );
    assert_eq!(
        authority
            .authorize(Some("missing-plan"), FIRST)
            .unwrap_err(),
        SearchPlanApprovalError::InvalidOrExpired
    );
    assert_eq!(
        authority.authorize(Some(&reference), SECOND).unwrap_err(),
        SearchPlanApprovalError::ScopeMismatch
    );
}
