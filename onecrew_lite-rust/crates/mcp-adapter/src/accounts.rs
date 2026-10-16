//! Inbound MCP port for platform-specific account connection workflows.
//!
//! The MCP adapter accepts credentials the user explicitly supplies to the
//! local server, but it never persists or projects them itself. Every call is
//! delegated to the same [`LocalPlatform`] use case used by the desktop GUI;
//! only secret-free runtime results cross back through MCP.

use std::sync::Arc;

use async_trait::async_trait;
use onecrew_local_runtime::{
    AccountConnectionOperationInput, BrowserAccountConnectionCompleteInput,
    BrowserAccountConnectionStartInput, BrowserAccountConnectionStatus, ClearXChatPasscodeInput,
    DirectAccountConnectionCancelInput, LocalPlatform, OperatorAccountConnectionBeginInput,
    OperatorAccountConnectionCancelInput, OperatorAccountConnectionFinalizeInput,
    OperatorAccountConnectionPrepareInput, OperatorAccountConnectionStatus,
    OperatorAccountConnectionStatusInput, OperatorAccountDisconnectError,
    OperatorAccountDisconnectInput, OperatorAccountDisconnectOutcome, OperatorAccountProjection,
    RenameYoutubeCredentialInput, ServiceCredentialConnectResult,
    ServiceCredentialProjection, SetXChatPasscodeInput, TelegramAccountConnectionOtpInput,
    TelegramAccountConnectionStartInput, TelegramAccountConnectionStatus,
    TelegramAccountConnectionTwoFactorInput, YoutubeApiKeyConnectInput,
    YoutubeCredentialDeleteError, YoutubeCredentialDeleteInput, YoutubeCredentialDeleteOutcome,
};

use crate::operation_registry::{BlockingTaskFailure, run_blocking};

#[async_trait]
pub trait OperatorAccountWorkflow: Send + Sync {
    async fn list(&self) -> Result<Vec<OperatorAccountProjection>, String>;

    async fn browser_start(
        &self,
        input: BrowserAccountConnectionStartInput,
    ) -> Result<BrowserAccountConnectionStatus, String>;
    async fn browser_status(
        &self,
        input: AccountConnectionOperationInput,
    ) -> Result<BrowserAccountConnectionStatus, String>;
    async fn browser_complete(
        &self,
        input: BrowserAccountConnectionCompleteInput,
    ) -> Result<BrowserAccountConnectionStatus, String>;
    async fn browser_cancel(
        &self,
        input: DirectAccountConnectionCancelInput,
    ) -> Result<BrowserAccountConnectionStatus, String>;

    async fn set_x_chat_passcode(
        &self,
        input: SetXChatPasscodeInput,
    ) -> Result<OperatorAccountProjection, String>;
    async fn clear_x_chat_passcode(
        &self,
        input: ClearXChatPasscodeInput,
    ) -> Result<OperatorAccountProjection, String>;

    async fn telegram_start(
        &self,
        input: TelegramAccountConnectionStartInput,
    ) -> Result<TelegramAccountConnectionStatus, String>;
    async fn telegram_status(
        &self,
        input: AccountConnectionOperationInput,
    ) -> Result<TelegramAccountConnectionStatus, String>;
    async fn telegram_submit_otp(
        &self,
        input: TelegramAccountConnectionOtpInput,
    ) -> Result<TelegramAccountConnectionStatus, String>;
    async fn telegram_submit_two_factor(
        &self,
        input: TelegramAccountConnectionTwoFactorInput,
    ) -> Result<TelegramAccountConnectionStatus, String>;
    async fn telegram_cancel(
        &self,
        input: DirectAccountConnectionCancelInput,
    ) -> Result<TelegramAccountConnectionStatus, String>;

    async fn connection_prepare(
        &self,
        _input: OperatorAccountConnectionPrepareInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        Err("operator reconnect is unavailable".into())
    }
    async fn connection_begin(
        &self,
        _input: OperatorAccountConnectionBeginInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        Err("operator reconnect is unavailable".into())
    }
    async fn connection_status(
        &self,
        _input: OperatorAccountConnectionStatusInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        Err("operator reconnect is unavailable".into())
    }
    async fn connection_finalize(
        &self,
        _input: OperatorAccountConnectionFinalizeInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        Err("operator reconnect is unavailable".into())
    }
    async fn connection_cancel(
        &self,
        _input: OperatorAccountConnectionCancelInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        Err("operator reconnect is unavailable".into())
    }

    async fn connect_youtube(
        &self,
        input: YoutubeApiKeyConnectInput,
    ) -> Result<ServiceCredentialConnectResult, String>;
    async fn rotate_youtube(
        &self,
        input: onecrew_local_runtime::RotateYoutubeCredentialInput,
    ) -> Result<ServiceCredentialConnectResult, String>;
    async fn rename_youtube(
        &self,
        input: RenameYoutubeCredentialInput,
    ) -> Result<ServiceCredentialProjection, String>;

    async fn disconnect(
        &self,
        input: OperatorAccountDisconnectInput,
    ) -> Result<OperatorAccountDisconnectOutcome, OperatorAccountDisconnectError>;

    async fn delete_youtube_credential(
        &self,
        input: YoutubeCredentialDeleteInput,
    ) -> Result<YoutubeCredentialDeleteOutcome, YoutubeCredentialDeleteError>;
}

