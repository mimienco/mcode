use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Mutex;

pub const SHUTDOWN_REQUEST_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownIntent {
    Quit,
    Restart,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownRequest {
    pub version: u8,
    pub request_id: String,
    pub intent: ShutdownIntent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleServiceError {
    ShutdownStateUnavailable,
    ShutdownRequestIdExhausted,
    NoPendingShutdown,
    ShutdownNotConfirmed,
    ShutdownAlreadyConfirmed,
    ShutdownAlreadyExecuting,
    StaleShutdownRequest,
}

impl LifecycleServiceError {
    pub fn code(self) -> &'static str {
        match self {
            Self::ShutdownStateUnavailable => "shutdown_state_unavailable",
            Self::ShutdownRequestIdExhausted => "shutdown_request_id_exhausted",
            Self::NoPendingShutdown => "no_pending_shutdown",
            Self::ShutdownNotConfirmed => "shutdown_not_confirmed",
            Self::ShutdownAlreadyConfirmed => "shutdown_already_confirmed",
            Self::ShutdownAlreadyExecuting => "shutdown_already_executing",
            Self::StaleShutdownRequest => "stale_shutdown_request",
        }
    }
}

impl fmt::Display for LifecycleServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ShutdownStateUnavailable => "shutdown state is unavailable",
            Self::ShutdownRequestIdExhausted => "shutdown request identifiers are exhausted",
            Self::NoPendingShutdown => "there is no pending shutdown request",
            Self::ShutdownNotConfirmed => "shutdown request has not been confirmed",
            Self::ShutdownAlreadyConfirmed => "a shutdown request is already confirmed",
            Self::ShutdownAlreadyExecuting => "a shutdown request is already executing",
            Self::StaleShutdownRequest => "shutdown request does not match the pending request",
        })
    }
}

impl std::error::Error for LifecycleServiceError {}

#[derive(Debug)]
enum ShutdownPhase {
    Idle,
    Pending(ShutdownRequest),
    Acknowledged(ShutdownRequest),
    Confirmed(ShutdownRequest),
    Executing(ShutdownRequest),
}

#[derive(Debug)]
pub(crate) struct ShutdownCoordinator {
    state: Mutex<ShutdownState>,
}

#[derive(Debug)]
struct ShutdownState {
    next_request_id: u64,
    phase: ShutdownPhase,
}

