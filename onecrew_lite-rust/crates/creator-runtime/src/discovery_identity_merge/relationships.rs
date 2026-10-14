use super::DiscoveryIdentityReviewKind;
use std::collections::BTreeMap;

pub(crate) type IdentityRelationship = (String, String, String);

pub(super) fn components(edges: &[IdentityRelationship]) -> BTreeMap<&str, &str> {
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (a, b, kind) in edges {
        if kind == "same_author" {
            adjacency.entry(a).or_default().push(b);
            adjacency.entry(b).or_default().push(a);
        }
    }
    let mut groups = BTreeMap::new();
    for root in adjacency.keys().copied() {
        let mut pending = vec![root];
        while let Some(current) = pending.pop() {
            if groups.contains_key(current) {
                continue;
            }
            groups.insert(current, root);
            if let Some(neighbors) = adjacency.get(current) {
                pending.extend(neighbors.iter().copied());
            }
        }
    }
    groups
}

fn connected(groups: &BTreeMap<&str, &str>, left: &str, right: &str) -> bool {
    groups.get(left).copied().unwrap_or(left) == groups.get(right).copied().unwrap_or(right)
}

/// Contradictory direct or transitive identity claims require explicit audited
/// revocation first; a new decision must not silently replace an active relation.
pub(crate) fn validate_addition(
    edges: &[IdentityRelationship],
    kind: DiscoveryIdentityReviewKind,
    ids: &[String; 2],
) -> Result<(), String> {
    if !matches!(
        kind,
        DiscoveryIdentityReviewKind::SameAuthor | DiscoveryIdentityReviewKind::DifferentAuthors
    ) {
        return Ok(());
    }
    for (a, b, existing) in edges {
        if (a == &ids[0] && b == &ids[1]) || (a == &ids[1] && b == &ids[0]) {
            return Err(if existing == kind.as_str() {
                "identity_review_relationship_already_reviewed"
            } else {
                "identity_review_conflicting_relationship"
            }
            .into());
        }
    }
    let groups = components(edges);
    let conflicts = match kind {
        DiscoveryIdentityReviewKind::SameAuthor => edges.iter().any(|(a, b, existing)| {
            existing == "different_authors"
                && ((connected(&groups, &ids[0], a) && connected(&groups, &ids[1], b))
                    || (connected(&groups, &ids[0], b) && connected(&groups, &ids[1], a)))
        }),
        DiscoveryIdentityReviewKind::DifferentAuthors => connected(&groups, &ids[0], &ids[1]),
        _ => false,
    };
    if conflicts {
        return Err("identity_review_conflicting_relationship".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_separation_cannot_be_hidden_by_a_transitive_merge() {
        let edges = vec![
            ("a".into(), "b".into(), "different_authors".into()),
            ("a".into(), "c".into(), "same_author".into()),
        ];
        assert!(
            validate_addition(
                &edges,
                DiscoveryIdentityReviewKind::SameAuthor,
                &["c".into(), "b".into()]
            )
            .is_err()
        );
        assert!(
            validate_addition(
                &edges,
                DiscoveryIdentityReviewKind::SameAuthor,
                &["a".into(), "c".into()]
            )
            .is_err()
        );
        assert!(
            validate_addition(
                &edges,
                DiscoveryIdentityReviewKind::SameAuthor,
                &["c".into(), "d".into()]
            )
            .is_ok()
        );
        let grouped = vec![
            ("a".into(), "c".into(), "same_author".into()),
            ("c".into(), "b".into(), "same_author".into()),
        ];
        assert!(
            validate_addition(
                &grouped,
                DiscoveryIdentityReviewKind::DifferentAuthors,
                &["a".into(), "b".into()]
            )
            .is_err()
        );
    }
}
