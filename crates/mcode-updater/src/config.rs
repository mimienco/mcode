use crate::ReleaseChannel;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use url::Url;

#[derive(Clone, Debug)]
pub struct ReleaseFeedConfiguration {
    public_key: String,
    endpoints: BTreeMap<ReleaseChannel, Url>,
}

impl ReleaseFeedConfiguration {
    pub fn new<I, S>(
        public_key: impl Into<String>,
        endpoints: I,
    ) -> Result<Self, ReleaseFeedConfigurationError>
    where
        I: IntoIterator<Item = (ReleaseChannel, S)>,
        S: AsRef<str>,
    {
        let public_key = public_key.into();
        if public_key.trim().is_empty() {
            return Err(ReleaseFeedConfigurationError::MissingPublicKey);
        }

        let mut parsed = BTreeMap::new();
        let mut unique_endpoints = BTreeSet::new();
        for (channel, endpoint) in endpoints {
            if parsed.contains_key(&channel) {
                return Err(ReleaseFeedConfigurationError::DuplicateChannel(channel));
            }
            let endpoint = Url::parse(endpoint.as_ref())
                .map_err(|_| ReleaseFeedConfigurationError::InvalidEndpoint(channel))?;
            if endpoint.scheme() != "https" {
                return Err(ReleaseFeedConfigurationError::InsecureEndpoint(channel));
            }
            if !unique_endpoints.insert(endpoint.to_string()) {
                return Err(ReleaseFeedConfigurationError::DuplicateEndpoint(channel));
            }
            parsed.insert(channel, endpoint);
        }

        for channel in ReleaseChannel::ALL {
            if !parsed.contains_key(&channel) {
                return Err(ReleaseFeedConfigurationError::MissingChannel(channel));
            }
        }

        Ok(Self {
            public_key,
            endpoints: parsed,
        })
    }

    pub fn public_key(&self) -> &str {
        &self.public_key
    }

    pub fn endpoint(&self, channel: ReleaseChannel) -> &Url {
        self.endpoints
            .get(&channel)
            .expect("validated release feed configuration must contain every channel")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseFeedConfigurationError {
    MissingPublicKey,
    MissingChannel(ReleaseChannel),
    DuplicateChannel(ReleaseChannel),
    DuplicateEndpoint(ReleaseChannel),
    InvalidEndpoint(ReleaseChannel),
    InsecureEndpoint(ReleaseChannel),
}

impl fmt::Display for ReleaseFeedConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPublicKey => {
                formatter.write_str("the updater public key is not configured")
            }
            Self::MissingChannel(channel) => {
                write!(
                    formatter,
                    "the {channel} updater endpoint is not configured"
                )
            }
            Self::DuplicateChannel(channel) => {
                write!(
                    formatter,
                    "the {channel} updater endpoint is configured more than once"
                )
            }
            Self::DuplicateEndpoint(channel) => {
                write!(
                    formatter,
                    "the {channel} updater endpoint duplicates another release channel"
                )
            }
            Self::InvalidEndpoint(channel) => {
                write!(
                    formatter,
                    "the {channel} updater endpoint is not a valid URL"
                )
            }
            Self::InsecureEndpoint(channel) => {
                write!(formatter, "the {channel} updater endpoint must use HTTPS")
            }
        }
    }
}

impl std::error::Error for ReleaseFeedConfigurationError {}
