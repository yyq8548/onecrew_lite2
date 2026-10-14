//! Creator account reference projections; credentials remain with the local vault.

use std::sync::Arc;

use crate::{
    AccountAvailability, AccountKind, AccountRef, AccountRefId, CreatorRepositoryCallError,
    Platform,
};

impl crate::CreatorRuntime {
    pub fn sync_account_ref(
        &self,
        account_ref_id: AccountRefId,
        platform: Platform,
        kind: &str,
        label: &str,
    ) -> Result<AccountRef, String> {
        self.repository
            .upsert_account_ref(account_ref_id, platform, kind, label)
    }

    pub fn sync_account_ref_with_state(
        &self,
        account_ref_id: AccountRefId,
        platform: Platform,
        kind: AccountKind,
        label: &str,
        availability: AccountAvailability,
    ) -> Result<AccountRef, String> {
        self.repository.upsert_account_ref_with_state(
            account_ref_id,
            platform,
            kind,
            label,
            availability,
        )
    }

    pub fn get_account_ref(&self, account_ref_id: &AccountRefId) -> Result<AccountRef, String> {
        self.repository.get_account_ref(account_ref_id)
    }

    pub fn list_account_refs(
        &self,
        platform: Platform,
        kind: AccountKind,
    ) -> Result<Vec<AccountRef>, String> {
        self.repository.list_account_refs(platform, kind)
    }

    /// The admission gate of one platform connection, read under bounded read
    /// admission: the reference's lifecycle marker (`active`, cleared durably
    /// by a disconnect or credential-delete claim even when the vault delete
    /// later fails) and its availability, which the runtime checks before it
    /// opens any provider facade. `None` when no reference with this id
    /// exists for the platform and kind.
    ///
    /// # Errors
    ///
    /// Returns [`CreatorRepositoryCallError::Owner`] for the repository's own
    /// read failure, or [`CreatorRepositoryCallError::Dispatch`] when
    /// admission is closed or overloaded or the blocking result is lost. The
    /// read has no repository effect, so a caller may retry a dispatch
    /// failure; a gate that cannot be read admits nothing.
    pub async fn read_account_ref_gate(
        &self,
        platform: Platform,
        kind: AccountKind,
        account_ref_id: AccountRefId,
    ) -> Result<Option<AccountRef>, CreatorRepositoryCallError<String>> {
        let repository = Arc::clone(&self.repository);
        self.repository_read_segment(move || {
            repository
                .list_account_refs(platform, kind)
                .map(|accounts| {
                    accounts
                        .into_iter()
                        .find(|account| account.id == account_ref_id)
                })
        })
        .await
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        AccountAvailability, AccountKind, AccountRefId, CreatorGateways, CreatorRuntime, Platform,
    };

    #[tokio::test]
    async fn the_gate_read_reports_the_durable_lifecycle_marker_of_an_open_disconnect_claim() {
        let directory = tempfile::tempdir().expect("temp directory");
        let runtime =
            CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
        let account_ref_id = AccountRefId::new(uuid::Uuid::new_v4().to_string());
        runtime
            .sync_account_ref_with_state(
                account_ref_id.clone(),
                Platform::X,
                AccountKind::Profile,
                "operator",
                AccountAvailability::Available,
            )
            .expect("account ref");

        let admitted = runtime
            .read_account_ref_gate(Platform::X, AccountKind::Profile, account_ref_id.clone())
            .await
            .expect("gate read")
            .expect("the reference exists");
        assert!(admitted.active);
        assert_eq!(admitted.availability, AccountAvailability::Available);

        // The platform and kind are part of the gate's identity.
        assert!(
            runtime
                .read_account_ref_gate(
                    Platform::X,
                    AccountKind::ServiceCredential,
                    account_ref_id.clone()
                )
                .await
                .expect("gate read")
                .is_none()
        );

        // A disconnect claim clears `active` durably before the vault delete
        // runs, so an interrupted or failed delete still reads as refused.
        runtime
            .begin_operator_account_disconnect("disconnect-1", &account_ref_id)
            .expect("claim");
        let closed = runtime
            .read_account_ref_gate(Platform::X, AccountKind::Profile, account_ref_id)
            .await
            .expect("gate read")
            .expect("the reference still exists while the claim is open");
        assert!(!closed.active);
    }
}
