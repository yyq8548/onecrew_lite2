//! V2 presentation projection facade.
mod batch;
mod call;
mod choice;
mod continuation_copy;
mod exact;
mod localization;
mod prefix;
mod terminal;

pub(super) use batch::{
    batch_is_presentable, combine_executed_batch, executed_batch, execution_continuation,
};
#[cfg(test)]
pub(super) use call::AdapterPresentationKind;
pub(super) use call::ProjectionCall;
#[cfg(test)]
pub(super) use choice::choice_continuation;
pub(super) use choice::{
    ASK_USER_QUESTION_OPERATION_ID, choice_batch, choice_projection, selected_choice_update,
};
pub(super) use continuation_copy::continuation_copy;
pub(super) use localization::localize_adapter_presentation;
pub(super) use prefix::readonly_prefix_projection;
pub(super) use terminal::{
    cancelled_continuation, confirm_action_label, confirmation_projection,
    presentation_failure_projection, presentation_failure_terminal_update,
    resumed_terminal_projection, stranded_terminal_update, terminal_projection,
    terminal_projection_with_budget,
};
