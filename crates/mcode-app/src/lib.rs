use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug)]
pub struct LifecycleService {
    context: LaunchContext,
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
        }
    }

    pub fn launch_context(&self) -> LaunchContext {
        self.context.clone()
    }
}
