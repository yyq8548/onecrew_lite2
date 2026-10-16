//! One name-keyed Web declaration owns lookup, typed decoding and execution class.
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

/// The host owns complete accepted futures; Creator owners admit their bounded
/// repository/CPU segments before dispatch and release them before provider I/O.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionClass {
    BusinessOwnerAsync,
    BusinessBlockingSegment,
    CreatorRead,
    CreatorWrite,
    CreatorSegments,
    ControlOwnerAsync,
    Stop,
}

/// Each declaration calls a concrete typed handler. No erased executor or
/// runtime operation ID participates in browser command execution.
macro_rules! web_commands {
    ($($name:ident : $class:ident [$($key:literal => $arg:ident),*]
        => |$state:ident, $origin:ident| $($handler:ident)::+ ($($call:expr),*);)*) => {
        #[allow(non_camel_case_types, reason = "Variant names are the exact Web wire command identifiers")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum CommandName { $($name),* }

        impl CommandName {
            pub(crate) fn lookup(name: &str) -> Option<Self> {
                match name { $(stringify!($name) => Some(Self::$name),)* _ => None }
            }

            pub(crate) const fn execution_class(self) -> ExecutionClass {
                match self { $(Self::$name => ExecutionClass::$class),* }
            }

            #[cfg(test)]
            const ALL: &'static [Self] = &[$(Self::$name),*];

            #[cfg(test)]
            const fn name(self) -> &'static str {
                match self { $(Self::$name => stringify!($name)),* }
            }

            #[cfg(test)]
            const fn argument_keys(self) -> &'static [&'static str] {
                match self { $(Self::$name => &[$($key),*]),* }
            }

            pub(crate) async fn dispatch(
                self,
                state: &std::sync::Arc<crate::ServiceState>,
                arguments: Value,
                origin: &onecrew_local_runtime::ResultOrigin,
            ) -> Result<Value, Value> {
                let mut arguments = arguments.as_object().cloned()
                    .ok_or_else(|| Value::from("command_arguments_must_be_object"))?;
                match self {
                    $(Self::$name => {
                        validate_keys(&arguments, &[$($key),*])
                            .map_err(|error| self.argument_error(error))?;
                        $(let $arg = argument(&mut arguments, $key)
                            .map_err(|error| self.argument_error(error))?;)*
                        let $state = state;
                        let $origin = origin;
                        encode(Box::pin($($handler)::+($($call),*)).await)
                    }),*
                }
            }
        }
    }
}

