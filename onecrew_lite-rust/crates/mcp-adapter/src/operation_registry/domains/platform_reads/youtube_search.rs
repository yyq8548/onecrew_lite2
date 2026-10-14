use super::read_failure::read_failure_projection;
use super::shared::{metadata, policy, scope_arguments, validate_search_policy};
use crate::operation_registry::{
    descriptor::{ConnectionRef, ConnectionRequirement, EffectClass, ServiceToolkitId},
    execution::ExecutionDeclaration,
    registration::{RegistryBuildError, RegistryBuilder},
};
use onecrew_local_runtime::{
    BoundSearchAuthorization, LocalPlatform, SearchPlanPolicy,
    toolkits::{PlatformNativeData, PlatformReadError, youtube},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

fn default_youtube_search_count() -> Option<u8> {
    Some(youtube::SEARCH_DEFAULT_COUNT)
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "schema-only projection of the provider order enum"
)]
enum SearchOrderSchema {
    Relevance,
    Date,
}

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct YoutubeSearchArguments {
    /// Execute-only. Direct search requires an explicit request from the user.
    #[serde(default)]
    search_plan_policy: SearchPlanPolicy,
    /// Execute-only reference issued by the confirmed search plan.
    #[serde(default)]
    confirmed_plan_ref: Option<String>,
    query: String,
    /// YouTube page size. Omitted means 20; a confirmed plan freezes it.
    #[serde(default)]
    #[schemars(range(min = youtube::SEARCH_MIN_COUNT, max = youtube::SEARCH_MAX_COUNT))]
    #[schemars(default = "default_youtube_search_count")]
    count: Option<u8>,
    /// How YouTube ranks the page. `date` returns the newest uploads first, so each hit's matched.published_at shows how recently that channel published.
    #[serde(default)]
    #[schemars(with = "SearchOrderSchema")]
    order: youtube::YoutubeSearchOrder,
    /// Keep only resources published at or after this RFC 3339 UTC instant, YYYY-MM-DDTHH:MM:SSZ.
    #[serde(default)]
    published_after: Option<String>,
    /// Execute-only native next_page_token from the preceding YouTube page; omit from plan scopes.
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    region_code: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

impl YoutubeSearchArguments {
    fn validate(&self) -> Result<(), ()> {
        validate_search_policy(self.search_plan_policy, self.confirmed_plan_ref.as_deref())?;
        if !youtube::accepts_search_query(&self.query)
            || !youtube::accepts_search_count(self.count.unwrap_or(youtube::SEARCH_DEFAULT_COUNT))
        {
            return Err(());
        }
        if self
            .published_after
            .as_deref()
            .is_some_and(|value| !youtube::accepts_published_after(value))
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
        scope_arguments(
            self,
            self.count.unwrap_or(youtube::SEARCH_DEFAULT_COUNT),
            self.cursor.as_deref(),
            purpose,
        )
    }
}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    let connection = ConnectionRequirement::ServiceCredential {
        toolkit: ServiceToolkitId::Youtube,
    };
    macro_rules! search {
        ($id:expr, $input:ident, $method:ident) => {{
            let platform = Arc::clone(&platform);
            builder.register_search_operation::<YoutubeSearchArguments, PlatformNativeData, PlatformReadError, _, _, _, _>(
                metadata($id),
                policy(
                    EffectClass::ReadOpenWorld,
                    connection.clone(),
                    ExecutionDeclaration::IndependentRead,
                ),
                move |context, input| {
                    let platform = Arc::clone(&platform);
                    async move {
                        let Some(ConnectionRef::ServiceCredential { credential_id, .. }) = &context.connection else {
                            return Err(PlatformReadError::InputInvalid("service credential required"));
                        };
                        let request = youtube::$input {
                            credential_id: credential_id.clone(), query: input.query, count: input.count.unwrap_or(youtube::SEARCH_DEFAULT_COUNT),
                            order: input.order, published_after: input.published_after,
                            cursor: input.cursor, region_code: input.region_code, language: input.language,
                        };
                        let authorization = BoundSearchAuthorization::bind(&request, input.search_plan_policy,
                            input.confirmed_plan_ref.as_deref(), context.search_authorization)
                            .map_err(PlatformReadError::SearchPlanRefused)?;
                        youtube::$method(&platform, request, authorization).await
                    }
                },
                read_failure_projection(&connection),
                |_, input: &YoutubeSearchArguments| input.validate(),
                YoutubeSearchArguments::compile_scope,
            )?;
        }};
    }
    search!(
        onecrew_local_runtime::OperationId::YOUTUBE_CHANNELS_SEARCH,
        ChannelsSearchInput,
        channels_search
    );
    search!(
        onecrew_local_runtime::OperationId::YOUTUBE_VIDEOS_SEARCH,
        VideosSearchInput,
        videos_search
    );
    Ok(())
}
