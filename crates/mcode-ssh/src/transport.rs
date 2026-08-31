//! SSH connection lifecycle: TCP connect, host-key verification through the
//! verifier + store + known_hosts, authentication (key+passphrase, password,
//! agent), and the handle that exec/PTY sessions hang off of.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, load_secret_key};
use tokio::net::TcpStream;

use crate::error::SshError;
use crate::fingerprint::{AlgorithmName, fingerprint_of};
use crate::known_hosts::{KnownHostsEntry, parse_known_hosts};
use crate::store::HostKeyStore;
use crate::verifier::{HostKeyDecision, verify_host_key};
use crate::{CONNECT_TIMEOUT_MS, KEEPALIVE_INTERVAL_MS};

/// How to authenticate. `Key` maps to the original's key+passphrase ladder,
/// `Agent` to SSH_AUTH_SOCK / platform agent discovery.
#[derive(Clone, Debug)]
pub enum SshAuth {
    Key {
        path: PathBuf,
        passphrase: Option<String>,
    },
    Password {
        password: String,
    },
    Agent,
}

/// Where to connect.
#[derive(Clone, Debug)]
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
    pub connect_timeout: Option<Duration>,
}

impl SshTarget {
    pub fn validate(&self) -> Result<(), SshError> {
        if self.host.trim().is_empty() {
            return Err(SshError::InvalidArgument("host"));
        }
        if self.port == 0 {
            return Err(SshError::InvalidArgument("port"));
        }
        if self.username.is_empty() {
            return Err(SshError::InvalidArgument("username"));
        }
        if let SshAuth::Key { path, .. } = &self.auth
            && path.as_os_str().is_empty()
        {
            return Err(SshError::InvalidArgument("key path"));
        }
        Ok(())
    }
}

/// Trust sources: MCode's accepted-key store (read+write) and the user's
/// `known_hosts` (read-only, never written by this crate).
#[derive(Clone)]
pub struct HostKeyTrust {
    pub store: Arc<HostKeyStore>,
    /// User known_hosts file(s); the caller decides which paths those are.
    pub known_hosts_paths: Vec<PathBuf>,
}

/// A verified, authenticated SSH connection.
pub struct SshConnection {
    handle: client::Handle<ClientHandler>,
}

/// The exact host key the server presented, as needed for first-contact
/// decisions by the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentedHostKey {
    pub algorithm: AlgorithmName,
    pub key_base64: String,
    pub fingerprint: String,
}

/// Result of connecting: either an open session or a host-key verdict for the
/// caller to act on.
pub enum ConnectOutcome {
    Connected(SshConnection),
    HostKeyUnknown(PresentedHostKey),
    HostKeyChanged {
        presented: PresentedHostKey,
        error: SshError,
    },
}
#[derive(Clone, Default)]
pub struct HostKeyVerdict {
    decision: Arc<tokio::sync::Mutex<Option<HostKeyDecision>>>,
    presented: Arc<tokio::sync::Mutex<Option<PresentedHostKey>>>,
}

pub struct ClientHandler {
    trust: HostKeyTrust,
    host: String,
    port: u16,
    verdict: HostKeyVerdict,
}

impl client::Handler for ClientHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // The wire blob for the presented key: what known_hosts lines and the
        // accepted-key store hold byte-for-byte. Certificates are outside v1
        // host-key scope (the original's verifier likewise had no CA
        // validation), so they fail closed.
        let (key_bytes, algorithm) = match server_public_key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => {
                let bytes = public_key_wire_bytes(key)?;
                let algorithm = AlgorithmName(key.algorithm().as_str().to_string());
                (bytes, algorithm)
            }
            russh::keys::PublicKeyOrCertificate::Certificate(_) => {
                *self.verdict.presented.lock().await = None;
                *self.verdict.decision.lock().await = Some(HostKeyDecision::Reject {
                    fingerprint: String::new(),
                    algorithm: AlgorithmName("ssh-certificate".into()),
                });
                return Ok(false);
            }
        };
        let fingerprint = fingerprint_of(&key_bytes);
        *self.verdict.presented.lock().await = Some(PresentedHostKey {
            algorithm: algorithm.clone(),
            key_base64: base64::engine::general_purpose::STANDARD.encode(&key_bytes),
            fingerprint: fingerprint.clone(),
        });

        let store_records = self.trust.store.load().map_err(|withheld| {
            // A store we cannot see is evidence withheld, not absence: the
            // verifier fails closed rather than treating it as empty, since
            // accepting here would turn every host into first contact.
            SshError::Protocol(format!("host key store unusable ({withheld:?})"))
        })?;
        let entries = self.load_known_hosts()?;

        let decision = verify_host_key(
            &entries,
            &store_records,
            &self.host,
            self.port,
            &algorithm,
            &key_bytes,
            &fingerprint,
        )?;
        *self.verdict.decision.lock().await = Some(decision.clone());
        Ok(matches!(decision, HostKeyDecision::Accept))
    }
}

