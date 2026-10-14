//! Only owner-observed numeric failures bypass semantic model assessment.
pub(super) fn below_minimum(
    criteria: &serde_json::Value,
    count: Option<u64>,
) -> Option<(u64, u64)> {
    let minimum = criteria["min_subscribers"]
        .as_u64()
        .or_else(|| criteria["min_followers"].as_u64())
        .filter(|n| *n > 0)?;
    count
        .filter(|count| *count < minimum)
        .map(|count| (count, minimum))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_numeric_failure_is_inclusive_and_unknown_is_not_failure() {
        let criteria = serde_json::json!({"min_subscribers":8000});
        assert_eq!(below_minimum(&criteria, Some(7999)), Some((7999, 8000)));
        assert_eq!(below_minimum(&criteria, Some(0)), Some((0, 8000)));
        assert_eq!(below_minimum(&criteria, Some(8000)), None);
        assert_eq!(below_minimum(&criteria, Some(8001)), None);
        assert_eq!(below_minimum(&criteria, None), None);
        assert_eq!(
            below_minimum(&serde_json::json!({"min_subscribers":0}), Some(0)),
            None
        );
        assert_eq!(below_minimum(&serde_json::json!({}), Some(0)), None);
        assert_eq!(
            below_minimum(&serde_json::json!({"min_followers":8000}), Some(592)),
            Some((592, 8000))
        );
    }
}
