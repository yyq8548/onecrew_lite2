//! Literal bytes captured from the released Platform batch contracts before
//! replacing the duplicate definitions. All addresses and payloads are fake.
use crate::{
    EmailBatchChunkResult, EmailBatchItem, EmailBatchItemOutcome, EmailBatchItemResult,
    EmailBatchRequest, EmailBatchResult, EmailBatchValidation,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn request(validation: EmailBatchValidation) -> EmailBatchRequest {
    EmailBatchRequest {
        idempotency_key: "fixture-intent-世界".into(),
        validation,
        items: serde_json::from_str(include_str!("testdata/batch-items.json")).unwrap(),
    }
}

fn result() -> EmailBatchResult {
    let outcomes = vec![
        EmailBatchItemResult {
            input_index: 2,
            outcome: EmailBatchItemOutcome::Unknown {
                message: "synthetic missing readback".into(),
            },
        },
        EmailBatchItemResult {
            input_index: 0,
            outcome: EmailBatchItemOutcome::Accepted {
                provider_message_id: "provider-message-0".into(),
            },
        },
        EmailBatchItemResult {
            input_index: 1,
            outcome: EmailBatchItemOutcome::Rejected {
                message: "synthetic rejection".into(),
            },
        },
    ];
    EmailBatchResult {
        logical_fingerprint: "fixture-provider-payload-fingerprint".into(),
        chunks: vec![
            EmailBatchChunkResult {
                chunk_index: 0,
                idempotency_key: "fixture:chunk-0".into(),
                outcomes: outcomes[..2].to_vec(),
            },
            EmailBatchChunkResult {
                chunk_index: 1,
                idempotency_key: "fixture:chunk-1".into(),
                outcomes: outcomes[2..].to_vec(),
            },
        ],
        outcomes,
    }
}

fn snapshot() -> BTreeMap<String, String> {
    let mut observed = BTreeMap::new();
    for (name, validation) in [
        ("strict", EmailBatchValidation::Strict),
        ("permissive", EmailBatchValidation::Permissive),
    ] {
        let request = request(validation);
        observed.insert(
            format!("{name}.request.bytes"),
            serde_json::to_string(&request).unwrap(),
        );
        let items = serde_json::to_string(&request.items).unwrap();
        observed.insert(
            format!("{name}.items.sha256"),
            format!("{:x}", Sha256::digest(items.as_bytes())),
        );
        observed.insert(format!("{name}.items.bytes"), items);
        for (index, item) in request.items.iter().enumerate() {
            let bytes = serde_json::to_string(item).unwrap();
            observed.insert(
                format!("{name}.item.{index}.sha256"),
                format!("{:x}", Sha256::digest(bytes.as_bytes())),
            );
            observed.insert(format!("{name}.item.{index}.bytes"), bytes);
        }
    }
    let result = result();
    observed.insert(
        "result.bytes".into(),
        serde_json::to_string(&result).unwrap(),
    );
    observed.insert(
        "result.via_value.bytes".into(),
        serde_json::to_string(&serde_json::to_value(&result).unwrap()).unwrap(),
    );
    observed
}

#[test]
fn batch_storage_goldens() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/testdata/platform-batch-v1.json");
    let expected: BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(path).expect("install the captured pre-cutover golden first"),
    )
    .unwrap();
    assert_eq!(snapshot(), expected);
    for name in ["strict", "permissive"] {
        let bytes = &expected[&format!("{name}.request.bytes")];
        let decoded: EmailBatchRequest = serde_json::from_str(bytes).unwrap();
        assert_eq!(serde_json::to_string(&decoded).unwrap(), *bytes);
        let items: Vec<EmailBatchItem> =
            serde_json::from_str(&expected[&format!("{name}.items.bytes")]).unwrap();
        assert_eq!(decoded.items, items);
        assert_eq!(
            decoded.logical_fingerprint().unwrap(),
            expected[&format!("{name}.items.sha256")]
        );
    }
    let decoded: EmailBatchResult = serde_json::from_str(&expected["result.bytes"]).unwrap();
    assert_eq!(
        serde_json::to_string(&decoded).unwrap(),
        expected["result.bytes"]
    );
}
