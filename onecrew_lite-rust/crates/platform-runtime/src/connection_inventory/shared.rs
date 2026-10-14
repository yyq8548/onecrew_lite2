use std::time::SystemTime;

use onecrew_local_domain::ServiceCredential;
use onecrew_local_vault::LocalVault;

use super::LocalConnectionAvailability;
use crate::{OperatorAccountCredentialAvailability, OperatorAccountProjection};

pub(super) fn local_availability(is_available: bool) -> LocalConnectionAvailability {
    if is_available {
        LocalConnectionAvailability::Available
    } else {
        LocalConnectionAvailability::Unavailable
    }
}

pub(super) fn operator_is_locally_available(account: &OperatorAccountProjection) -> bool {
    account.configured
        && account.credential_availability == OperatorAccountCredentialAvailability::Available
}

pub(super) fn service_credential_is_locally_available(
    vault: &LocalVault,
    credential: &ServiceCredential,
) -> bool {
    !credential.disabled
        && credential.last_error.is_none()
        && !has_active_cooldown(credential)
        && vault.get_service_credential(credential.id).is_ok()
}

fn has_active_cooldown(credential: &ServiceCredential) -> bool {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    credential.cooldown_until.is_some_and(|until| until > now)
}
