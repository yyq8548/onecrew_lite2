mod instagram;
mod telegram;
mod x;

pub use instagram::{
    InstagramPlatformDetail, InstagramPlatformGetInput, InstagramPlatformGetOutcome,
};
pub use telegram::{TelegramPlatformDetail, TelegramPlatformGetInput, TelegramPlatformGetOutcome};
pub use x::{XPlatformDetail, XPlatformGetInput, XPlatformGetOutcome};

use crate::{LocalPlatform, OperatorAccountPlatform, OperatorAccountProjection};

pub(super) fn resolve_exact_operator_account(
    platform: &LocalPlatform,
    operator_account_id: &str,
    expected_platform: OperatorAccountPlatform,
) -> Result<Option<OperatorAccountProjection>, String> {
    Ok(platform
        .operator_account_list()?
        .into_iter()
        .find(|account| {
            account.operator_account_id == operator_account_id
                && account.platform == expected_platform
        }))
}