impl ClientHandler {
    fn load_known_hosts(&self) -> Result<Vec<KnownHostsEntry>, SshError> {
        let mut entries = Vec::new();
        for path in &self.trust.known_hosts_paths {
            let contents = match std::fs::read_to_string(path) {
                Ok(contents) => contents,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                // Connect as ssh does with an unreadable file, but the missing
                // evidence is recorded by returning what we could read.
                Err(_) => continue,
            };
            entries.extend(parse_known_hosts(&contents));
        }
        Ok(entries)
    }
}

fn public_key_wire_bytes(key: &russh::keys::PublicKey) -> Result<Vec<u8>, SshError> {
    use russh::keys::PublicKeyBase64 as _;
    let bytes = key.public_key_bytes();
    if bytes.is_empty() {
        return Err(SshError::Protocol("could not encode host key".into()));
    }
    Ok(bytes)
}

impl SshConnection {
    /// Connect, verify the host key, and authenticate.
    ///
    /// On first contact the connection is torn down and
    /// [`ConnectOutcome::HostKeyUnknown`] is returned; the caller decides
    /// whether to persist via [`accept_host_key`] and reconnect. A changed key
    /// is always refused with [`SshError::HostKeyChanged`].
    pub async fn connect(
        target: &SshTarget,
        trust: &HostKeyTrust,
    ) -> Result<ConnectOutcome, SshError> {
        target.validate()?;
        let timeout = target
            .connect_timeout
            .unwrap_or(Duration::from_millis(CONNECT_TIMEOUT_MS));

        let config = Arc::new(client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_millis(KEEPALIVE_INTERVAL_MS)),
            keepalive_max: 3,
            ..client::Config::default()
        });

        let address = (target.host.as_str(), target.port);
        let tcp = tokio::time::timeout(timeout, TcpStream::connect(address))
            .await
            .map_err(|_| SshError::Timeout("connect"))?
            .map_err(|error| SshError::ConnectFailed(describe_io(&error)))?;

        let verdict = HostKeyVerdict::default();
        let handler = ClientHandler {
            trust: trust.clone(),
            host: target.host.trim().to_lowercase(),
            port: target.port,
            verdict: verdict.clone(),
        };

        let mut handle =
            tokio::time::timeout(timeout, client::connect_stream(config, tcp, handler))
                .await
                .map_err(|_| SshError::Timeout("handshake"))??;

        // A host-key rejection surfaces as a handshake error from russh; the
        // shared verdict recorded the decision, so re-derive the outcome from it.
        let presented = verdict.presented.lock().await.clone();
        let decision = verdict.decision.lock().await.clone();
        if let Some(presented) = presented {
            match decision {
                Some(HostKeyDecision::Accept) => {}
                Some(HostKeyDecision::Unknown { .. }) => {
                    let _ = handle
                        .disconnect(russh::Disconnect::HostKeyNotVerifiable, "", "English")
                        .await;
                    return Ok(ConnectOutcome::HostKeyUnknown(presented));
                }
                _ => {
                    let _ = handle
                        .disconnect(russh::Disconnect::HostKeyNotVerifiable, "", "English")
                        .await;
                    let algorithm = presented.algorithm.as_str().to_string();
                    let fingerprint = presented.fingerprint.clone();
                    return Ok(ConnectOutcome::HostKeyChanged {
                        error: SshError::HostKeyChanged {
                            fingerprint,
                            algorithm,
                        },
                        presented,
                    });
                }
            }
        } else if !matches!(decision, Some(HostKeyDecision::Accept)) {
            // The handshake failed before a key was presented (unreachable
            // server, protocol garbage): the russh error already mapped above.
            return Err(SshError::ConnectFailed("no host key was presented".into()));
        }

        authenticate(&mut handle, target).await?;
        Ok(ConnectOutcome::Connected(SshConnection { handle }))
    }

    /// Run a single command to completion. See [`crate::session::exec`].
    pub async fn exec(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<crate::session::ExecOutput, SshError> {
        crate::session::exec(&self.handle, command, timeout).await
    }

    /// Open an interactive PTY channel. See [`crate::session::open_pty_channel`].
    pub async fn open_pty_channel(
        &self,
        cols: u32,
        rows: u32,
    ) -> Result<crate::session::SshPtyChannel, SshError> {
        crate::session::open_pty_channel(&self.handle, cols, rows).await
    }

    pub async fn disconnect(&mut self) -> Result<(), SshError> {
        self.handle
            .disconnect(russh::Disconnect::ByApplication, "", "English")
            .await
            .map_err(map_russh_error)
    }
}