web_commands! {
    onecrew_table_sync_status: BusinessOwnerAsync ["campaignId" => a0]
        => |state, _origin| crate::table_sync::status(state, a0);
    onecrew_table_sync_consent: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::table_sync::change_consent(state, a0);
    onecrew_table_sync_configure: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::table_sync::configure(state, a0);
    onecrew_discovery_evidence_collect: CreatorSegments ["input" => a0]
        => |state, _origin| crate::discovery_evidence::collect(state, a0);
    onecrew_components_check: BusinessOwnerAsync []
        => |state, _origin| crate::components::check(state);
    onecrew_discovery_execution_start: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::discovery_execution::start(state, a0);
    onecrew_discovery_execution_pause: BusinessOwnerAsync ["campaignId" => a0]
        => |state, _origin| crate::discovery_execution::pause(state, a0);
    onecrew_discovery_execution_get: CreatorRead []
        => |state, _origin| crate::discovery_execution::get(state);
    onecrew_discovery_workspace_presence: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::discovery_workspace::presence(state, a0);
    onecrew_codex_plugin_status_refresh: ControlOwnerAsync []
        => |state, _origin| crate::codex_plugin_status::refresh(state);
    onecrew_discovery_advance: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::advance(state, a0);
    onecrew_discovery_intent_validate: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::opencli_discovery::validate_intent(state, a0);
    onecrew_discovery_draft_prepare: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::prepare_draft(state, a0);
    onecrew_discovery_draft_progress_get: CreatorRead ["id" => a0]
        => |state, _origin| crate::opencli_discovery::draft_progress(state, a0);
    onecrew_discovery_campaign_create: CreatorWrite ["input" => a0]
        => |state, _origin| crate::opencli_discovery::create(state, a0);
    onecrew_discovery_campaign_approve: CreatorWrite ["id" => a0, "expectedRevision" => a1]
        => |state, _origin| crate::opencli_discovery::approve(state, a0, a1);
    onecrew_discovery_budget_increase: CreatorWrite ["input" => a0]
        => |state, _origin| crate::opencli_discovery::increase_budget(state, a0);
    onecrew_discovery_target_increase: CreatorWrite ["input" => a0]
        => |state, _origin| crate::opencli_discovery::increase_target(state, a0);
    onecrew_discovery_campaign_get: CreatorRead ["id" => a0]
        => |state, _origin| crate::opencli_discovery::get(state, a0);
    onecrew_discovery_campaigns_recent: CreatorRead []
        => |state, _origin| crate::opencli_discovery::recent(state);
    onecrew_discovery_youtube_search_once: CreatorSegments ["id" => a0, "expectedRevision" => a1, "profileRef" => a2, "limit" => a3]
        => |state, _origin| crate::opencli_discovery::search_youtube_once(state, a0, a1, a2, a3);
    onecrew_discovery_search_once: CreatorSegments ["id" => a0, "expectedRevision" => a1, "platform" => a2, "profileRef" => a3, "limit" => a4]
        => |state, _origin| crate::opencli_discovery::search_once(state, a0, a1, a2, a3, a4);
    onecrew_discovery_search_recover_once: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::search_recover_once(state, a0);
    onecrew_discovery_youtube_verify_hit_once: CreatorSegments ["id" => a0, "expectedRevision" => a1, "profileRef" => a2, "searchHitId" => a3]
        => |state, _origin| crate::opencli_discovery::verify_youtube_hit_once(state, a0, a1, a2, a3);
    onecrew_discovery_search_hits_list: CreatorRead ["input" => a0]
        => |state, _origin| crate::opencli_discovery::list_hits(state, a0);
    onecrew_discovery_library_sync: BusinessOwnerAsync ["campaignId" => a0]
        => |state, _origin| crate::opencli_discovery::sync_library(state, a0);
    onecrew_discovery_transcript_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::opencli_discovery::get_transcript(state, a0);
    onecrew_discovery_profile_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::opencli_discovery::get_profile(state, a0);
    onecrew_discovery_public_page_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::opencli_discovery::get_public_page(state, a0);
    onecrew_discovery_review_material_get: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::get_review_material(state, a0);
    onecrew_discovery_review_submit: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::submit_review(state, a0);
    onecrew_discovery_review_get: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::get_review(state, a0);
    onecrew_discovery_match_evaluate: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::evaluate_match(state, a0);
    onecrew_discovery_match_reevaluate: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::reevaluate_match(state, a0);
    onecrew_discovery_identity_material_get: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::identity_material_get(state, a0);
    onecrew_discovery_identity_groups_get: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::identity_groups_get(state, a0);
    onecrew_discovery_identity_review_get: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::identity_review_get(state, a0);
    onecrew_discovery_identity_review_submit: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::identity_review_submit(state, a0);
    onecrew_discovery_identity_review_revoke: CreatorSegments ["input" => a0]
        => |state, _origin| crate::opencli_discovery::identity_review_revoke(state, a0);
    onecrew_discovery_content_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::opencli_discovery::get_content(state, a0);
    onecrew_discovery_identity_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::opencli_discovery::get_identity(state, a0);
    onecrew_discovery_candidates_list: CreatorRead ["input" => a0]
        => |state, _origin| crate::opencli_discovery::list_candidates(state, a0);
    onecrew_discovery_candidates_count: CreatorRead ["id" => a0]
        => |state, _origin| crate::opencli_discovery::count_candidates(state, a0);
    onecrew_opencli_setup_status: BusinessOwnerAsync []
        => |state, _origin| crate::opencli_onboarding::status(state);
    onecrew_opencli_bridge_recover: BusinessOwnerAsync []
        => |state, _origin| crate::opencli_onboarding::recover(state);
    onecrew_opencli_installation_repair: BusinessOwnerAsync ["approved" => a0]
        => |state, _origin| crate::opencli_onboarding::repair(state, a0);
    onecrew_opencli_setup_page_open: BusinessOwnerAsync ["page" => a0]
        => |state, _origin| crate::opencli_onboarding::open_setup_page(state, a0);
    onecrew_opencli_addons_install: BusinessOwnerAsync ["approved" => a0]
        => |state, _origin| crate::opencli_onboarding::install_addons(state, a0);
    onecrew_opencli_profile_check: BusinessOwnerAsync ["profile" => a0]
        => |state, _origin| crate::opencli_onboarding::check_profile(state, a0);
    onecrew_opencli_site_auth_check: BusinessOwnerAsync ["profile" => a0, "platform" => a1]
        => |state, _origin| crate::opencli_onboarding::check_site_auth(state, a0, a1);
    onecrew_opencli_auth_observations: BusinessOwnerAsync ["profile" => a0]
        => |state, _origin| crate::opencli_onboarding::auth_observations(state, a0);
    onecrew_opencli_login_page_open: BusinessOwnerAsync ["profile" => a0, "platform" => a1]
        => |state, _origin| crate::opencli_onboarding::open_login_page(state, a0, a1);
    onecrew_operator_account_browser_status_in_app: BusinessBlockingSegment []
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_browser_status_in_app(state);
    refresh_profile: BusinessOwnerAsync ["profileId" => a0]
        => |state, _origin| crate::operator_accounts::refresh_profile(state, a0);
    list_profiles: BusinessBlockingSegment []
        => |state, _origin| crate::operator_accounts::list_profiles(state);
    onecrew_operator_account_disconnect: BusinessBlockingSegment ["input" => a0]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_disconnect(state, a0);
    onecrew_operator_account_connection_prepare: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_prepare(state, a0);
    onecrew_operator_account_connection_begin: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_begin(state, a0);
    onecrew_operator_account_connection_status: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_status(state, a0);
    onecrew_operator_account_connection_finalize: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_finalize(state, a0);
    onecrew_operator_account_connection_cancel: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_cancel(state, a0);
    onecrew_operator_account_connection_configure_browser_in_app: BusinessOwnerAsync ["flowId" => a0, "profileName" => a1, "proxy" => a2, "xChatPreference" => a3, "xChatPasscode" => a4]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_configure_browser_in_app(state, a0, a1, a2, a3, a4);
    onecrew_operator_account_x_chat_passcode_set_in_app: BusinessBlockingSegment ["input" => a0]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_x_chat_passcode_set_in_app(state, a0);
    onecrew_operator_account_x_chat_passcode_clear_in_app: BusinessBlockingSegment ["input" => a0]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_x_chat_passcode_clear_in_app(state, a0);
    onecrew_operator_account_connection_configure_telegram_in_app: BusinessOwnerAsync ["flowId" => a0, "apiId" => a1, "apiHash" => a2, "phone" => a3, "proxy" => a4]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_configure_telegram_in_app(state, a0, a1, a2, a3, a4);
    onecrew_operator_account_connection_open_telegram_credentials_in_app: BusinessOwnerAsync ["flowId" => a0, "proxy" => a1]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_open_telegram_credentials_in_app(state, a0, a1);
    onecrew_operator_account_connection_handoff_in_app: BusinessOwnerAsync ["flowId" => a0]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_handoff_in_app(state, a0);
    onecrew_operator_account_connection_submit_telegram_code_in_app: BusinessOwnerAsync ["flowId" => a0, "code" => a1]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_submit_telegram_code_in_app(state, a0, a1);
    onecrew_operator_account_connection_submit_telegram_password_in_app: BusinessOwnerAsync ["flowId" => a0, "password" => a1]
        => |state, _origin| crate::operator_accounts::onecrew_operator_account_connection_submit_telegram_password_in_app(state, a0, a1);
    list_youtube_api_keys: BusinessBlockingSegment []
        => |state, _origin| crate::service_credentials::list_youtube_api_keys(state);
    connect_youtube_api_key: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::service_credentials::connect_youtube_api_key(state, a0);
    delete_youtube_api_key: BusinessBlockingSegment ["credentialId" => a0, "confirmed" => a1, "idempotencyKey" => a2]
        => |state, _origin| crate::service_credentials::delete_youtube_api_key(state, a0, a1, a2);
    test_youtube_api_key: BusinessOwnerAsync ["credentialId" => a0]
        => |state, _origin| crate::service_credentials::test_youtube_api_key(state, a0);
    rotate_youtube_api_key: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::service_credentials::rotate_youtube_api_key(state, a0);
    rename_youtube_api_key: BusinessBlockingSegment ["input" => a0]
        => |state, _origin| crate::service_credentials::rename_youtube_api_key(state, a0);
    onecrew_telegram_community_administrators_read: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::creator_save::onecrew_creator_save_telegram_administrators_read(state, a0);
    onecrew_creator_save_prepare: CreatorSegments ["input" => a0]
        => |state, _origin| crate::creator_save::onecrew_creator_save_prepare(state, a0, _origin);
    onecrew_creator_save_confirm: CreatorSegments ["input" => a0]
        => |state, _origin| crate::creator_save::onecrew_creator_save_confirm(state, a0, _origin);
    onecrew_creator_save_result_get: CreatorSegments ["input" => a0]
        => |state, _origin| crate::creator_save::onecrew_creator_save_result_get(state, a0, _origin);
    onecrew_creator_export: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_export(state, a0);
    onecrew_creator_import_confirm: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_import_confirm(state, a0);
    onecrew_creator_import_cancel: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_import_cancel(state, a0);
    onecrew_creator_list: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_list(state, a0);
    onecrew_creator_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_get(state, a0);
    onecrew_creator_group_list: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_group_list(state, a0);
    onecrew_creator_group_create: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_group_create(state, a0);
    onecrew_creator_group_update: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_group_update(state, a0);
    onecrew_creator_group_delete: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_group_delete(state, a0);
    onecrew_creator_group_set: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_group_set(state, a0);
    onecrew_creator_group_bulk_move: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_group_bulk_move(state, a0);
    onecrew_creator_manual_create: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_manual_create(state, a0);
    onecrew_creator_contact_route_add: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_contact_route_add(state, a0);
    onecrew_creator_contact_route_update: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_contact_route_update(state, a0);
    onecrew_creator_contact_route_delete_prepare: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_contact_route_delete_prepare(state, a0);
    onecrew_creator_contact_route_delete_confirm: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_contact_route_delete_confirm(state, a0);
    onecrew_creator_pin: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_pin(state, a0);
    onecrew_creator_update_stage: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_update_stage(state, a0);
    onecrew_creator_update: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_update(state, a0);
    onecrew_creator_archive: CreatorWrite ["creatorId" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_archive(state, a0);
    onecrew_creator_delete: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_creators::onecrew_creator_delete(state, a0);
    onecrew_contact_target_resolve: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::onecrew_contact_targets::onecrew_contact_target_resolve(state, a0);
    onecrew_relationship_update_prepare: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::onecrew_relationships::onecrew_relationship_update_prepare(state, a0, _origin);
    onecrew_outreach_message_prepare: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::onecrew_outreach::onecrew_outreach_message_prepare(state, a0);
    onecrew_outreach_message_send_prepare: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::onecrew_outreach::onecrew_outreach_message_send_prepare(state, a0, _origin);
    onecrew_outreach_confirm: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::outreach_review::onecrew_outreach_confirm(state, a0, _origin);
    onecrew_outreach_result_get: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::outreach_review::onecrew_outreach_result_get(state, a0, _origin);
    onecrew_outreach_cancel: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::outreach_review::onecrew_outreach_cancel(state, a0, _origin);
    onecrew_outreach_action_status: BusinessOwnerAsync ["input" => a0]
        => |state, _origin| crate::onecrew_outreach::onecrew_outreach_action_status(state, a0);
    onecrew_contact_route_last_contact_account: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_outreach::onecrew_contact_route_last_contact_account(state, a0);
    onecrew_conversation_list: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_outreach::onecrew_conversation_list(state, a0);
    onecrew_conversation_sync: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_outreach::onecrew_conversation_sync(state, a0);
    onecrew_conversation_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_outreach::onecrew_conversation_get(state, a0);
    onecrew_inbox_attention: CreatorRead []
        => |state, _origin| crate::onecrew_outreach::onecrew_inbox_attention(state);
    onecrew_email_mailbox_list: CreatorRead []
        => |state, _origin| crate::onecrew_email::onecrew_email_mailbox_list(state);
    onecrew_gmail_bulk_recipients: CreatorSegments []
        => |state, _origin| crate::onecrew_email::onecrew_gmail_bulk_recipients(state);
    onecrew_gmail_bulk_list: CreatorSegments []
        => |state, _origin| crate::onecrew_email::onecrew_gmail_bulk_list(state);
    onecrew_gmail_bulk_prepare: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_gmail_bulk_prepare(state, a0);
    onecrew_gmail_bulk_control: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_gmail_bulk_control(state, a0);
    onecrew_gmail_connect_url: BusinessOwnerAsync []
        => |state, _origin| crate::onecrew_email::onecrew_gmail_connect_url(state);
    onecrew_gmail_refresh: BusinessOwnerAsync []
        => |state, _origin| crate::onecrew_email::onecrew_gmail_refresh(state);
    onecrew_mail_access_connections: BusinessOwnerAsync []
        => |state, _origin| crate::onecrew_email::onecrew_mail_access_connections(state);
    onecrew_email_mailbox_sync: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_mailbox_sync(state, a0);
    onecrew_email_mailbox_sync_status: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_mailbox_sync_status(state, a0);
    onecrew_email_folder_list: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_folder_list(state, a0);
    onecrew_email_thread_list: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_thread_list(state, a0);
    onecrew_email_thread_update_disposition: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_thread_update_disposition(state, a0);
    onecrew_email_thread_mark_read: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_thread_mark_read(state, a0);
    onecrew_email_thread_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_thread_get(state, a0);
    onecrew_email_template_list: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_template_list(state, a0);
    onecrew_email_template_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_template_get(state, a0);
    onecrew_email_template_save: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_template_save(state, a0);
    onecrew_email_template_delete: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_template_delete(state, a0);
    onecrew_email_template_render: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_template_render(state, a0);
    onecrew_email_draft_list: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_draft_list(state, a0);
    onecrew_email_draft_get: CreatorRead ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_draft_get(state, a0);
    onecrew_email_draft_save: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_draft_save(state, a0);
    onecrew_email_draft_trash: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_draft_trash(state, a0);
    onecrew_email_draft_restore: CreatorWrite ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_draft_restore(state, a0);
    onecrew_email_draft_send: CreatorSegments ["input" => a0]
        => |state, _origin| crate::onecrew_email::onecrew_email_draft_send(state, a0);
    toolkit_directory: BusinessOwnerAsync []
        => |state, _origin| crate::toolkit_directory::toolkit_directory(state);
    set_profile_proxy: BusinessOwnerAsync ["profileId" => a0, "proxy" => a1]
        => |state, _origin| crate::proxy::set_profile_proxy(state, a0, a1);
    clear_profile_proxy: BusinessBlockingSegment ["profileId" => a0]
        => |state, _origin| crate::proxy::clear_profile_proxy(state, a0);
    probe_proxy: BusinessOwnerAsync ["proxy" => a0]
        => |state, _origin| crate::proxy::probe_proxy(state, a0);
    creator_sync_pairing_status: ControlOwnerAsync []
        => |state, _origin| crate::web::creator_sync_pairing_status(state);
    creator_sync_pairing_begin: ControlOwnerAsync []
        => |state, _origin| crate::web::creator_sync_pairing_begin(state);
    creator_sync_pairing_reopen: ControlOwnerAsync []
        => |state, _origin| crate::web::creator_sync_pairing_reopen(state);
    creator_sync_pairing_cancel: ControlOwnerAsync []
        => |state, _origin| crate::web::creator_sync_pairing_cancel(state);
    onecrew_logout: ControlOwnerAsync []
        => |state, _origin| crate::web::onecrew_logout(state);
    local_data_status: ControlOwnerAsync []
        => |state, _origin| crate::web::local_data_status(state);
    service_status: ControlOwnerAsync []
        => |state, _origin| crate::web::service_status(state);
    service_stop: Stop []
        => |state, _origin| crate::web::service_stop(state);
    set_application_locale: ControlOwnerAsync ["locale" => locale]
        => |state, _origin| crate::web::set_application_locale(state, locale);
}

impl CommandName {
    fn argument_error(self, error: Value) -> Value {
        // Preserve the existing locale command's single validation error.
        if self == Self::set_application_locale {
            Value::from("invalid_locale")
        } else {
            error
        }
    }
}

fn validate_keys(arguments: &Map<String, Value>, keys: &[&str]) -> Result<(), Value> {
    if arguments.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err(Value::from("unexpected_command_argument"));
    }
    Ok(())
}

