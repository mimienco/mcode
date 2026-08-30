use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    Stable,
    Rc,
    Hourly,
    Daily,
    Adhoc,
}

impl ReleaseChannel {
    pub const ALL: [Self; 5] = [
        Self::Stable,
        Self::Rc,
        Self::Hourly,
        Self::Daily,
        Self::Adhoc,
    ];
}

impl fmt::Display for ReleaseChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Stable => "stable",
            Self::Rc => "rc",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Adhoc => "adhoc",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateOperation {
    Check,
    Download,
    Install,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableUpdate {
    pub channel: ReleaseChannel,
    pub current_version: String,
    pub version: String,
    pub body: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum UpdateStatus {
    Idle,
    Checking {
        channel: ReleaseChannel,
        user_initiated: bool,
    },
    Available {
        channel: ReleaseChannel,
        current_version: String,
        version: String,
        body: Option<String>,
        published_at: Option<String>,
        user_initiated: bool,
    },
    NotAvailable {
        channel: ReleaseChannel,
        user_initiated: bool,
    },
    Downloading {
        channel: ReleaseChannel,
        version: String,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        percent: Option<u8>,
    },
    Downloaded {
        channel: ReleaseChannel,
        version: String,
    },
    Installing {
        channel: ReleaseChannel,
        version: String,
    },
    Error {
        operation: UpdateOperation,
        code: String,
        message: String,
        retryable: bool,
        channel: Option<ReleaseChannel>,
        version: Option<String>,
        user_initiated: bool,
    },
}

#[derive(Debug)]
struct UpdateServiceState {
    status: UpdateStatus,
    candidate: Option<AvailableUpdate>,
    verified_download: bool,
}

#[derive(Debug)]
pub struct UpdateService {
    state: Mutex<UpdateServiceState>,
}

impl Default for UpdateService {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateService {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(UpdateServiceState {
                status: UpdateStatus::Idle,
                candidate: None,
                verified_download: false,
            }),
        }
    }

    pub fn status(&self) -> Result<UpdateStatus, UpdateServiceError> {
        Ok(self.lock()?.status.clone())
    }

    pub fn begin_check(
        &self,
        channel: ReleaseChannel,
        user_initiated: bool,
    ) -> Result<(), UpdateServiceError> {
        let mut state = self.lock()?;
        if let Some(operation) = active_operation(&state.status) {
            return Err(UpdateServiceError::OperationInProgress(operation));
        }
        state.candidate = None;
        state.verified_download = false;
        state.status = UpdateStatus::Checking {
            channel,
            user_initiated,
        };
        Ok(())
    }

    pub fn complete_check(
        &self,
        candidate: Option<AvailableUpdate>,
    ) -> Result<(), UpdateServiceError> {
        let mut state = self.lock()?;
        let (channel, user_initiated) = match state.status {
            UpdateStatus::Checking {
                channel,
                user_initiated,
            } => (channel, user_initiated),
            _ => {
                return Err(UpdateServiceError::InvalidTransition(
                    UpdateOperation::Check,
                ));
            }
        };
        if let Some(candidate) = candidate {
            if candidate.channel != channel {
                return Err(UpdateServiceError::MismatchedChannel);
            }
            state.status = UpdateStatus::Available {
                channel,
                current_version: candidate.current_version.clone(),
                version: candidate.version.clone(),
                body: candidate.body.clone(),
                published_at: candidate.published_at.clone(),
                user_initiated,
            };
            state.candidate = Some(candidate);
        } else {
            state.status = UpdateStatus::NotAvailable {
                channel,
                user_initiated,
            };
            state.candidate = None;
        }
        state.verified_download = false;
        Ok(())
    }

    pub fn begin_download(&self) -> Result<(), UpdateServiceError> {
        let mut state = self.lock()?;
        if let Some(operation) = active_operation(&state.status) {
            return Err(UpdateServiceError::OperationInProgress(operation));
        }
        let candidate = state
            .candidate
            .as_ref()
            .ok_or(UpdateServiceError::NoAvailableUpdate)?;
        if state.verified_download {
            return Err(UpdateServiceError::InvalidTransition(
                UpdateOperation::Download,
            ));
        }
        let channel = candidate.channel;
        let version = candidate.version.clone();
        state.status = UpdateStatus::Downloading {
            channel,
            version,
            downloaded_bytes: 0,
            total_bytes: None,
            percent: None,
        };
        Ok(())
    }

    pub fn record_download_progress(
        &self,
        chunk_bytes: usize,
        reported_total: Option<u64>,
    ) -> Result<(), UpdateServiceError> {
        let mut state = self.lock()?;
        let UpdateStatus::Downloading {
            downloaded_bytes,
            total_bytes,
            percent,
            ..
        } = &mut state.status
        else {
            return Err(UpdateServiceError::InvalidTransition(
                UpdateOperation::Download,
            ));
        };
        *downloaded_bytes = downloaded_bytes.saturating_add(chunk_bytes as u64);
        if let Some(total) = reported_total {
            *total_bytes = Some(total);
        }
        *percent = total_bytes.and_then(|total| {
            (total > 0).then(|| ((*downloaded_bytes).saturating_mul(100) / total).min(100) as u8)
        });
        Ok(())
    }

    pub fn complete_download(&self) -> Result<(), UpdateServiceError> {
        let mut state = self.lock()?;
        if !matches!(state.status, UpdateStatus::Downloading { .. }) {
            return Err(UpdateServiceError::InvalidTransition(
                UpdateOperation::Download,
            ));
        }
        let candidate = state
            .candidate
            .as_ref()
            .ok_or(UpdateServiceError::NoAvailableUpdate)?;
        let channel = candidate.channel;
        let version = candidate.version.clone();
        state.verified_download = true;
        state.status = UpdateStatus::Downloaded { channel, version };
        Ok(())
    }

    pub fn begin_install(&self) -> Result<(), UpdateServiceError> {
        let mut state = self.lock()?;
        if let Some(operation) = active_operation(&state.status) {
            return Err(UpdateServiceError::OperationInProgress(operation));
        }
        if !state.verified_download {
            return Err(UpdateServiceError::NoDownloadedUpdate);
        }
        let candidate = state
            .candidate
            .as_ref()
            .ok_or(UpdateServiceError::NoDownloadedUpdate)?;
        let channel = candidate.channel;
        let version = candidate.version.clone();
        state.status = UpdateStatus::Installing { channel, version };
        Ok(())
    }

    pub fn fail(
        &self,
        operation: UpdateOperation,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
    ) -> Result<(), UpdateServiceError> {
        let mut state = self.lock()?;
        if active_operation(&state.status) != Some(operation) {
            return Err(UpdateServiceError::InvalidTransition(operation));
        }
        let (channel, version) = state
            .candidate
            .as_ref()
            .map(|candidate| (Some(candidate.channel), Some(candidate.version.clone())))
            .unwrap_or_else(|| match &state.status {
                UpdateStatus::Checking { channel, .. } => (Some(*channel), None),
                _ => (None, None),
            });
        let user_initiated = match &state.status {
            UpdateStatus::Checking { user_initiated, .. }
            | UpdateStatus::Available { user_initiated, .. }
            | UpdateStatus::NotAvailable { user_initiated, .. }
            | UpdateStatus::Error { user_initiated, .. } => *user_initiated,
            _ => true,
        };
        state.status = UpdateStatus::Error {
            operation,
            code: code.into(),
            message: message.into(),
            retryable,
            channel,
            version,
            user_initiated,
        };
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, UpdateServiceState>, UpdateServiceError> {
        self.state
            .lock()
            .map_err(|_| UpdateServiceError::StateUnavailable)
    }
}

fn active_operation(status: &UpdateStatus) -> Option<UpdateOperation> {
    match status {
        UpdateStatus::Checking { .. } => Some(UpdateOperation::Check),
        UpdateStatus::Downloading { .. } => Some(UpdateOperation::Download),
        UpdateStatus::Installing { .. } => Some(UpdateOperation::Install),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateServiceError {
    StateUnavailable,
    OperationInProgress(UpdateOperation),
    NoAvailableUpdate,
    NoDownloadedUpdate,
    MismatchedChannel,
    InvalidTransition(UpdateOperation),
}

impl UpdateServiceError {
    pub fn code(self) -> &'static str {
        match self {
            Self::StateUnavailable => "update_state_unavailable",
            Self::OperationInProgress(_) => "update_operation_in_progress",
            Self::NoAvailableUpdate => "no_available_update",
            Self::NoDownloadedUpdate => "no_downloaded_update",
            Self::MismatchedChannel => "update_channel_mismatch",
            Self::InvalidTransition(_) => "invalid_update_transition",
        }
    }
}

impl fmt::Display for UpdateServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateUnavailable => formatter.write_str("updater state is unavailable"),
            Self::OperationInProgress(operation) => {
                write!(
                    formatter,
                    "an update {operation:?} operation is already in progress"
                )
            }
            Self::NoAvailableUpdate => formatter.write_str("there is no available update"),
            Self::NoDownloadedUpdate => {
                formatter.write_str("there is no verified downloaded update")
            }
            Self::MismatchedChannel => {
                formatter.write_str("the update belongs to a different release channel")
            }
            Self::InvalidTransition(operation) => {
                write!(
                    formatter,
                    "the update {operation:?} transition is not valid now"
                )
            }
        }
    }
}

impl std::error::Error for UpdateServiceError {}
