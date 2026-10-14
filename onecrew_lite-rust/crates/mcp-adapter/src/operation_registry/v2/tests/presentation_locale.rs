//! 冻结卡片语言后仅翻译 adapter 固定文案，保留原生证据和未知效果。
use std::sync::Arc;

use onecrew_local_runtime::{
    CallDisposition, PresentationLocale, PresentationSection, ResultState,
};
use serde_json::json;

use super::{
    creator_save_support as support,
    fixtures::{
        TestInput, TestOutput, TestOwnerError, call, metadata, policy, result_owner, revision,
    },
};
use crate::operation_registry::{
    descriptor::{ApprovalClass, EffectClass, SurfaceId},
    execution::{ClassifiedOwnerFailure, PublicFailureCode},
    registration::RegistryBuilder,
    v2::{
        contract::{
            PresentationRequest, ResultPresentInput, ResultPresentStatus, ToolExecuteInput,
            ToolPresentationFailure,
        },
        state::result_present_v2,
    },
};

#[tokio::test]
async fn presentation_locale_keeps_native_evidence_and_localizes_omission_counts() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry_with_large_reads(Arc::clone(&platform), 1);
    let operation = "onecrew.test.native-read";
    let input = ToolExecuteInput {
        calls: (0..8)
            .map(|index| {
                call(
                    &format!("read-{index}"),
                    operation,
                    revision(&fixture.registry, operation),
                    json!({"value":"native"}),
                )
            })
            .collect(),
        presentation: PresentationRequest::Requested,
    };
    let result = fixture
        .registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "zh-read-summary",
            &input,
        )
        .await
        .unwrap();
    let presented = result_present_v2(
        &platform,
        &origin,
        ResultPresentInput {
            result_ref: result.result_ref.unwrap(),
            locale: PresentationLocale::SimplifiedChinese,
        },
    )
    .unwrap();
    assert_eq!(presented.model.status, ResultPresentStatus::Resolved);
    let wire = serde_json::to_string(&presented.snapshot).unwrap();
    assert!(!wire.contains("Showing "), "{wire}");
    assert!(!wire.contains("Batch execution completed."));
    assert!(
        wire.contains("cursor-2"),
        "the original cursor is not translated"
    );
    assert!(
        wire.contains("Creator 0"),
        "the creator name is owner evidence"
    );
    let notices = presented
        .snapshot
        .presentation
        .sections
        .iter()
        .filter_map(|section| match section {
            PresentationSection::Notice { body, .. } => Some(body.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(notices.len(), 8);
    assert!(
        notices
            .iter()
            .all(|notice| notice.contains("50") && notice.contains("展示"))
    );
}

#[tokio::test]
async fn presentation_locale_preserves_unknown_effects_with_a_safe_display_diagnostic() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.long-read",
            "onecrew.test.effect-unknown",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.long-read"),
            policy(ApprovalClass::None, EffectClass::ReadLocal),
            |_, _| async {
                Ok(TestOutput {
                    value: "x".repeat(8193),
                })
            },
            |_| unreachable!(),
        )
        .unwrap();
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.effect-unknown"),
            policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite),
            |_, _| async { Err(TestOwnerError) },
            |_| {
                ClassifiedOwnerFailure::unknown(
                    PublicFailureCode::parse("effect_unknown").unwrap(),
                    None,
                )
            },
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: ["onecrew.test.long-read", "onecrew.test.effect-unknown"]
            .into_iter()
            .enumerate()
            .map(|(index, operation)| {
                call(
                    &format!("call-{index}"),
                    operation,
                    revision(&registry, operation),
                    json!({"value":"fixture"}),
                )
            })
            .collect(),
        presentation: PresentationRequest::Requested,
    };
    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "unknown-display",
            &input,
        )
        .await
        .unwrap();
    assert_eq!(
        result.presentation_failure,
        Some(ToolPresentationFailure::ProjectionUnavailable)
    );
    let presented = result_present_v2(
        &platform,
        &origin,
        ResultPresentInput {
            result_ref: result.result_ref.unwrap(),
            locale: PresentationLocale::SimplifiedChinese,
        },
    )
    .unwrap();
    assert_eq!(presented.model.status, ResultPresentStatus::Unknown);
    let diagnostic = presented
        .model
        .failure
        .expect("unknown effect still exposes the display issue");
    assert_eq!(diagnostic.code, "presentation_unavailable");
    assert!(!diagnostic.retryable);
    assert!(diagnostic.summary.contains("未知") || diagnostic.summary.contains("尚未确认"));
    assert!(!diagnostic.summary.contains("操作已执行完成"));
    assert!(matches!(presented.snapshot.state, ResultState::Unknown(_)));
    assert!(
        matches!(&presented.snapshot.batch.outcomes[1].disposition, CallDisposition::Unknown { code } if code == "effect_unknown")
    );
    assert!(
        !serde_json::to_string(&presented.snapshot)
            .unwrap()
            .contains("The owner finished")
    );
}
