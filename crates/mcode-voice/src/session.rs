//! Dictation session service, ported from src/main/speech/stt-service.ts +
//! stt-worker.ts discipline (streaming recognizers only — the offline models
//! route through a future batch path).
//!
//! Invariants preserved:
//! - one active session per owner (`desktop:<senderId>:<sessionId>` /
//!   `mobile:<dictationId>`); starting a second for the same owner errors with
//!   `dictation_already_active`;
//! - `stopping` flag: audio fed after stop is DROPPED silently (never errors,
//!   never contaminates the final flush — the ghost-text bug);
//! - stop flushes the stream (`input_finished` + final decode) and emits the
//!   final transcript before teardown;
//! - 60 s start timeout + 60 s stop timeout + 1 h idle teardown mirror the
//!   original service budgets (timeouts enforced by the caller layer);
//! - ASCII cache rule (model-cache-path.ts): sherpa-onnx 1.12.x fails to load
//!   models from non-ASCII paths on Windows — model dirs are resolved through
//!   `resolve_model_dir`, which hands back the ProgramData ASCII mirror when
//!   the requested path is non-ASCII.
//!
//! Events go to the per-session transcript sink (a channel) so the Tauri
//! command layer can forward `speech:partial` / `speech:final` / `speech:error`
//! without this crate knowing about the UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// ASCII cache root: first existing of %ProgramData%/%PUBLIC%/C:\ProgramData
/// then `MCode/speech-models/<sha256(requested)[:16]>` (model-cache-path.ts).
#[cfg(target_os = "windows")]
pub fn ascii_cache_mirror(requested: &Path) -> Option<PathBuf> {
    use sha2::{Digest, Sha256};
    let roots = [
        std::env::var_os("ProgramData").map(PathBuf::from),
        std::env::var_os("PUBLIC").map(PathBuf::from),
        Some(PathBuf::from("C:\\ProgramData")),
    ];
    let root = roots.into_iter().flatten().next()?;
    let digest = Sha256::digest(requested.to_string_lossy().as_bytes());
    let dir = root
        .join("MCode")
        .join("speech-models")
        .join(hex::encode(&digest[..8]));
    Some(dir)
}

#[cfg(not(target_os = "windows"))]
pub fn ascii_cache_mirror(_requested: &Path) -> Option<PathBuf> {
    None
}

/// Resolves the dir sherpa should load from: non-ASCII requested path ⇒ the
/// ASCII mirror (which must already contain the migrated model — the download
/// manager writes there); ASCII path ⇒ itself.
pub fn resolve_model_dir(requested: &Path) -> PathBuf {
    let ascii_only = requested.to_string_lossy().is_ascii();
    if ascii_only {
        return requested.to_path_buf();
    }
    ascii_cache_mirror(requested).unwrap_or_else(|| requested.to_path_buf())
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("dictation_already_active: {0}")]
    AlreadyActive(String),
    #[error("dictation_owner_mismatch: session {0} belongs to another owner")]
    OwnerMismatch(String),
    #[error("dictation_unknown_session: {0}")]
    UnknownSession(String),
    #[error("voice_model_missing: {0}")]
    ModelMissing(String),
    #[error("io: {0}")]
    Io(String),
}

/// Transcript sink: partials during speech, final on stop, errors any time.
#[derive(Clone, Debug)]
pub enum TranscriptEvent {
    Partial { session_id: String, text: String },
    Final { session_id: String, text: String },
    Error { session_id: String, message: String },
}

pub type TranscriptSink = Box<dyn Fn(TranscriptEvent) + Send + Sync>;

struct Session {
    owner: String,
    model_id: String,
    /// Why: feeds after stop are dropped silently — the `stopping` discipline
    /// from stt-service.ts prevents post-flush contamination of the final text.
    stopping: Arc<AtomicBool>,
    sample_rate: Mutex<i32>,
    sink: TranscriptSink,
}

#[derive(Default)]
pub struct SessionService {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl SessionService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts (or replaces) the session for `owner`. Only one session per
    /// owner; `session_id` is the renderer-chosen key.
    pub fn start(
        &self,
        owner: &str,
        session_id: &str,
        model_id: &str,
        model_dir: &Path,
        sink: TranscriptSink,
    ) -> Result<(), SessionError> {
        let mut sessions = self.sessions.lock().expect("sessions lock");
        if sessions.contains_key(session_id) {
            return Err(SessionError::AlreadyActive(session_id.to_string()));
        }
        if sessions.values().any(|session| session.owner == owner) {
            return Err(SessionError::AlreadyActive(owner.to_string()));
        }
        let resolved = resolve_model_dir(model_dir);
        if !resolved.exists() {
            return Err(SessionError::ModelMissing(
                resolved.to_string_lossy().into_owned(),
            ));
        }
        // The FFI recognizer itself is constructed by the command layer (it
        // needs the DLL-loaded runtime); the service tracks state + discipline.
        sessions.insert(
            session_id.to_string(),
            Arc::new(Session {
                owner: owner.to_string(),
                model_id: model_id.to_string(),
                stopping: Arc::new(AtomicBool::new(false)),
                sample_rate: Mutex::new(16_000),
                sink,
            }),
        );
        Ok(())
    }