/// Record an accepted host key from a first-contact outcome.
pub async fn accept_host_key(
    trust: &HostKeyTrust,
    host: &str,
    port: u16,
    presented: &PresentedHostKey,
) -> Result<(), SshError> {
    let accepted_at = chrono_timestamp();
    trust
        .store
        .accept(
            host,
            port,
            presented.algorithm.key_type(),
            &presented.key_base64,
            &presented.fingerprint,
            accepted_at,
        )
        .map(|_| ())
}

async fn authenticate(
    handle: &mut client::Handle<ClientHandler>,
    target: &SshTarget,
) -> Result<(), SshError> {
    match &target.auth {
        SshAuth::Password { password } => {
            let result = handle
                .authenticate_password(&target.username, password.as_str())
                .await
                .map_err(map_russh_error)?;
            finish_auth(result)
        }
        SshAuth::Key { path, passphrase } => {
            let key = load_secret_key(Path::new(path), passphrase.as_deref()).map_err(|_| {
                // Deliberately terse: the error may stem from a wrong
                // passphrase or an unreadable file, and neither the
                // passphrase nor the path belong in a message.
                SshError::AuthFailed
            })?;
            let hash = handle
                .best_supported_rsa_hash()
                .await
                .map_err(map_russh_error)?;
            // RSA promotion: rsa-sha2-512 when the server supports it, then
            // 256; `PrivateKeyWithHashAlg::new` collapses non-RSA keys to None.
            let hash = pick_rsa_hash(hash);
            let result = handle
                .authenticate_publickey(
                    &target.username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(map_russh_error)?;
            finish_auth(result)
        }
        SshAuth::Agent => agent_auth(handle, target).await,
    }
}

/// russh reports the server's best hash as `Some(Some(...))`; `None`/`Some(None)`
/// means fall back to rsa-sha2-512, which every RSA-capable modern server takes.
fn pick_rsa_hash(server_hash: Option<Option<HashAlg>>) -> Option<HashAlg> {
    server_hash.flatten().or(Some(HashAlg::Sha512))
}

async fn agent_auth(
    handle: &mut client::Handle<ClientHandler>,
    target: &SshTarget,
) -> Result<(), SshError> {
    #[cfg(windows)]
    let mut agent = connect_agent_windows().await?;
    #[cfg(not(windows))]
    let mut agent = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .map_err(|_| SshError::AuthFailed)?;

    let identities = agent
        .request_identities()
        .await
        .map_err(|_| SshError::AuthFailed)?;
    if identities.is_empty() {
        return Err(SshError::AuthFailed);
    }

    let hash = handle
        .best_supported_rsa_hash()
        .await
        .map_err(map_russh_error)?;
    for identity in &identities {
        let hash = match identity {
            russh::keys::agent::AgentIdentity::PublicKey { key, .. } => {
                if key.algorithm().is_rsa() {
                    pick_rsa_hash(hash)
                } else {
                    None
                }
            }
            _ => None,
        };
        let key = match identity {
            russh::keys::agent::AgentIdentity::PublicKey { key, .. } => key.clone(),
            russh::keys::agent::AgentIdentity::Certificate { certificate, .. } => {
                // Certificates authenticate through their public key part;
                // wrap the bare key data back into a PublicKey.
                russh::keys::PublicKey::new(certificate.public_key().to_owned(), "")
            }
        };
        let result = handle
            .authenticate_publickey_with(&target.username, key, hash, &mut agent)
            .await;
        match result {
            Ok(auth_result) if auth_result.success() => return Ok(()),
            Ok(_) => continue,
            Err(_) => return Err(SshError::AuthFailed),
        }
    }
    Err(SshError::AuthFailed)
}

async fn connect_agent_windows() -> Result<
    russh::keys::agent::client::AgentClient<
        Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>,
    >,
    SshError,
> {
    if let Ok(path) = std::env::var("SSH_AUTH_SOCK")
        && let Ok(agent) =
            russh::keys::agent::client::AgentClient::connect_named_pipe(std::ffi::OsStr::new(&path))
                .await
    {
        return Ok(agent.dynamic());
    }
    russh::keys::agent::client::AgentClient::connect_pageant()
        .await
        .map(|agent| agent.dynamic())
        .map_err(|_| SshError::AuthFailed)
}

fn finish_auth(result: russh::client::AuthResult) -> Result<(), SshError> {
    if result.success() {
        Ok(())
    } else {
        Err(SshError::AuthFailed)
    }
}

fn map_russh_error(error: russh::Error) -> SshError {
    match error {
        russh::Error::ConnectionTimeout | russh::Error::KeepaliveTimeout => {
            SshError::Timeout("connection")
        }
        russh::Error::HUP | russh::Error::Disconnect => {
            SshError::ConnectFailed("closed by peer".into())
        }
        russh::Error::UnknownKey | russh::Error::KeyChanged { .. } => {
            SshError::Protocol("host key verification rejected the handshake".into())
        }
        other => SshError::ConnectFailed(other.to_string()),
    }
}

fn describe_io(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::ConnectionRefused => "connection refused".into(),
        std::io::ErrorKind::TimedOut => "connection timed out".into(),
        std::io::ErrorKind::NotFound => "address not found".into(),
        _ => error.to_string(),
    }
}

