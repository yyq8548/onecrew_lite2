use onecrew_creator_runtime::UpdateRelationshipInput;
use onecrew_local_runtime::{ResultOrigin, ResultSnapshot};

use crate::ServiceState;

pub(super) async fn onecrew_relationship_update_prepare(
    state: &ServiceState,
    input: UpdateRelationshipInput,
    origin: &ResultOrigin,
) -> Result<ResultSnapshot, String> {
    let platform = state.platform().await?;
    let review = platform
        .prepare_outreach_relationship(
            input,
            crate::outreach_review::context(origin, "onecrew.outreach.relationship.update")?,
        )
        .await
        .map_err(|error| error.to_string())?;
    crate::outreach_review::present(&platform, review)
}
