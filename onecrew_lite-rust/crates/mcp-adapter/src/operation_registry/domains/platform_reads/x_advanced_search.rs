use super::read_failure::read_failure_projection;
use super::shared::{metadata, policy, scope_arguments, validate_search_policy};
use crate::operation_registry::{
    descriptor::{ConnectionRef, ConnectionRequirement, EffectClass, OperatorToolkitId},
    execution::ExecutionDeclaration,
    registration::{RegistryBuildError, RegistryBuilder},
};
use onecrew_local_runtime::{
    BoundSearchAuthorization, LocalPlatform, SearchPlanPolicy,
    toolkits::{
        PlatformNativeData, PlatformReadError,
        x::{self, AdvancedSearchCriteria, AdvancedSearchFilter, AdvancedSearchResultMode},
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "schema-only projection of the provider filter enum"
)]
enum FilterSchema {
    Any,
    Include,
    Exclude,
}
#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "schema-only projection of the provider result-mode enum"
)]
enum ResultModeSchema {
    Top,
    Latest,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AdvancedXPostSearchArguments {
    /// Execute-only; omit from plan scope arguments. Set user_requested_direct only when the current user explicitly asks to skip the plan or search directly; never choose it autonomously.
    #[serde(default)]
    search_plan_policy: SearchPlanPolicy,
    /// Execute-only; omit from plan scope arguments. Reference issued by the user-confirmed onecrew.search.plan.prepare card.
    #[serde(default)]
    confirmed_plan_ref: Option<String>,
    /// Words that must all occur; each value is one word, or a space-separated phrase compiled in double quotes.
    #[schemars(length(max = 20))]
    #[serde(default)]
    all_words: Vec<String>,
    /// One phrase compiled inside X Web Search double quotes.
    #[schemars(length(max = 256))]
    #[serde(default)]
    exact_phrase: Option<String>,
    /// Alternatives compiled as one parenthesized OR group; each value is one word, or a space-separated phrase compiled in double quotes.
    #[schemars(length(max = 20))]
    #[serde(default)]
    any_words: Vec<String>,
    /// Words excluded from results; each value is one word, or a space-separated phrase compiled in double quotes.
    #[schemars(length(max = 20))]
    #[serde(default)]
    exclude_words: Vec<String>,
    /// Hashtag names, with or without a leading `#`.
    #[schemars(length(max = 20))]
    #[serde(default)]
    hashtags: Vec<String>,
    /// X handles whose posts may match, with or without a leading `@`.
    #[schemars(length(max = 20))]
    #[serde(default)]
    from_accounts: Vec<String>,
    /// X handles whose replies may match, with or without a leading `@`.
    #[schemars(length(max = 20))]
    #[serde(default)]
    to_accounts: Vec<String>,
    /// X handles that matching posts mention, with or without a leading `@`.
    #[schemars(length(max = 20))]
    #[serde(default)]
    mentioning_accounts: Vec<String>,
    /// Lowercase X Web Search language code such as `en` or `zh-cn`.
    #[serde(default)]
    language: Option<String>,
    /// Gregorian date compiled through X's `since:YYYY-MM-DD` operator.
    #[serde(default)]
    since: Option<String>,
    /// Gregorian date compiled through X's `until:YYYY-MM-DD` operator.
    #[serde(default)]
    until: Option<String>,
    /// Minimum reply count compiled through `min_replies`.
    #[serde(default)]
    min_replies: Option<u32>,
    /// Minimum like count compiled to the X Web operator `min_faves`.
    #[serde(default)]
    min_likes: Option<u32>,
    /// Minimum repost count compiled to the X Web operator `min_retweets`.
    #[serde(default)]
    min_reposts: Option<u32>,
    /// Include replies, exclude replies, or leave reply filtering unset.
    #[serde(default)]
    #[schemars(with = "FilterSchema")]
    replies: AdvancedSearchFilter,
    /// Include links, exclude links, or leave link filtering unset.
    #[serde(default)]
    #[schemars(with = "FilterSchema")]
    links: AdvancedSearchFilter,
    /// `top` requests X relevance results; `latest` requests recent results.
    #[serde(default)]
    #[schemars(with = "ResultModeSchema")]
    result_mode: AdvancedSearchResultMode,
    /// Provider page size from 1 through 100; omitted means 20; a confirmed plan freezes it.
    #[schemars(range(min = x::ADVANCED_SEARCH_MIN_COUNT, max = x::ADVANCED_SEARCH_MAX_COUNT))]
    #[serde(default)]
    count: Option<u8>,
    /// Execute-only continuation from next_cursor; omit from plan scopes.
    #[serde(default)]
    cursor: Option<String>,
}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    let mut operation_metadata =
        metadata(onecrew_local_runtime::OperationId::X_POSTS_SEARCH_ADVANCED);
    operation_metadata.aliases = vec![
        "structured X post filters".into(),
        "advanced search criteria".into(),
        "find X creators through posts".into(),
        "discover authors by topic".into(),
    ];
    let connection = ConnectionRequirement::OperatorAccount {
        toolkit: OperatorToolkitId::X,
    };
    builder.register_search_operation::<AdvancedXPostSearchArguments, PlatformNativeData, PlatformReadError, _, _, _, _>(
        operation_metadata,
        policy(
            EffectClass::ReadOpenWorld,
            connection.clone(),
            ExecutionDeclaration::IndependentRead,
        ),
        move |context, input| {
            let platform = Arc::clone(&platform);
            async move {
                let Some(ConnectionRef::OperatorAccount { operator_account_id, .. }) = &context.connection else {
                    return Err(PlatformReadError::InputInvalid("operator connection required"));
                };
                let policy = input.search_plan_policy;
                let reference = input.confirmed_plan_ref.clone();
                let request = input.into_request(operator_account_id.clone());
                let authorization = BoundSearchAuthorization::bind(&request, policy,
                    reference.as_deref(), context.search_authorization).map_err(PlatformReadError::SearchPlanRefused)?;
                x::search_advanced(&platform, request, authorization).await
            }
        },
        read_failure_projection(&connection),
        |_, input: &AdvancedXPostSearchArguments| input.validate(),
        AdvancedXPostSearchArguments::compile_scope,
    )
}
impl AdvancedXPostSearchArguments {
    fn into_request(self, operator_account_id: String) -> x::AdvancedSearchInput {
        x::AdvancedSearchInput {
            operator_account_id,
            criteria: self.criteria(),
            result_mode: self.result_mode,
            count: self.count(),
            cursor: self.cursor,
        }
    }

