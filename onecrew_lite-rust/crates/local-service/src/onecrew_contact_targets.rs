use onecrew_creator_runtime::{ContactTargetResolutionResult, ResolveContactTargetInput};

use crate::ServiceState;

pub(super) async fn onecrew_contact_target_resolve(
    state: &ServiceState,
    input: ResolveContactTargetInput,
) -> Result<ContactTargetResolutionResult, String> {
    state
        .platform()
        .await?
        .resolve_contact_target(input)
        .await
        .map_err(|error| error.to_string())
}