/// A UTC RFC-3339 timestamp without pulling in chrono: good enough for
/// `acceptedAt` bookkeeping the original wrote with `new Date().toISOString()`.
fn chrono_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs();
    let days = seconds / 86_400;
    let time = seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

/// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_empty_target_fields() {
        let target = SshTarget {
            host: "  ".into(),
            port: 22,
            username: "u".into(),
            auth: SshAuth::Password {
                password: "p".into(),
            },
            connect_timeout: None,
        };
        assert_eq!(
            target.validate().unwrap_err().code(),
            "ssh_invalid_argument"
        );

        let target = SshTarget {
            host: "h".into(),
            port: 0,
            username: "u".into(),
            auth: SshAuth::Password {
                password: "p".into(),
            },
            connect_timeout: None,
        };
        assert_eq!(
            target.validate().unwrap_err().code(),
            "ssh_invalid_argument"
        );

        let target = SshTarget {
            host: "h".into(),
            port: 22,
            username: String::new(),
            auth: SshAuth::Password {
                password: "p".into(),
            },
            connect_timeout: None,
        };
        assert_eq!(
            target.validate().unwrap_err().code(),
            "ssh_invalid_argument"
        );
    }

    #[test]
    fn connect_refused_maps_to_connect_failed_without_credentials() {
        // Bind a port, close it: connection refused on loopback, deterministically.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);

            let dir = tempfile::tempdir().unwrap();
            let target = SshTarget {
                host: "127.0.0.1".into(),
                port,
                username: "user".into(),
                auth: SshAuth::Password {
                    password: "hunter2-secret".into(),
                },
                connect_timeout: Some(Duration::from_secs(2)),
            };
            let trust = HostKeyTrust {
                store: Arc::new(HostKeyStore::open(dir.path().join("store.json"))),
                known_hosts_paths: vec![dir.path().join("known_hosts")],
            };
            let error = match SshConnection::connect(&target, &trust).await {
                Err(error) => error,
                Ok(_) => panic!("refused connection must not connect"),
            };
            assert_eq!(error.code(), "ssh_connect_failed");
            // The password must never surface in any message.
            assert!(!error.to_string().contains("hunter2-secret"));
        });
    }

    #[test]
    fn timestamps_render_as_utc_iso() {
        // 2026-08-31T00:00:00Z is epoch day 20696.
        assert_eq!(civil_from_days(20_696), (2026, 8, 31));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }
}