impl ShutdownCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(ShutdownState {
                next_request_id: 1,
                phase: ShutdownPhase::Idle,
            }),
        }
    }

    pub(crate) fn request(
        &self,
        intent: ShutdownIntent,
    ) -> Result<ShutdownRequest, LifecycleServiceError> {
        let mut state = self.lock()?;
        match &state.phase {
            ShutdownPhase::Confirmed(_) => {
                return Err(LifecycleServiceError::ShutdownAlreadyConfirmed);
            }
            ShutdownPhase::Executing(_) => {
                return Err(LifecycleServiceError::ShutdownAlreadyExecuting);
            }
            ShutdownPhase::Idle | ShutdownPhase::Pending(_) | ShutdownPhase::Acknowledged(_) => {}
        }
        let request_id = state.next_request_id;
        state.next_request_id = request_id
            .checked_add(1)
            .ok_or(LifecycleServiceError::ShutdownRequestIdExhausted)?;
        let request = ShutdownRequest {
            version: SHUTDOWN_REQUEST_VERSION,
            request_id: request_id.to_string(),
            intent,
        };
        state.phase = ShutdownPhase::Pending(request.clone());
        Ok(request)
    }

    pub(crate) fn pending(&self) -> Result<Option<ShutdownRequest>, LifecycleServiceError> {
        let state = self.lock()?;
        Ok(match &state.phase {
            ShutdownPhase::Pending(request) | ShutdownPhase::Acknowledged(request) => {
                Some(request.clone())
            }
            ShutdownPhase::Idle | ShutdownPhase::Confirmed(_) | ShutdownPhase::Executing(_) => None,
        })
    }

    pub(crate) fn acknowledge(
        &self,
        request_id: &str,
    ) -> Result<ShutdownRequest, LifecycleServiceError> {
        let mut state = self.lock()?;
        let request = match &state.phase {
            ShutdownPhase::Pending(request) if request.request_id == request_id => request.clone(),
            ShutdownPhase::Acknowledged(request)
            | ShutdownPhase::Confirmed(request)
            | ShutdownPhase::Executing(request)
                if request.request_id == request_id =>
            {
                return Ok(request.clone());
            }
            ShutdownPhase::Pending(_)
            | ShutdownPhase::Acknowledged(_)
            | ShutdownPhase::Confirmed(_)
            | ShutdownPhase::Executing(_) => {
                return Err(LifecycleServiceError::StaleShutdownRequest);
            }
            ShutdownPhase::Idle => return Err(LifecycleServiceError::NoPendingShutdown),
        };
        state.phase = ShutdownPhase::Acknowledged(request.clone());
        Ok(request)
    }

    pub(crate) fn confirm(
        &self,
        request_id: &str,
    ) -> Result<ShutdownRequest, LifecycleServiceError> {
        let mut state = self.lock()?;
        let request = match &state.phase {
            ShutdownPhase::Executing(request) if request.request_id == request_id => {
                return Ok(request.clone());
            }
            ShutdownPhase::Pending(request)
            | ShutdownPhase::Acknowledged(request)
            | ShutdownPhase::Confirmed(request)
                if request.request_id == request_id =>
            {
                request.clone()
            }
            ShutdownPhase::Pending(_)
            | ShutdownPhase::Acknowledged(_)
            | ShutdownPhase::Confirmed(_)
            | ShutdownPhase::Executing(_) => {
                return Err(LifecycleServiceError::StaleShutdownRequest);
            }
            ShutdownPhase::Idle => return Err(LifecycleServiceError::NoPendingShutdown),
        };
        state.phase = ShutdownPhase::Confirmed(request.clone());
        Ok(request)
    }

    pub(crate) fn cancel(&self, request_id: &str) -> Result<(), LifecycleServiceError> {
        let mut state = self.lock()?;
        match &state.phase {
            ShutdownPhase::Pending(request)
            | ShutdownPhase::Acknowledged(request)
            | ShutdownPhase::Confirmed(request)
                if request.request_id == request_id =>
            {
                state.phase = ShutdownPhase::Idle;
                Ok(())
            }
            ShutdownPhase::Executing(request) if request.request_id == request_id => {
                Err(LifecycleServiceError::ShutdownAlreadyExecuting)
            }
            ShutdownPhase::Pending(_)
            | ShutdownPhase::Acknowledged(_)
            | ShutdownPhase::Confirmed(_)
            | ShutdownPhase::Executing(_) => Err(LifecycleServiceError::StaleShutdownRequest),
            ShutdownPhase::Idle => Err(LifecycleServiceError::NoPendingShutdown),
        }
    }

    pub(crate) fn claim_execution(
        &self,
        request_id: &str,
    ) -> Result<Option<ShutdownRequest>, LifecycleServiceError> {
        let mut state = self.lock()?;
        let request = match &state.phase {
            ShutdownPhase::Confirmed(request) if request.request_id == request_id => {
                request.clone()
            }
            ShutdownPhase::Executing(request) if request.request_id == request_id => {
                return Ok(None);
            }
            ShutdownPhase::Pending(request) | ShutdownPhase::Acknowledged(request)
                if request.request_id == request_id =>
            {
                return Err(LifecycleServiceError::ShutdownNotConfirmed);
            }
            ShutdownPhase::Pending(_)
            | ShutdownPhase::Acknowledged(_)
            | ShutdownPhase::Confirmed(_)
            | ShutdownPhase::Executing(_) => {
                return Err(LifecycleServiceError::StaleShutdownRequest);
            }
            ShutdownPhase::Idle => return Err(LifecycleServiceError::NoPendingShutdown),
        };
        state.phase = ShutdownPhase::Executing(request.clone());
        Ok(Some(request))
    }

    pub(crate) fn force_claim_execution(
        &self,
        request_id: &str,
    ) -> Result<Option<ShutdownRequest>, LifecycleServiceError> {
        let mut state = self.lock()?;
        let request = match &state.phase {
            ShutdownPhase::Pending(request) if request.request_id == request_id => request.clone(),
            ShutdownPhase::Acknowledged(request)
            | ShutdownPhase::Confirmed(request)
            | ShutdownPhase::Executing(request)
                if request.request_id == request_id =>
            {
                return Ok(None);
            }
            ShutdownPhase::Pending(_)
            | ShutdownPhase::Acknowledged(_)
            | ShutdownPhase::Confirmed(_)
            | ShutdownPhase::Executing(_) => {
                return Err(LifecycleServiceError::StaleShutdownRequest);
            }
            ShutdownPhase::Idle => return Err(LifecycleServiceError::NoPendingShutdown),
        };
        state.phase = ShutdownPhase::Executing(request.clone());
        Ok(Some(request))
    }

    pub(crate) fn is_executing(
        &self,
        intent: ShutdownIntent,
    ) -> Result<bool, LifecycleServiceError> {
        let state = self.lock()?;
        Ok(matches!(
            &state.phase,
            ShutdownPhase::Executing(request) if request.intent == intent
        ))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ShutdownState>, LifecycleServiceError> {
        self.state
            .lock()
            .map_err(|_| LifecycleServiceError::ShutdownStateUnavailable)
    }
}
