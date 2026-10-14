use super::descriptor::ApprovalClass;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApprovalRequest {
    pub(crate) message: String,
}

impl ApprovalRequest {
    pub(crate) fn new(message: impl Into<String>) -> Result<Self, ApprovalProjectionFailure> {
        let message = message.into();
        (!message.trim().is_empty() && message.len() <= 512)
            .then_some(Self { message })
            .ok_or(ApprovalProjectionFailure)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ApprovalProjectionFailure;

/// Admits an attended-prompt operation only when the human confirmed this
/// exact call through a result-store submission. A direct execute never
/// carries that confirmation, so it fails closed before any owner runs.
pub(crate) fn preflight(
    class: ApprovalClass,
    request: Option<&ApprovalRequest>,
    confirmed: bool,
) -> Result<(), ApprovalFailure> {
    match class {
        ApprovalClass::None | ApprovalClass::OwnerGuarded => Ok(()),
        ApprovalClass::AttendedPrompt => {
            request.ok_or(ApprovalFailure::PolicyInvalid)?;
            confirmed
                .then_some(())
                .ok_or(ApprovalFailure::ChannelUnavailable)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApprovalFailure {
    PolicyInvalid,
    /// The public code stays `approval_channel_unavailable`: the call reached
    /// the owner without a confirmed result submission.
    ChannelUnavailable,
}
