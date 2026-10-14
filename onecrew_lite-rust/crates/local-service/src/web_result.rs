//! Trusted browser result binding shared by the actual prepare/confirm peers.
use onecrew_local_runtime::{
    ResultBinding, ResultFingerprint, ResultOrigin, ResultRef, ResultViewBinding,
};
use serde::Deserialize;

/// Browser echoes the reviewed identity, but never supplies its authority scope.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WebResultBinding {
    result_ref: ResultRef,
    view: ResultViewBinding,
    fingerprint: ResultFingerprint,
    revision: u64,
}

impl WebResultBinding {
    pub(super) fn bind(self, origin: &ResultOrigin) -> Result<ResultBinding, String> {
        if self.revision == 0 {
            return Err("result_revision_invalid".into());
        }
        Ok(ResultBinding {
            result_ref: self.result_ref,
            view: self.view,
            fingerprint: self.fingerprint,
            revision: self.revision,
            origin: origin.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_accepts_only_the_review_binding_and_injects_server_origin() {
        let wire = serde_json::json!({
            "result_ref": format!("rr_{}", "a".repeat(32)),
            "view": {"identity": format!("rv_{}", "b".repeat(32)), "kind": "confirmation"},
            "fingerprint": format!("sha256:{}", "c".repeat(64)), "revision": 1,
        });
        let origin = ResultOrigin::new("web:authenticated-session").unwrap();
        let binding = serde_json::from_value::<WebResultBinding>(wire.clone())
            .unwrap()
            .bind(&origin)
            .unwrap();
        assert_eq!(binding.origin, origin);
        for field in [
            "body",
            "target",
            "operator_account_ref_id",
            "confirmed",
            "origin",
        ] {
            let mut modified = wire.clone();
            modified[field] = serde_json::json!("caller-controlled");
            assert!(
                serde_json::from_value::<WebResultBinding>(modified).is_err(),
                "{field}"
            );
        }
    }
}
