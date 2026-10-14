/* Generated. Do not edit. */

export interface LoginProjection {
  browser_opened: boolean;
  callback_received: boolean;
  flow_id: string | null;
  issue:
    | (
        | "device_name_unavailable"
        | "device_name_invalid"
        | "callback_expired"
        | "canceled"
        | "replay"
        | "owner_changed"
        | "pairing_rejected"
        | "recovery_required"
      )
    | null;
  status: {
    action: ("begin_pairing" | "await_pairing_callback" | "cancel_pairing" | "validate_admission" | "retry") | null;
    credential_id: string | null;
    installation_id: string | null;
    issue:
      | (
          | "configuration_unavailable"
          | "credential_unavailable"
          | "pairing_rejected"
          | "pairing_expired"
          | "pairing_cancelled"
          | "admission_validation_required"
          | "safe_cleanup_incomplete"
          | "enrollment_unknown"
          | "verification_failed"
          | "remote_unavailable"
          | "remote_rate_limited"
          | "remote_unauthorized"
          | "remote_revoked"
          | "protocol_mismatch"
          | "local_vault_unavailable"
        )
      | null;
    state:
      | "unconfigured"
      | "pairing_required"
      | "pairing_pending"
      | "pairing_completing"
      | "validation_required"
      | "validating"
      | "active"
      | "revoked"
      | "unauthorized"
      | "recovery_required"
      | "temporarily_unavailable";
  };
}
