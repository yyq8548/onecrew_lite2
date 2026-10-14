use super::read_failure::read_failure_projection;
use super::shared::{metadata, policy, scope_arguments, validate_search_policy};
use super::telegram_gets::TELEGRAM_CONNECTION;
use crate::operation_registry::{
    descriptor::{ConnectionRef, EffectClass},
    execution::ExecutionDeclaration,
    registration::{RegistryBuildError, RegistryBuilder},
};
use onecrew_local_runtime::{
    BoundSearchAuthorization, LocalPlatform, SearchPlanPolicy,
    toolkits::{PlatformNativeData, PlatformReadError, telegram},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchArguments {
    /// Execute-only. Direct search requires an explicit request from the user.
    #[serde(default)]
    search_plan_policy: SearchPlanPolicy,
    /// Execute-only reference issued by the confirmed search plan.
    #[serde(default)]
    confirmed_plan_ref: Option<String>,
    query: String,
    #[serde(default)]
    #[schemars(range(
        min = telegram::COMMUNITY_SEARCH_MIN_COUNT,
        max = telegram::COMMUNITY_SEARCH_MAX_COUNT
    ))]
    count: Option<u8>,
}

impl SearchArguments {
    /// The requested page size, or Telegram's default when none is named.
    fn count(&self) -> u16 {
        self.count
            .map_or(telegram::COMMUNITY_SEARCH_DEFAULT_COUNT, u16::from)
    }

    fn validate(&self) -> Result<(), ()> {
        validate_search_policy(self.search_plan_policy, self.confirmed_plan_ref.as_deref())?;
        if !telegram::accepts_community_search_query(&self.query)
            || !telegram::accepts_community_search_count(self.count())
        {
            return Err(());
        }
        Ok(())
    }
    fn compile_scope(
        &self,
        purpose: crate::operation_registry::execution::SearchScopePurpose,
    ) -> Result<serde_json::Map<String, serde_json::Value>, ()> {
        self.validate()?;
        // An accepted page size always fits the wire field's type.
        let count = u8::try_from(self.count()).map_err(|_| ())?;
        scope_arguments(self, count, None, purpose)
    }
}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    builder.register_search_operation::<SearchArguments, Vec<PlatformNativeData>, PlatformReadError, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITIES_SEARCH),
        policy(
            EffectClass::ReadOpenWorld,
            TELEGRAM_CONNECTION,
            ExecutionDeclaration::IndependentRead,
        ),
        move |context, input| {
            let platform = Arc::clone(&platform);
            async move {
                let Some(ConnectionRef::OperatorAccount { operator_account_id, .. }) = &context.connection else {
                    return Err(PlatformReadError::InputInvalid("operator connection required"));
                };
                let count = input.count();
                let request = telegram::CommunitiesSearchInput {
                    operator_account_id: operator_account_id.clone(), query: input.query, count,
                };
                let authorization = BoundSearchAuthorization::bind(&request, input.search_plan_policy,
                    input.confirmed_plan_ref.as_deref(), context.search_authorization)
                    .map_err(PlatformReadError::SearchPlanRefused)?;
                telegram::search_communities(&platform, request, authorization).await
            }
        },
        read_failure_projection(&TELEGRAM_CONNECTION),
        |_, input: &SearchArguments| input.validate(),
        SearchArguments::compile_scope,
    )
}
