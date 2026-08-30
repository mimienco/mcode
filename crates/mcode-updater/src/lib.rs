mod config;
mod service;

pub use config::{ReleaseFeedConfiguration, ReleaseFeedConfigurationError};
pub use service::{
    AvailableUpdate, ReleaseChannel, UpdateOperation, UpdateService, UpdateServiceError,
    UpdateStatus,
};
