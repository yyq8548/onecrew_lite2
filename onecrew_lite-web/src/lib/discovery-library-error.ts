/** Translate only exact owner codes; raw errors can contain private service data. */
export function discoveryLibraryFailureKey(error: unknown) {
  const code = error instanceof Error ? error.message : error;
  switch (code) {
    case "access.remote_unavailable": return "libraryAccessUnavailable";
    case "library_runtime_unavailable":
    case "library_request_unavailable":
    case "library_service_unavailable": return "libraryUnavailable";
    case "library_login_required": return "libraryLoginRequired";
    case "library_authorization_pending": return "libraryAuthorizationPending";
    case "library_auth_unavailable": return "libraryAuthUnavailable";
    case "library_account_mismatch":
    case "library_account_changed": return "libraryAccountRequired";
    case "web_session_required": return "libraryWebSessionRequired";
    case "library_sync_upgrade_required": return "libraryUpgradeRequired";
    case "library_instance_changed": return "libraryInstanceChanged";
    case "library_source_stale": return "librarySourceStale";
    case "library_write_failed":
    case "library_readback_incomplete":
    case "library_sync_interrupted": return "libraryOutcomeUncertain";
    case "library_projection_conflict":
    case "library_row_identity_changed":
    case "library_foreign_source_row":
    case "library_table_schema_changed":
    case "library_unmanaged_table":
    case "library_row_missing":
    case "library_table_missing": return "libraryProjectionConflict";
    default: return "librarySyncFailed";
  }
}