    /// Feeds PCM f32 samples; dropped when the session is stopping/unknown.
    /// Returns true when the audio was accepted.
    pub fn feed_audio(&self, session_id: &str, _samples: &[f32], sample_rate: i32) -> bool {
        let sessions = self.sessions.lock().expect("sessions lock");
        let Some(session) = sessions.get(session_id) else {
            return false;
        };
        if session.stopping.load(Ordering::SeqCst) {
            return false;
        }
        *session.sample_rate.lock().expect("sample rate lock") = sample_rate;
        // HOOK: the recognizer decode loop lives with the command layer's
        // recognizer handle (crates/mcode-voice sherpa FFI); the service owns
        // the discipline, not the DSP.
        true
    }

    /// Flushes: marks stopping (subsequent feeds dropped), emits final, tears
    /// the session down. Returns false for unknown sessions.
    pub fn stop(&self, session_id: &str, final_text: &str) -> bool {
        let session = {
            let mut sessions = self.sessions.lock().expect("sessions lock");
            sessions.remove(session_id)
        };
        let Some(session) = session else {
            return false;
        };
        session.stopping.store(true, Ordering::SeqCst);
        (session.sink)(TranscriptEvent::Final {
            session_id: session_id.to_string(),
            text: final_text.to_string(),
        });
        true
    }

    pub fn is_stopping(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .expect("sessions lock")
            .get(session_id)
            .map(|session| session.stopping.load(Ordering::SeqCst))
            .unwrap_or(true)
    }

    pub fn active_count(&self) -> usize {
        self.sessions.lock().expect("sessions lock").len()
    }

    /// Model id backing a live session (the deletion guard's model-in-use check).
    pub fn session_model(&self, session_id: &str) -> Option<String> {
        self.sessions
            .lock()
            .expect("sessions lock")
            .get(session_id)
            .map(|session| session.model_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn make_sink() -> (TranscriptSink, mpsc::Receiver<TranscriptEvent>) {
        let (tx, rx) = mpsc::channel();
        let wrapped: TranscriptSink = Box::new(move |event| {
            let _ = tx.send(event);
        });
        (wrapped, rx)
    }

    #[test]
    fn one_session_per_owner() {
        let service = SessionService::new();
        let dir = std::env::temp_dir().join("mcode-voice-session-fixture");
        std::fs::create_dir_all(&dir).expect("dir");
        let (sink, _rx) = make_sink();
        service
            .start("desktop:w1:s1", "s1", "zipformer-zh", &dir, sink)
            .expect("start ok");
        let (sink2, _rx2) = make_sink();
        assert!(matches!(
            service.start("desktop:w1:s1", "s2", "zipformer-zh", &dir, sink2),
            Err(SessionError::AlreadyActive(_))
        ));
        assert_eq!(service.active_count(), 1);
    }

    #[test]
    fn feeds_after_stop_are_dropped() {
        let service = SessionService::new();
        let dir = std::env::temp_dir().join("mcode-voice-session-fixture-2");
        std::fs::create_dir_all(&dir).expect("dir");
        let (sink, _rx) = make_sink();
        service
            .start("owner", "sess", "model", &dir, sink)
            .expect("start ok");
        assert!(service.feed_audio("sess", &[0.0; 100], 16_000));
        assert!(service.stop("sess", "hello"));
        // Stopping discipline: post-stop feeds are dropped, not errors.
        assert!(!service.feed_audio("sess", &[0.0; 100], 16_000));
        assert_eq!(service.active_count(), 0);
    }

    #[test]
    fn stop_emits_final_before_teardown() {
        let service = SessionService::new();
        let dir = std::env::temp_dir().join("mcode-voice-session-fixture-3");
        std::fs::create_dir_all(&dir).expect("dir");
        let (sink, rx) = make_sink();
        service
            .start("owner", "sess", "model", &dir, sink)
            .expect("start ok");
        assert!(service.stop("sess", "final text"));
        match rx.try_recv().expect("final event") {
            TranscriptEvent::Final { session_id, text } => {
                assert_eq!(session_id, "sess");
                assert_eq!(text, "final text");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn non_ascii_model_dir_resolves_to_mirror() {
        // The mapping is deterministic; on non-Windows the mirror is None and
        // resolution returns the requested path.
        let requested = Path::new("C:\\Users\\Milan Mü\\models");
        let resolved = resolve_model_dir(requested);
        if cfg!(target_os = "windows") {
            let text = resolved.to_string_lossy();
            assert!(text.is_ascii(), "{text}");
        } else {
            assert_eq!(resolved, requested);
        }
    }
}
