//! Endpoint-file publication — Rust port of
//! `src/shared/agent-hook-listener.ts:4687-4760` (`writeEndpointFile`) +
//! `src/shared/agent-hook-endpoint-file.ts` (parser). This file is the
//! discovery channel CLIs source to learn the hook server's port/token after
//! an app restart.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Per-platform extension lets hook scripts source the file natively
/// (POSIX `. "$file"` / Windows `call "%file%"`).
pub fn endpoint_file_name() -> &'static str {
    if cfg!(windows) {
        "endpoint.cmd"
    } else {
        "endpoint.env"
    }
}

/// `agent-hook-listener.ts:4692-4695` — values are shell-sourced; the `+`
/// rejects empty strings so a sourced `KEY=` can't clear the env var.
pub fn is_shell_safe_endpoint_value(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'/' | b'-'))
}

/// `EndpointFileFields` (`agent-hook-listener.ts:4697-4701`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointFileFields {
    pub port: u16,
    pub token: String,
    pub env: String,
    pub version: String,
}

impl EndpointFileFields {
    pub fn as_lines(&self) -> Vec<(String, String)> {
        vec![
            ("MCODE_AGENT_HOOK_PORT".into(), self.port.to_string()),
            ("MCODE_AGENT_HOOK_TOKEN".into(), self.token.clone()),
            ("MCODE_AGENT_HOOK_ENV".into(), self.env.clone()),
            ("MCODE_AGENT_HOOK_VERSION".into(), self.version.clone()),
        ]
    }

    /// win32: CRLF `set KEY=value` lines; POSIX: LF `KEY=value` lines.
    pub fn render(&self) -> String {
        let is_windows = cfg!(windows);
        let mut out = String::new();
        for (key, value) in self.as_lines() {
            if is_windows {
                out.push_str("set ");
            }
            out.push_str(&key);
            out.push('=');
            out.push_str(&value);
            out.push_str(if is_windows { "\r\n" } else { "\n" });
        }
        out
    }
}

fn parse_endpoint_lines(content: &str) -> Result<EndpointFileFields, EndpointFileError> {
    let mut port = None;
    let mut token = None;
    let mut env = None;
    let mut version = None;
    for line in content.lines() {
        let line = line.strip_prefix("set ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "MCODE_AGENT_HOOK_PORT" => port = Some(value),
            "MCODE_AGENT_HOOK_TOKEN" => token = Some(value),
            "MCODE_AGENT_HOOK_ENV" => env = Some(value),
            "MCODE_AGENT_HOOK_VERSION" => version = Some(value),
            _ => {}
        }
    }
    Ok(EndpointFileFields {
        port: port
            .and_then(|value| value.parse().ok())
            .ok_or(EndpointFileError::MissingField("MCODE_AGENT_HOOK_PORT"))?,
        token: token
            .ok_or(EndpointFileError::MissingField("MCODE_AGENT_HOOK_TOKEN"))?
            .to_string(),
        env: env
            .ok_or(EndpointFileError::MissingField("MCODE_AGENT_HOOK_ENV"))?
            .to_string(),
        version: version
            .ok_or(EndpointFileError::MissingField("MCODE_AGENT_HOOK_VERSION"))?
            .to_string(),
    })
}

/// Reader-side parser (`agent-hook-endpoint-file.ts`) — accepts both `set `
/// prefixes; fails closed on missing fields.
pub fn parse_agent_hook_endpoint_file(
    content: &str,
) -> Result<EndpointFileFields, EndpointFileError> {
    let fields = parse_endpoint_lines(content)?;
    if fields.port == 0
        || fields
            .as_lines()
            .iter()
            .any(|(_, value)| !is_shell_safe_endpoint_value(value))
    {
        return Err(EndpointFileError::InvalidValue);
    }
    Ok(fields)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointFileError {
    MissingField(&'static str),
    InvalidValue,
    Io,
}

impl core::fmt::Display for EndpointFileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "endpoint file missing {field}"),
            Self::InvalidValue => write!(f, "endpoint file value is not shell-safe"),
            Self::Io => write!(f, "endpoint file io failure"),
        }
    }
}

impl std::error::Error for EndpointFileError {}

/// Atomic tmp+rename write (`agent-hook-listener.ts:4707-4760`).
pub fn write_endpoint_file(
    dir: &Path,
    fields: &EndpointFileFields,
) -> Result<PathBuf, EndpointFileError> {
    for (key, value) in fields.as_lines() {
        if !is_shell_safe_endpoint_value(&value) {
            eprintln!(
                "[agent-hooks] refusing to write endpoint file: {key} value is not shell-safe"
            );
            return Err(EndpointFileError::InvalidValue);
        }
    }
    std::fs::create_dir_all(dir).map_err(|_| EndpointFileError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| EndpointFileError::Io)?;
    }
    let path = dir.join(endpoint_file_name());
    let tmp = dir.join(format!(
        ".endpoint-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp).map_err(|_| EndpointFileError::Io)?;
    file.write_all(fields.render().as_bytes())
        .map_err(|_| EndpointFileError::Io)?;
    drop(file);
    std::fs::rename(&tmp, &path).map_err(|_| EndpointFileError::Io)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_platform_shape() {
        let fields = EndpointFileFields {
            port: 53111,
            token: "tok-1".into(),
            env: "development".into(),
            version: "1".into(),
        };
        let rendered = fields.render();
        if cfg!(windows) {
            assert_eq!(
                rendered,
                "set MCODE_AGENT_HOOK_PORT=53111\r\nset MCODE_AGENT_HOOK_TOKEN=tok-1\r\nset MCODE_AGENT_HOOK_ENV=development\r\nset MCODE_AGENT_HOOK_VERSION=1\r\n"
            );
        } else {
            assert!(rendered.starts_with("MCODE_AGENT_HOOK_PORT=53111\n"));
            assert!(!rendered.contains("set "));
        }
    }

    #[test]
    fn parser_accepts_both_shapes_and_fails_closed() {
        let parsed = parse_agent_hook_endpoint_file(
            "set MCODE_AGENT_HOOK_PORT=1234\r\nset MCODE_AGENT_HOOK_TOKEN=t\r\nset MCODE_AGENT_HOOK_ENV=production\r\nset MCODE_AGENT_HOOK_VERSION=1\r\n",
        )
        .unwrap();
        assert_eq!(parsed.port, 1234);
        let missing = parse_agent_hook_endpoint_file("MCODE_AGENT_HOOK_PORT=1\n");
        assert_eq!(
            missing,
            Err(EndpointFileError::MissingField("MCODE_AGENT_HOOK_TOKEN"))
        );
    }

    #[test]
    fn shell_safety_gate() {
        assert!(is_shell_safe_endpoint_value("127.0.0.1"));
        assert!(is_shell_safe_endpoint_value("tok_9-x.y:z/w"));
        assert!(!is_shell_safe_endpoint_value(""));
        assert!(!is_shell_safe_endpoint_value("a b"));
        assert!(!is_shell_safe_endpoint_value("a&b"));
    }

    #[test]
    fn write_is_atomic_and_re_readable() {
        let dir =
            std::env::temp_dir().join(format!("mcode-as-ep-{}-{}", std::process::id(), line!()));
        let fields = EndpointFileFields {
            port: 1,
            token: "t".into(),
            env: "development".into(),
            version: "1".into(),
        };
        let path = write_endpoint_file(&dir, &fields).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(parse_agent_hook_endpoint_file(&raw).unwrap(), fields);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
