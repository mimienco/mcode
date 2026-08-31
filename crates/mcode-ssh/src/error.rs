use std::fmt;

/// Every failure the SSH transport reports, with the stable code the
/// renderer matches on (`ssh_*`). `Display` never includes credentials,
/// key material, or connection strings beyond the endpoint a user typed.
#[derive(Debug)]
pub enum SshError {
    /// TCP or handshake failure before authentication could run.
    ConnectFailed(String),
    /// The server refused the offered credentials.
    AuthFailed,
    /// First contact: no record of this host key anywhere.
    HostKeyUnknown { fingerprint: String },
    /// The presented key contradicts a known key for this host.
    HostKeyChanged {
        fingerprint: String,
        algorithm: String,
    },
    /// An operation exceeded its deadline.
    Timeout(&'static str),
    /// The channel carrying the operation closed before completing.
    ChannelClosed,
    /// A malformed record, store, or server response.
    Protocol(String),
    /// A caller-supplied argument failed validation.
    InvalidArgument(&'static str),
}

impl SshError {
    pub fn code(&self) -> &'static str {
        match self {
            SshError::ConnectFailed(_) => "ssh_connect_failed",
            SshError::AuthFailed => "ssh_auth_failed",
            SshError::HostKeyUnknown { .. } => "ssh_host_key_unknown",
            SshError::HostKeyChanged { .. } => "ssh_host_key_changed",
            SshError::Timeout(_) => "ssh_timeout",
            SshError::ChannelClosed => "ssh_channel_closed",
            SshError::Protocol(_) => "ssh_protocol",
            SshError::InvalidArgument(_) => "ssh_invalid_argument",
        }
    }
}

impl fmt::Display for SshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SshError::ConnectFailed(detail) => write!(formatter, "SSH connection failed: {detail}"),
            SshError::AuthFailed => formatter
                .write_str("authentication failed: the server rejected the offered credentials"),

            SshError::HostKeyUnknown { fingerprint } => write!(
                formatter,
                "host key is not known for this host (first contact), fingerprint {fingerprint}"
            ),
            SshError::HostKeyChanged {
                fingerprint,
                algorithm,
            } => write!(
                formatter,
                "host key verification failed: the host presented a different {algorithm} key than the one on record, fingerprint {fingerprint}"
            ),
            SshError::Timeout(operation) => {
                write!(formatter, "SSH operation timed out: {operation}")
            }
            SshError::ChannelClosed => {
                formatter.write_str("SSH channel closed before the operation completed")
            }
            SshError::Protocol(detail) => {
                write!(formatter, "SSH protocol or store error: {detail}")
            }
            SshError::InvalidArgument(name) => write!(formatter, "invalid SSH argument: {name}"),
        }
    }
}

impl std::error::Error for SshError {}

impl From<russh::Error> for SshError {
    fn from(error: russh::Error) -> Self {
        // The transport module owns the fine-grained mapping; this fallback
        // keeps the Handler trait bound satisfied without leaking credentials.
        match error {
            russh::Error::ConnectionTimeout | russh::Error::KeepaliveTimeout => {
                SshError::Timeout("connection")
            }
            russh::Error::HUP | russh::Error::Disconnect => {
                SshError::ConnectFailed("closed by peer".into())
            }
            other => SshError::Protocol(other.to_string()),
        }
    }
}
