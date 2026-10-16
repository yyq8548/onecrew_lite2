//! Model-only projection. The full owner material and cache identity stay intact.
use super::{DiscoveryReviewMaterial, RUBRIC};

pub(super) fn build(
    objective: &str,
    material: &DiscoveryReviewMaterial,
    mut value: serde_json::Value,
) -> String {
    // Planning/display metadata is not a creator qualification condition.
    if let Some(criteria) = value["criteria"].as_object_mut() {
        criteria.remove("automatic_query_expansion");
        criteria.remove("table_subject");
    }
    let language_start = RUBRIC
        .find("Return language_only_pending=true ONLY")
        .expect("language rubric boundary");
    let instagram_start = RUBRIC
        .find("\nFor Instagram,")
        .expect("Instagram rubric boundary");
    let mut rubric = RUBRIC[..language_start].to_owned();
    rubric.push_str("\nReturn requirement_findings as an array. For EACH frozen criteria.requirements[index], return exactly one finding with criterion={\"requirement\":index}, outcome supported/contradicted/insufficient, rationale and exact saved citations. Supported means the candidate satisfies that complete customer condition; contradicted means positive evidence violates it. Preserve all exclusions. Missing evidence or no mention in a sample cannot establish absence of a forbidden partnership, an all-time guarantee or an unknown private fact: return insufficient. Never invent requirements or weaken the core customer goal. tool, country_code and match_level must be null for these findings. With no requirements return []. These findings supplement the single match_fit finding. Evaluate these additional conditions in requirement_findings; evaluate core topic, followers, language and relevance in match_fit. The owner combines all eligibility gates, so a supported match_fit never overrides an unresolved requirement.\n");
    if material.native_follower_estimate.is_some() {
        rubric.push_str("\nThe native_follower_estimate is a rounded public display, not an exact count. Report its display as approximate (约). Use lower >= required minimum to confirm the follower condition; upper < minimum to confirm failure. If the interval crosses the minimum, the condition is unknown. A null observed_followers does not erase this bound native estimate.\n");
    }
    if material.criteria["language"]
        .as_str()
        .is_some_and(|v| !v.trim().is_empty())
    {
        rubric.push_str(&RUBRIC[language_start..instagram_start]);
    } else {
        rubric.push_str("No content-language condition is approved. Return language_only_pending=false. Do not request language evidence or comments.\n");
        if let Some(samples) = value["language_samples"].as_array_mut() {
            for sample in samples {
                if let Some(object) = sample.as_object_mut() {
                    object.remove("comments");
                }
            }
        }
    }
    if material
        .native_platform
        .as_deref()
        .unwrap_or(&material.hit.platform)
        == "instagram"
    {
        rubric.push_str(&RUBRIC[instagram_start..]);
    }
    if let Some(object) = value.as_object_mut() {
        for key in [
            "approved_scopes",
            "approved_at",
            "campaign_id",
            "criteria_revision",
            "candidate_id",
            "format_version",
        ] {
            object.remove(key);
        }
    }
    format!(
        "{rubric}\nUNTRUSTED_SAVED_DATA:\n{}",
        serde_json::json!({"objective":objective,"material":value})
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LocalPlatform,
        discovery_identity_test_support::{authenticate, saved_pair},
    };

    #[test]
    fn projection_keeps_citations_and_only_applicable_policy() {
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        authenticate(&platform, "operator");
        let pair = saved_pair(&platform);
        let mut material = platform
            .creators()
            .unwrap()
            .prepare_discovery_review_material(&pair.campaign_id, 1, &pair.left_search_hit_id)
            .unwrap();
        let original = serde_json::to_value(&material).unwrap();
        let short = build("Shoe reviewers", &material, original.clone());
        assert!(!short.contains("at least THREE distinct"));
        assert!(!short.contains("For Instagram,"));
        let projected: serde_json::Value =
            serde_json::from_str(short.split("UNTRUSTED_SAVED_DATA:\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            projected["material"]["video_description"],
            original["video_description"]
        );
        assert_eq!(projected["material"]["hit"], original["hit"]);
        assert!(projected["material"].get("approved_scopes").is_none());
        material.criteria["language"] = serde_json::json!("中文");
        let full_language = build(
            "中文鞋评",
            &material,
            serde_json::to_value(&material).unwrap(),
        );
        assert!(full_language.contains("at least THREE distinct"));
        assert!(full_language.contains("first FIVE saved comments"));
        assert!(short.len() < full_language.len());
        material.native_platform = Some("instagram".into());
        let instagram = build(
            "中文美妆",
            &material,
            serde_json::to_value(&material).unwrap(),
        );
        assert!(instagram.contains("For Instagram,"));
        assert!(instagram.contains("at least THREE distinct"));
        assert_eq!(
            original["approved_scopes"],
            serde_json::to_value(&material).unwrap()["approved_scopes"]
        );
    }
}
