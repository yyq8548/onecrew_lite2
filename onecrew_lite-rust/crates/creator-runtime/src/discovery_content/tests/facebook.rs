use crate::{DiscoveryContentClaim, FinishDiscoveryContentInput};
use serde_json::json;

fn input() -> FinishDiscoveryContentInput {
    let source = "https://www.facebook.com/reel/2067046843941874";
    FinishDiscoveryContentInput {
        claim: DiscoveryContentClaim {
            campaign_id: "isolated-facebook".into(),
            expected_revision: 1,
            search_hit_id: "observed".into(),
            platform: "facebook".into(),
            profile_ref: "test".into(),
            author_id: "61590270912364".into(),
            source_url: source.into(),
            call_number: 3,
        },
        native_content: Some(
            json!({"post_id":"2067046843941874","author_id":"61590270912364",
            "source_url":source,"text":"Original caption","text_truncated":false}),
        ),
        stop_code: None,
    }
}

#[test]
fn numeric_facebook_caption_requires_exact_author_post_and_source() {
    assert!(super::super::validate_content_result(&input()).is_ok());
    for key in ["post_id", "author_id", "source_url"] {
        let mut item = input();
        item.native_content.as_mut().unwrap()[key] = json!("other");
        assert!(super::super::validate_content_result(&item).is_err());
    }
    for source in [
        "https://www.facebook.com/profile.php?id=61590270912364",
        "https://www.facebook.com/reel/2067046843941874/",
        "https://www.facebook.com/reel/2067046843941874?comment_id=123",
    ] {
        let mut item = input();
        item.claim.source_url = source.into();
        item.native_content.as_mut().unwrap()["source_url"] = json!(source);
        assert!(super::super::validate_content_result(&item).is_err());
    }
}

#[test]
fn audience_or_private_fields_cannot_be_promoted_to_author_content() {
    for key in ["comments", "commenter_id", "session"] {
        let mut item = input();
        item.native_content.as_mut().unwrap()[key] = json!("unneeded");
        assert!(super::super::validate_content_result(&item).is_err());
    }
    for text in [" ".to_owned(), "bad\0text".to_owned(), "日".repeat(2667)] {
        let mut item = input();
        item.native_content.as_mut().unwrap()["text"] = json!(text);
        assert!(super::super::validate_content_result(&item).is_err());
    }
    let mut item = input();
    item.native_content.as_mut().unwrap()["text"] = json!("日".repeat(2666));
    item.native_content.as_mut().unwrap()["text_truncated"] = json!(true);
    assert!(super::super::validate_content_result(&item).is_ok());
    item.stop_code = Some("opencli_invalid_output".into());
    assert!(super::super::validate_content_result(&item).is_err());
}
