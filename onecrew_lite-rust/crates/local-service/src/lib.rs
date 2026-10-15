//! OneCrew's local application host. This crate composes the existing runtime
//! owners and exposes authenticated web and MCP adapters. It neither starts
//! Codex nor owns another Creator model.

mod blocking;
mod cli_installation;
mod cli_update_status;
mod client;
mod codex_plugin_status;
mod commands;
mod components;
mod creator_save;
mod creator_snapshot_supervisor;
mod device_name;
mod discovery_evidence;
mod discovery_execution;
mod discovery_metrics;
mod discovery_workspace;
mod gmail_bulk_worker;
mod host;
mod ipc;
mod login;
mod mcp;
mod onecrew_contact_targets;
mod onecrew_creators;
mod onecrew_email;
mod onecrew_outreach;
mod onecrew_relationships;
mod opencli_discovery;
mod opencli_onboarding;
mod operator_accounts;
mod outreach_review;
mod pairing_loopback;
mod paths;
mod private_file;
mod proxy;
mod runtime_binary;
mod runtime_changes;
mod service_credentials;
mod service_host;
mod startup;
mod state;
mod status;
mod table_sync;
mod toolkit_directory;
mod update_lease;
mod web;
mod web_result;
mod work;
mod workspace;
mod workspace_card_requests;

#[cfg(test)]
mod presentation_vocabulary_tests;

use state::ServiceState;

pub use cli_installation::{
    UserInstallLayout, current_cli_target, default_cli_installation, is_managed_cli_copy,
};
pub use client::{ServiceClient, ensure_service, service_start_preflight};
pub use host::{ServiceOptions, run_service};
pub use paths::default_data_dir;
pub use runtime_binary::{copy_with_origin_metadata, sync_copied_file};
pub use service_host::{configure_service_host, run_service_host};
pub use update_lease::{ServiceUpdateLease, stop_for_update};
pub use workspace::library_workspace_url;
