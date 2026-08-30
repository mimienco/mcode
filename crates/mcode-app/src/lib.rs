use serde::{Deserialize, Serialize};

mod shutdown;

pub use shutdown::{LifecycleServiceError, ShutdownIntent, ShutdownRequest};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildLane {
    Development,
    Production,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchContext {
    pub version: String,
    pub build_lane: BuildLane,
    pub headless: bool,
    pub safe_mode: bool,
}

#[derive(Debug)]
pub struct LifecycleService {
    context: LaunchContext,
    shutdown: shutdown::ShutdownCoordinator,
}

impl LifecycleService {
    pub fn new(version: impl Into<String>, build_lane: BuildLane) -> Self {
        Self {
            context: LaunchContext {
                version: version.into(),
                build_lane,
                headless: false,
                safe_mode: false,
            },
            shutdown: shutdown::ShutdownCoordinator::new(),
        }
    }

    pub fn launch_context(&self) -> LaunchContext {
        self.context.clone()
    }

    pub fn request_shutdown(
        &self,
        intent: ShutdownIntent,
    ) -> Result<ShutdownRequest, LifecycleServiceError> {
        self.shutdown.request(intent)
    }

    pub fn pending_shutdown(&self) -> Result<Option<ShutdownRequest>, LifecycleServiceError> {
        self.shutdown.pending()
    }

    pub fn confirm_shutdown(
        &self,
        request_id: &str,
    ) -> Result<ShutdownRequest, LifecycleServiceError> {
        self.shutdown.confirm(request_id)
    }

    pub fn acknowledge_shutdown(
        &self,
        request_id: &str,
    ) -> Result<ShutdownRequest, LifecycleServiceError> {
        self.shutdown.acknowledge(request_id)
    }

    pub fn cancel_shutdown(&self, request_id: &str) -> Result<(), LifecycleServiceError> {
        self.shutdown.cancel(request_id)
    }

    pub fn claim_shutdown_execution(
        &self,
        request_id: &str,
    ) -> Result<Option<ShutdownRequest>, LifecycleServiceError> {
        self.shutdown.claim_execution(request_id)
    }

    pub fn abort_update_install_execution(
        &self,
        request_id: &str,
    ) -> Result<(), LifecycleServiceError> {
        self.shutdown.abort_update_install_execution(request_id)
    }

    pub fn abort_unacknowledged_update_install(
        &self,
        request_id: &str,
    ) -> Result<bool, LifecycleServiceError> {
        self.shutdown
            .abort_unacknowledged_update_install(request_id)
    }

    pub fn force_claim_shutdown_execution(
        &self,
        request_id: &str,
    ) -> Result<Option<ShutdownRequest>, LifecycleServiceError> {
        self.shutdown.force_claim_execution(request_id)
    }

    pub fn is_shutdown_executing(
        &self,
        intent: ShutdownIntent,
    ) -> Result<bool, LifecycleServiceError> {
        self.shutdown.is_executing(intent)
    }
}
