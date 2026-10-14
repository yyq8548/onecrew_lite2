//! Process-local progress, scoped to the admitted request and authenticated user.
use schemars::JsonSchema;
use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DraftPhase {
    CheckingConnection,
    CheckingCodex,
    Planning,
    ValidatingPlan,
    SavingPlan,
    Finished,
    Failed,
}
#[derive(Clone, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftTiming {
    pub phase: DraftPhase,
    pub elapsed_ms: u64,
}
#[derive(Clone, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftProgress {
    pub id: String,
    pub phase: DraftPhase,
    pub elapsed_ms: u64,
    pub phase_elapsed_ms: u64,
    pub timings: Vec<DraftTiming>,
    pub error_code: Option<String>,
    /// Validated excerpts of the customer's request, never raw model output.
    pub unsupported_conditions: Vec<String>,
}
pub(crate) struct DraftProgressState {
    account: String,
    started: Instant,
    entered: Instant,
    pub(crate) value: DraftProgress,
}
impl DraftProgressState {
    pub(crate) fn new(id: String, account: String) -> Self {
        Self {
            account,
            started: Instant::now(),
            entered: Instant::now(),
            value: DraftProgress {
                id,
                phase: DraftPhase::CheckingConnection,
                elapsed_ms: 0,
                phase_elapsed_ms: 0,
                timings: Vec::new(),
                error_code: None,
                unsupported_conditions: Vec::new(),
            },
        }
    }
    pub(crate) fn phase(&mut self, phase: DraftPhase) {
        if self.value.phase == phase {
            return;
        }
        self.value.timings.push(DraftTiming {
            phase: self.value.phase,
            elapsed_ms: self.entered.elapsed().as_millis() as u64,
        });
        self.value.phase = phase;
        self.value.elapsed_ms = self.started.elapsed().as_millis() as u64;
        self.value.phase_elapsed_ms = 0;
        self.entered = Instant::now();
    }
    fn snapshot(&self, account: &str, id: &str) -> Option<DraftProgress> {
        if self.account != account || self.value.id != id {
            return None;
        }
        let mut value = self.value.clone();
        if !matches!(value.phase, DraftPhase::Finished | DraftPhase::Failed) {
            value.elapsed_ms = self.started.elapsed().as_millis() as u64;
            value.phase_elapsed_ms = self.entered.elapsed().as_millis() as u64;
        }
        Some(value)
    }
}
impl crate::LocalPlatform {
    pub async fn discovery_draft_progress(
        &self,
        id: String,
    ) -> Result<Option<DraftProgress>, String> {
        let account = self.library_access_user().await?;
        Ok(self
            .discovery_draft_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|state| state.snapshot(&account, &id)))
    }
    pub(crate) fn draft_phase(&self, phase: DraftPhase) {
        if let Some(state) = self
            .discovery_draft_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            state.phase(phase);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_is_request_and_account_bound_and_retains_completed_phases() {
        let mut state = DraftProgressState::new("request".into(), "account".into());
        state.phase(DraftPhase::CheckingCodex);
        state.phase(DraftPhase::Planning);
        assert!(state.snapshot("other", "request").is_none());
        assert!(state.snapshot("account", "other").is_none());
        assert_eq!(
            state.snapshot("account", "request").unwrap().timings.len(),
            2
        );
        state.phase(DraftPhase::Failed);
        state.value.unsupported_conditions = vec!["男明星".into()];
        assert_eq!(
            state
                .snapshot("account", "request")
                .unwrap()
                .unsupported_conditions,
            vec!["男明星"]
        );
        assert!(state.snapshot("other", "request").is_none());
        assert!(
            DraftProgressState::new("next".into(), "account".into())
                .value
                .unsupported_conditions
                .is_empty()
        );
        let ended = state.snapshot("account", "request").unwrap().elapsed_ms;
        assert_eq!(
            state.snapshot("account", "request").unwrap().elapsed_ms,
            ended
        );
    }
}
