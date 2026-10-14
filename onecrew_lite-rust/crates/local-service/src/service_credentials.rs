use onecrew_local_domain::ServiceCredential;
use onecrew_local_runtime::{
    RenameYoutubeCredentialInput, ResendCredentialConnectInput, ResendCredentialDeleteInput,
    ResendCredentialDeletionError, ResendCredentialTest, RotateYoutubeCredentialInput,
    ServiceCredentialConnectResult, ServiceCredentialProjection, YoutubeApiKeyConnectInput,
    YoutubeCredentialDeleteInput, YoutubeCredentialDeleteOutcome, YoutubePlatformTestInput,
    YoutubePlatformTestResult,
};

use crate::{ServiceState, blocking::run_web_blocking};

pub(super) async fn list_youtube_api_keys(
    state: &ServiceState,
) -> Result<Vec<ServiceCredential>, String> {
    let platform = state.platform().await?;
    run_web_blocking("list_youtube_api_keys", move || {
        platform.list_youtube_api_keys()
    })
    .await?
}

pub(super) async fn connect_youtube_api_key(
    state: &ServiceState,
    input: YoutubeApiKeyConnectInput,
) -> Result<ServiceCredentialConnectResult, String> {
    state.platform().await?.connect_youtube_api_key(input).await
}

pub(super) async fn delete_youtube_api_key(
    state: &ServiceState,
    credential_id: String,
    confirmed: bool,
    idempotency_key: String,
) -> Result<YoutubeCredentialDeleteOutcome, String> {
    let platform = state.platform().await?;
    run_web_blocking("delete_youtube_api_key", move || {
        platform
            .delete_youtube_credential(YoutubeCredentialDeleteInput {
                credential_id,
                confirmed,
                idempotency_key,
            })
            .map_err(|error| error.safe_message().to_string())
    })
    .await?
}

pub(super) async fn test_youtube_api_key(
    state: &ServiceState,
    credential_id: String,
) -> Result<YoutubePlatformTestResult, String> {
    state
        .platform()
        .await?
        .platform_youtube_test(YoutubePlatformTestInput { credential_id })
        .await
        .map_err(|error| error.safe_message().to_string())
}

pub(super) async fn list_resend_credentials(
    state: &ServiceState,
) -> Result<Vec<ServiceCredential>, String> {
    let platform = state.platform().await?;
    run_web_blocking("list_resend_credentials", move || {
        platform.list_resend_credentials()
    })
    .await?
}

pub(super) async fn connect_resend_credential(
    state: &ServiceState,
    input: ResendCredentialConnectInput,
) -> Result<ServiceCredentialConnectResult, String> {
    state
        .platform()
        .await?
        .connect_resend_credential(input)
        .await
}

pub(super) async fn delete_resend_credential(
    state: &ServiceState,
    input: ResendCredentialDeleteInput,
) -> Result<bool, ResendCredentialDeletionError> {
    let platform = state
        .platform()
        .await
        .map_err(|message| ResendCredentialDeletionError::Runtime { message })?;
    run_web_blocking("delete_resend_credential", move || {
        platform.delete_resend_credential(input)
    })
    .await
    .map_err(|message| ResendCredentialDeletionError::Runtime { message })?
}

pub(super) async fn rotate_youtube_api_key(
    state: &ServiceState,
    input: RotateYoutubeCredentialInput,
) -> Result<ServiceCredentialConnectResult, String> {
    state
        .platform()
        .await?
        .rotate_youtube_credential(input)
        .await
}

pub(super) async fn rename_youtube_api_key(
    state: &ServiceState,
    input: RenameYoutubeCredentialInput,
) -> Result<ServiceCredentialProjection, String> {
    let platform = state.platform().await?;
    run_web_blocking("rename_youtube_api_key", move || {
        platform.rename_youtube_credential(input)
    })
    .await?
}

pub(super) async fn test_resend_credential(
    state: &ServiceState,
    credential_id: String,
) -> Result<ResendCredentialTest, String> {
    state
        .platform()
        .await?
        .test_resend_credential(&credential_id)
        .await
}