fn argument<T: DeserializeOwned>(
    arguments: &mut Map<String, Value>,
    name: &str,
) -> Result<T, Value> {
    serde_json::from_value(arguments.remove(name).unwrap_or(Value::Null))
        .map_err(|_| serde_json::json!({"code":"invalid_command_argument","parameter":name}))
}

fn encode<T: Serialize, E: Serialize>(result: Result<T, E>) -> Result<Value, Value> {
    match result {
        Ok(value) => {
            serde_json::to_value(value).map_err(|_| Value::from("command_result_unavailable"))
        }
        Err(error) => {
            Err(serde_json::to_value(error).unwrap_or_else(|_| Value::from("command_failed")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dm_command_failure_serializes_native_evidence_without_effect_or_retry_inference() {
        use onecrew_local_runtime::{DmDiagnostic, DmFailure, OutreachError};
        for message in ["login", "429", "sent successfully", "timeout"] {
            let mut diagnostic = DmDiagnostic::provider("test", "test_private_protocol", message);
            diagnostic.code = Some("429".into());
            diagnostic.http_status = Some(409);
            let error = crate::onecrew_outreach::WebDirectMessageError::from_outreach(
                OutreachError::DirectMessage(DmFailure {
                    stage: "message_capability",
                    diagnostic,
                }),
            );
            let wire = encode::<(), _>(Err(error)).unwrap_err();
            assert_eq!(
                wire,
                json!({
                    "code": "direct_message_failed", "message": message,
                    "failure": { "stage": "message_capability", "diagnostic": {
                        "platform": "test", "protocol": "test_private_protocol", "origin": "provider",
                        "code": "429", "http_status": 409, "rpc_code": null, "message": message,
                    }},
                })
            );
        }
    }

    #[test]
    fn web_command_declarations_are_the_exact_allowlist_and_host_classes() {
        let names = CommandName::ALL
            .iter()
            .map(|command| command.name())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names.len(), 152);
        assert_eq!(names.len(), CommandName::ALL.len());
        let mut wire = CommandName::ALL
            .iter()
            .map(|command| json!({"name": command.name(), "keys": command.argument_keys()}))
            .collect::<Vec<_>>();
        wire.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
        let expected: Value =
            serde_json::from_str(include_str!("web_commands.fixture.json")).unwrap();
        assert_eq!(json!(wire), expected);
        for command in CommandName::ALL {
            assert_eq!(CommandName::lookup(command.name()), Some(*command));
        }
        for forbidden in [
            "",
            "onecrew.creator.get",
            "youtube.videos.search",
            "local_data_upgrade",
            "onecrew_posts_list",
            "onecrew_discovery_candidate_approve",
        ] {
            assert_eq!(CommandName::lookup(forbidden), None);
        }
        assert_eq!(
            CommandName::service_stop.execution_class(),
            ExecutionClass::Stop
        );
        assert_eq!(
            CommandName::list_profiles.execution_class(),
            ExecutionClass::BusinessBlockingSegment
        );
        assert_eq!(
            CommandName::service_status.execution_class(),
            ExecutionClass::ControlOwnerAsync
        );
        assert_eq!(
            CommandName::onecrew_codex_plugin_status_refresh.execution_class(),
            ExecutionClass::ControlOwnerAsync
        );
        assert_eq!(
            CommandName::onecrew_opencli_site_auth_check.execution_class(),
            ExecutionClass::BusinessOwnerAsync
        );
        assert_eq!(
            CommandName::onecrew_discovery_intent_validate.execution_class(),
            ExecutionClass::BusinessOwnerAsync
        );
        assert_eq!(
            CommandName::onecrew_discovery_draft_prepare.execution_class(),
            ExecutionClass::CreatorSegments
        );
        assert_eq!(
            CommandName::onecrew_creator_get.execution_class(),
            ExecutionClass::CreatorRead
        );
        assert_eq!(
            CommandName::onecrew_discovery_identity_get.execution_class(),
            ExecutionClass::CreatorRead
        );
        assert_eq!(
            CommandName::onecrew_discovery_public_page_get.execution_class(),
            ExecutionClass::CreatorRead
        );
        assert_eq!(
            CommandName::onecrew_discovery_review_material_get.execution_class(),
            ExecutionClass::CreatorSegments
        );
        assert_eq!(
            CommandName::onecrew_discovery_review_get.execution_class(),
            ExecutionClass::CreatorSegments
        );
        assert_eq!(
            CommandName::onecrew_discovery_review_submit.execution_class(),
            ExecutionClass::CreatorSegments
        );
        assert_eq!(
            CommandName::onecrew_creator_manual_create.execution_class(),
            ExecutionClass::CreatorWrite
        );
        assert_eq!(
            CommandName::onecrew_creator_export.execution_class(),
            ExecutionClass::CreatorSegments
        );
    }

    #[tokio::test]
    async fn web_command_typed_decode_preserves_parameters_and_error_shapes() {
        let directory = tempfile::tempdir().unwrap();
        let state = crate::ServiceState::new(
            directory.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        let origin = onecrew_local_runtime::ResultOrigin::new("web:decode-test").unwrap();
        for input in [
            json!({}),
            json!(17),
            json!({"id":"draft", "brief":"shoes", "profile_ref":"browser",
            "opencli_version":"1.8.8","target_count":30,"max_calls":200,"command":"must-not-run"}),
        ] {
            assert_eq!(
                CommandName::onecrew_discovery_draft_prepare
                    .dispatch(&state, json!({"input":input}), &origin)
                    .await,
                Err(json!({"code":"invalid_command_argument","parameter":"input"}))
            );
        }
        for args in [json!({}), json!({"approved": "true"})] {
            assert_eq!(
                CommandName::onecrew_opencli_installation_repair
                    .dispatch(&state, args.clone(), &origin)
                    .await,
                Err(json!({"code":"invalid_command_argument", "parameter":"approved"}))
            );
            assert_eq!(
                CommandName::onecrew_opencli_addons_install
                    .dispatch(&state, args, &origin)
                    .await,
                Err(json!({"code":"invalid_command_argument", "parameter":"approved"}))
            );
        }
        assert_eq!(
            CommandName::list_profiles
                .dispatch(&state, json!([]), &origin)
                .await,
            Err(json!("command_arguments_must_be_object"))
        );
        assert_eq!(
            CommandName::list_profiles
                .dispatch(&state, json!({"input":{}}), &origin)
                .await,
            Err(json!("unexpected_command_argument"))
        );
        assert_eq!(
            CommandName::onecrew_creator_get
                .dispatch(&state, json!({"input":17}), &origin)
                .await,
            Err(json!({"code":"invalid_command_argument", "parameter":"input"}))
        );
        for input in [
            json!({}),
            json!(17),
            json!({"campaign_id":"campaign","candidate_id":"candidate",
            "requested_url":"https://example.com/tool","unexpected":"must-not-echo"}),
        ] {
            assert_eq!(
                CommandName::onecrew_discovery_public_page_get
                    .dispatch(&state, json!({"input":input}), &origin)
                    .await,
                Err(json!({"code":"invalid_command_argument", "parameter":"input"}))
            );
        }
        for input in [
            json!({}),
            json!(17),
            json!({"campaign_id":"campaign","expected_revision":1,"search_hit_id":"hit","verdict":"secret-sentinel"}),
        ] {
            assert_eq!(
                CommandName::onecrew_discovery_review_material_get
                    .dispatch(&state, json!({"input":input}), &origin)
                    .await,
                Err(json!({"code":"invalid_command_argument","parameter":"input"}))
            );
        }
        assert_eq!(CommandName::delete_youtube_api_key.dispatch(&state,
            json!({"credentialId":"fixture", "confirmed":"yes", "idempotencyKey":"fixture"}), &origin).await,
            Err(json!({"code":"invalid_command_argument", "parameter":"confirmed"})));
        assert_eq!(
            CommandName::delete_youtube_api_key
                .dispatch(
                    &state,
                    json!({"credentialId":"fixture", "confirmed":true}),
                    &origin
                )
                .await,
            Err(json!({"code":"invalid_command_argument", "parameter":"idempotencyKey"}))
        );
        assert_eq!(
            CommandName::set_application_locale
                .dispatch(&state, json!({"locale":"unsupported"}), &origin)
                .await,
            Err(json!("invalid_locale"))
        );
        assert_eq!(
            CommandName::set_application_locale
                .dispatch(&state, json!({"locale":"en-US", "extra":true}), &origin)
                .await,
            Err(json!("invalid_locale"))
        );
        assert_eq!(
            CommandName::set_application_locale
                .dispatch(&state, json!({"locale":"en-US"}), &origin)
                .await,
            Ok(Value::Null)
        );
    }

    /// Frozen proxy decoding for the five Web proxy commands: each rejects the
    /// same malformed proxies as its `proxy` parameter, and an accepted proxy
    /// keeps every field (the password is hidden only in Debug output).
    #[tokio::test]
    async fn web_proxy_commands_decode_the_frozen_proxy_json() {
        use onecrew_local_runtime::ProxyConfig;

        let directory = tempfile::tempdir().unwrap();
        let state = crate::ServiceState::new(
            directory.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        let origin = onecrew_local_runtime::ResultOrigin::new("web:proxy-decode-test").unwrap();
        let commands = |proxy: &Value| {
            [
                (
                    CommandName::set_profile_proxy,
                    json!({"profileId": "fixture", "proxy": proxy}),
                ),
                (CommandName::probe_proxy, json!({"proxy": proxy})),
                (
                    CommandName::onecrew_operator_account_connection_configure_browser_in_app,
                    json!({"flowId": "fixture", "profileName": "operator", "proxy": proxy,
                        "xChatPreference": "disabled", "xChatPasscode": null}),
                ),
                (
                    CommandName::onecrew_operator_account_connection_configure_telegram_in_app,
                    json!({"flowId": "fixture", "apiId": 12345, "apiHash": "api-hash",
                        "phone": null, "proxy": proxy}),
                ),
                (
                    CommandName::onecrew_operator_account_connection_open_telegram_credentials_in_app,
                    json!({"flowId": "fixture", "proxy": proxy}),
                ),
            ]
        };
        for proxy in [
            json!({"kind": "HTTP", "host": "proxy.example", "port": 8080}),
            json!({"kind": "socks4", "host": "proxy.example", "port": 1080}),
            json!({"kind": "http", "port": 8080}),
            json!({"kind": "http", "host": "proxy.example", "port": 65536}),
            json!({"kind": "http", "host": "proxy.example", "port": "8080"}),
            json!({"kind": "http", "host": "proxy.example", "port": 8080,
                "username": "proxy-user", "password": 42}),
            json!("http://proxy-user:proxy-password@proxy.example:8080"),
        ] {
            for (command, arguments) in commands(&proxy) {
                assert_eq!(
                    command.dispatch(&state, arguments, &origin).await,
                    Err(json!({"code": "invalid_command_argument", "parameter": "proxy"})),
                    "{command:?}: {proxy}"
                );
            }
        }
        for (proxy, debug, password) in [
            (
                json!({"kind": "http", "host": "proxy.example", "port": 8080,
                    "username": "proxy-user", "password": "proxy-password"}),
                r#"ProxyConfig { kind: Http, host: "proxy.example", port: 8080, username: Some("proxy-user"), password: Some("[REDACTED]") }"#,
                Some("proxy-password"),
            ),
            (
                json!({"kind": "socks5", "host": "socks.example", "port": 1080,
                    "username": null, "password": null}),
                r#"ProxyConfig { kind: Socks5, host: "socks.example", port: 1080, username: None, password: None }"#,
                None,
            ),
            (
                json!({"kind": "https", "host": "secure.example", "port": 443, "label": "ignored"}),
                r#"ProxyConfig { kind: Https, host: "secure.example", port: 443, username: None, password: None }"#,
                None,
            ),
        ] {
            let mut arguments = Map::from_iter([("proxy".to_owned(), proxy.clone())]);
            let decoded: ProxyConfig = argument(&mut arguments, "proxy").unwrap();
            assert_eq!(
                (format!("{decoded:?}"), decoded.password.as_deref()),
                (debug.to_owned(), password)
            );
            // The browser command decodes its proxy before the next parameter.
            assert_eq!(
                CommandName::onecrew_operator_account_connection_configure_browser_in_app
                    .dispatch(
                        &state,
                        json!({"flowId": "fixture", "profileName": "operator", "proxy": proxy,
                            "xChatPreference": "maybe", "xChatPasscode": null}),
                        &origin
                    )
                    .await,
                Err(json!({"code": "invalid_command_argument", "parameter": "xChatPreference"}))
            );
        }
        let mut absent = Map::from_iter([("proxy".to_owned(), Value::Null)]);
        assert!(matches!(
            argument::<Option<ProxyConfig>>(&mut absent, "proxy"),
            Ok(None)
        ));
    }
}