/// Thin bridge over the canonical platform runtime. It owns no secondary
/// client, vault, account state, proxy probe, or provider validation logic.
pub struct PlatformRuntimeOperatorAccountWorkflow {
    platform: Arc<LocalPlatform>,
}

impl PlatformRuntimeOperatorAccountWorkflow {
    pub fn new(platform: Arc<LocalPlatform>) -> Self {
        Self { platform }
    }
}

#[async_trait]
impl OperatorAccountWorkflow for PlatformRuntimeOperatorAccountWorkflow {
    async fn list(&self) -> Result<Vec<OperatorAccountProjection>, String> {
        let platform = Arc::clone(&self.platform);
        run_blocking(move || platform.operator_account_list()).await
    }

    async fn browser_start(
        &self,
        input: BrowserAccountConnectionStartInput,
    ) -> Result<BrowserAccountConnectionStatus, String> {
        self.platform.browser_account_connection_start(input).await
    }

    async fn browser_status(
        &self,
        input: AccountConnectionOperationInput,
    ) -> Result<BrowserAccountConnectionStatus, String> {
        self.platform.browser_account_connection_status(input).await
    }

    async fn browser_complete(
        &self,
        input: BrowserAccountConnectionCompleteInput,
    ) -> Result<BrowserAccountConnectionStatus, String> {
        self.platform
            .browser_account_connection_complete(input)
            .await
    }

    async fn browser_cancel(
        &self,
        input: DirectAccountConnectionCancelInput,
    ) -> Result<BrowserAccountConnectionStatus, String> {
        self.platform.browser_account_connection_cancel(input).await
    }

    async fn set_x_chat_passcode(
        &self,
        input: SetXChatPasscodeInput,
    ) -> Result<OperatorAccountProjection, String> {
        let platform = Arc::clone(&self.platform);
        run_blocking(move || platform.operator_account_x_chat_passcode_set(input)).await
    }

    async fn clear_x_chat_passcode(
        &self,
        input: ClearXChatPasscodeInput,
    ) -> Result<OperatorAccountProjection, String> {
        let platform = Arc::clone(&self.platform);
        run_blocking(move || platform.operator_account_x_chat_passcode_clear(input)).await
    }

    async fn telegram_start(
        &self,
        input: TelegramAccountConnectionStartInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        self.platform.telegram_account_connection_start(input).await
    }

    async fn telegram_status(
        &self,
        input: AccountConnectionOperationInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        self.platform
            .telegram_account_connection_status(input)
            .await
    }

    async fn telegram_submit_otp(
        &self,
        input: TelegramAccountConnectionOtpInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        self.platform
            .telegram_account_connection_submit_otp(input)
            .await
    }

    async fn telegram_submit_two_factor(
        &self,
        input: TelegramAccountConnectionTwoFactorInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        self.platform
            .telegram_account_connection_submit_two_factor(input)
            .await
    }

    async fn telegram_cancel(
        &self,
        input: DirectAccountConnectionCancelInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        self.platform
            .telegram_account_connection_cancel(input)
            .await
    }

    async fn connection_prepare(
        &self,
        input: OperatorAccountConnectionPrepareInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.platform
            .operator_account_connection_prepare(input)
            .await
    }

    async fn connection_begin(
        &self,
        input: OperatorAccountConnectionBeginInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.platform.operator_account_connection_begin(input).await
    }

    async fn connection_status(
        &self,
        input: OperatorAccountConnectionStatusInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.platform
            .operator_account_connection_status(input)
            .await
    }

    async fn connection_finalize(
        &self,
        input: OperatorAccountConnectionFinalizeInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.platform
            .operator_account_connection_finalize(input)
            .await
    }

    async fn connection_cancel(
        &self,
        input: OperatorAccountConnectionCancelInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.platform
            .operator_account_connection_cancel(input)
            .await
    }

    async fn connect_youtube(
        &self,
        input: YoutubeApiKeyConnectInput,
    ) -> Result<ServiceCredentialConnectResult, String> {
        self.platform.connect_youtube_api_key(input).await
    }

    async fn rotate_youtube(
        &self,
        input: onecrew_local_runtime::RotateYoutubeCredentialInput,
    ) -> Result<ServiceCredentialConnectResult, String> {
        self.platform.rotate_youtube_credential(input).await
    }

    async fn rename_youtube(
        &self,
        input: RenameYoutubeCredentialInput,
    ) -> Result<ServiceCredentialProjection, String> {
        let platform = Arc::clone(&self.platform);
        run_blocking(move || platform.rename_youtube_credential(input)).await
    }

    async fn disconnect(
        &self,
        input: OperatorAccountDisconnectInput,
    ) -> Result<OperatorAccountDisconnectOutcome, OperatorAccountDisconnectError> {
        let platform = Arc::clone(&self.platform);
        run_blocking(move || platform.operator_account_disconnect(input)).await
    }

    async fn delete_youtube_credential(
        &self,
        input: YoutubeCredentialDeleteInput,
    ) -> Result<YoutubeCredentialDeleteOutcome, YoutubeCredentialDeleteError> {
        let platform = Arc::clone(&self.platform);
        run_blocking(move || platform.delete_youtube_credential(input)).await
    }
}

/// A lost blocking-pool result during disconnect maps to the owner's own
/// internal variant; the lifecycle handler classifies it like any other
/// disconnect effect failure.
impl From<BlockingTaskFailure> for OperatorAccountDisconnectError {
    fn from(_failure: BlockingTaskFailure) -> Self {
        Self::Internal
    }
}