    /// The requested page size, or X's default when none is named.
    fn count(&self) -> u16 {
        self.count
            .map_or(x::ADVANCED_SEARCH_DEFAULT_COUNT, u16::from)
    }

    fn criteria(&self) -> AdvancedSearchCriteria {
        AdvancedSearchCriteria {
            all_words: self.all_words.clone(),
            exact_phrase: self.exact_phrase.clone(),
            any_words: self.any_words.clone(),
            exclude_words: self.exclude_words.clone(),
            hashtags: self.hashtags.clone(),
            from_accounts: self.from_accounts.clone(),
            to_accounts: self.to_accounts.clone(),
            mentioning_accounts: self.mentioning_accounts.clone(),
            language: self.language.clone(),
            since: self.since.clone(),
            until: self.until.clone(),
            min_replies: self.min_replies,
            min_likes: self.min_likes,
            min_reposts: self.min_reposts,
            replies: self.replies,
            links: self.links,
        }
    }

    fn validate(&self) -> Result<(), ()> {
        validate_search_policy(self.search_plan_policy, self.confirmed_plan_ref.as_deref())?;
        if !x::accepts_advanced_search_count(self.count()) {
            return Err(());
        }
        self.criteria().validate().map_err(|_| ())
    }
    fn compile_scope(
        &self,
        purpose: crate::operation_registry::execution::SearchScopePurpose,
    ) -> Result<serde_json::Map<String, serde_json::Value>, ()> {
        self.validate()?;
        // An accepted page size always fits the wire field's type.
        let count = u8::try_from(self.count()).map_err(|_| ())?;
        scope_arguments(self, count, self.cursor.as_deref(), purpose)
    }
}
