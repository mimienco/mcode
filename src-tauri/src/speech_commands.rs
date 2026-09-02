//! Speech bridge: wires the `mcode-voice` services (catalog, download
//! manager, dictation sessions) into Tauri commands for the renderer.
//!
//! Mirrors the Electron IPC surface (`src/main/ipc/speech.ts`):
//! `speech:getCatalog` → `ui_speechGetCatalog`, `speech:getModelStates` →
//! `ui_speechGetModelStates`, `speech:downloadModel` →
//! `ui_speechDownloadModel`, `speech:cancelDownload` →
//! `ui_speechCancelDownload`, `speech:startDictation` →
//! `ui_speechStartDictation`, `speech:feedAudio` → `ui_speechFeedAudio`,
//! `speech:stopDictation` → `ui_speechStopDictation`.
//!
//! Events forwarded to the renderer (`window.webContents.send` upstream):
//! `speech://downloadProgress`, `speech://modelReady`, `speech://partial`,
//! `speech://final`, `speech://error`.
//!
//! DLL-free vs DLL-required behavior: catalog, model states, downloads,
//! dictation start/stop discipline work without the sherpa DLL. Partial and
//! final transcripts require `sherpa-onnx-c-api.dll` (found via
//! `mcode_voice::sherpa_lib_dir()` / `MCODE_SHERPA_LIB_DIR`); when it cannot
//! be loaded the recognizer thread emits `speech://error` and the session is
//! marked dead (feeds are rejected, stop still flushes an empty final).

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mcode_voice::catalog::{CATALOG, ModelCatalogEntry, ModelFileSpec};
use mcode_voice::download::{DownloadError, DownloadManager, ModelDownloadState, download_file};
use mcode_voice::session::{SessionService, TranscriptEvent};
use serde::Serialize;
use tauri::{Emitter, Manager};

/// Renderer event channels (Electron: `speech:downloadProgress` etc.).
pub(crate) const DOWNLOAD_PROGRESS_EVENT: &str = "speech://downloadProgress";
pub(crate) const MODEL_READY_EVENT: &str = "speech://modelReady";
pub(crate) const PARTIAL_EVENT: &str = "speech://partial";
pub(crate) const FINAL_EVENT: &str = "speech://final";
pub(crate) const ERROR_EVENT: &str = "speech://error";

/// Ring buffer cap: 30 s of 16 kHz mono audio. Audio past the cap is dropped
/// from the front (the recognizer can never catch up past 30 s of lag).
const RING_MAX_SAMPLES: usize = 30 * 16_000;
/// How long `stop` waits for the recognizer thread to flush its final text
/// before falling back to an empty final (mirrors the bounded stop budget).
const STOP_FLUSH_TIMEOUT: Duration = Duration::from_secs(3);
const STOP_FLUSH_POLL: Duration = Duration::from_millis(30);

// ---------------------------------------------------------------------------
// DTOs — keep mcode-voice UI-free; serialize here with camelCase keys.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SpeechModelFileDto {
    pub(crate) name: String,
    pub(crate) size_bytes: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SpeechModelDto {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) streaming: bool,
    pub(crate) repo: String,
    pub(crate) revision: String,
    pub(crate) files: Vec<SpeechModelFileDto>,
}

fn model_file_dto(file: &ModelFileSpec) -> SpeechModelFileDto {
    SpeechModelFileDto {
        name: file.name.to_owned(),
        size_bytes: file.size_bytes,
        sha256: file.sha256.to_owned(),
    }
}

fn model_dto(entry: &ModelCatalogEntry<'_>) -> SpeechModelDto {
    SpeechModelDto {
        id: entry.id.to_owned(),
        label: entry.label.to_owned(),
        streaming: entry.streaming,
        repo: entry.repo.to_owned(),
        revision: entry.revision.to_owned(),
        files: entry.files.iter().map(model_file_dto).collect(),
    }
}

// ---------------------------------------------------------------------------
// Voice state
// ---------------------------------------------------------------------------

/// Shared app state for the speech bridge (managed via `app.manage`).
pub(crate) struct VoiceState {
    sessions: Arc<SessionService>,
    downloads: Arc<DownloadManager>,
    models_root: PathBuf,
    /// Per-session recognizer plumbing (created lazily on first feed).
    dictations: Mutex<HashMap<String, DictationHandle>>,
}

impl VoiceState {
    pub(crate) fn new(models_root: PathBuf) -> Self {
        Self {
            sessions: Arc::new(SessionService::new()),
            downloads: Arc::new(DownloadManager::new()),
            models_root,
            dictations: Mutex::new(HashMap::new()),
        }
    }

    /// Catalog entry for a model id, or `None` for unknown ids (also rejects
    /// path-traversal shapes — ids only ever come from the static catalog).
    fn catalog_entry(&self, model_id: &str) -> Option<&'static ModelCatalogEntry<'static>> {
        CATALOG.iter().find(|entry| entry.id == model_id)
    }

    /// Directory of a model under the models root. Unknown ids are rejected.
    fn model_dir(&self, model_id: &str) -> Result<PathBuf, String> {
        self.catalog_entry(model_id)
            .map(|_| self.models_root.join(model_id))
            .ok_or_else(|| format!("unknown model: {model_id}"))
    }

    /// Download state string for one model: live manager state wins; an idle
    /// model is `ready` when its resolved dir contains `tokens.txt`, else
    /// `idle`.
    fn state_string(&self, model_id: &str) -> Result<String, String> {
        let manager_state = self.downloads.state(model_id);
        if manager_state != ModelDownloadState::Idle {
            return Ok(download_state_string(manager_state));
        }
        let dir = self.model_dir(model_id)?;
        let resolved = mcode_voice::session::resolve_model_dir(&dir);
        let ready = resolved.join("tokens.txt").is_file();
        Ok(if ready { "ready" } else { "idle" }.to_owned())
    }
}

/// Builds the managed voice state. Models root: `MCODE_MODELS_DIR` override,
/// else `<app data dir>/speech-models` (mirroring Electron's
/// `userData/speech-models` default).
pub(crate) fn voice_state(app: &tauri::App) -> VoiceState {
    let models_root = std::env::var_os("MCODE_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            app.path()
                .app_data_dir()
                .unwrap_or_else(|error| {
                    eprintln!(
                        "[speech] failed to resolve app data dir, using temp models root: {error}"
                    );
                    std::env::temp_dir().join("mcode-speech-models")
                })
                .join("speech-models")
        });
    VoiceState::new(models_root)
}

fn download_state_string(state: ModelDownloadState) -> String {
    match state {
        ModelDownloadState::Idle => "idle",
        ModelDownloadState::Downloading => "downloading",
        ModelDownloadState::Paused => "paused",
        ModelDownloadState::Ready => "ready",
        ModelDownloadState::Failed => "failed",
    }
    .to_owned()
}

// ---------------------------------------------------------------------------
// Dictation plumbing: ring buffer + recognizer handle
// ---------------------------------------------------------------------------

/// PCM ring shared between the feed command and the recognizer thread.
struct AudioRing {
    samples: Mutex<RingInner>,
}

struct RingInner {
    queue: VecDeque<f32>,
    sample_rate: i32,
}

impl AudioRing {
    fn new() -> Self {
        Self {
            samples: Mutex::new(RingInner {
                queue: VecDeque::new(),
                sample_rate: 16_000,
            }),
        }
    }

    fn push(&self, samples: &[f32], sample_rate: i32) {
        let mut inner = self.samples.lock().expect("audio ring lock");
        inner.sample_rate = sample_rate;
        inner.queue.extend(samples.iter().copied());
        let overflow = inner.queue.len().saturating_sub(RING_MAX_SAMPLES);
        if overflow > 0 {
            inner.queue.drain(..overflow);
        }
    }

    fn drain(&self) -> (Vec<f32>, i32) {
        let mut inner = self.samples.lock().expect("audio ring lock");
        let sample_rate = inner.sample_rate;
        let drained: Vec<f32> = inner.queue.drain(..).collect();
        (drained, sample_rate)
    }
}

/// Handle to a session's recognizer thread.
struct DictationHandle {
    ring: Arc<AudioRing>,
    /// Set on stop and on fatal recognizer errors; feeds are rejected after.
    dead: Arc<AtomicBool>,
    /// The recognizer thread finished (final text is available).
    finished: Arc<AtomicBool>,
    final_text: Arc<Mutex<String>>,
}

/// Transcript sink bound to a Tauri app handle: forwards session transcript
/// events to the renderer.
fn speech_sink(app: &tauri::AppHandle) -> mcode_voice::session::TranscriptSink {
    let app = app.clone();
    Box::new(move |event| match event {
        TranscriptEvent::Partial { session_id, text } => {
            emit_json(
                &app,
                PARTIAL_EVENT,
                &serde_json::json!({ "sessionId": session_id, "text": text }),
            );
        }
        TranscriptEvent::Final { session_id, text } => {
            emit_json(
                &app,
                FINAL_EVENT,
                &serde_json::json!({ "sessionId": session_id, "text": text }),
            );
        }
        TranscriptEvent::Error {
            session_id,
            message,
        } => {
            emit_json(
                &app,
                ERROR_EVENT,
                &serde_json::json!({ "sessionId": session_id, "error": message }),
            );
        }
    })
}

fn emit_json(app: &tauri::AppHandle, channel: &str, payload: &serde_json::Value) {
    if let Err(error) = app.emit(channel, payload) {
        eprintln!("[speech] failed to emit {channel}: {error}");
    }
}

// ---------------------------------------------------------------------------
// Recognizer construction (DLL-dependent, runs on the recognizer thread)
// ---------------------------------------------------------------------------

/// Builds the online recognizer config from a model directory, mirroring the
/// mcode-voice acceptance test: transducer packages (encoder/decoder/joiner)
/// preferred, paraformer packages as fallback, `tokens.txt` required.
fn recognizer_config(model_dir: &Path) -> Result<mcode_voice::OnlineRecognizerConfig, String> {
    let find = |needle: &str| -> Option<String> {
        std::fs::read_dir(model_dir)
            .ok()?
            .filter_map(|entry| entry.ok())
            .find(|entry| {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                name.contains(needle) && name.ends_with(".onnx")
            })
            .map(|entry| entry.path().to_string_lossy().into_owned())
    };

    let transducer = (|| {
        Some(mcode_voice::TransducerPaths {
            encoder: find("encoder")?,
            decoder: find("decoder")?,
            joiner: find("joiner")?,
        })
    })();
    let paraformer = if transducer.is_none() {
        (|| {
            Some(mcode_voice::ParaformerPaths {
                encoder: find("encoder")?,
                decoder: find("decoder")?,
            })
        })()
    } else {
        None
    };
    if transducer.is_none() && paraformer.is_none() {
        return Err(format!(
            "no encoder/decoder .onnx pair found in {}",
            model_dir.display()
        ));
    }
    let tokens_path = model_dir.join("tokens.txt");
    if !tokens_path.is_file() {
        return Err(format!("tokens.txt not found in {}", model_dir.display()));
    }

    Ok(mcode_voice::OnlineRecognizerConfig {
        sample_rate: 16_000,
        feature_dim: 80,
        transducer,
        paraformer,
        tokens_path: tokens_path.to_string_lossy().into_owned(),
        num_threads: 1,
        debug: false,
        provider: "cpu".to_owned(),
        decoding_method: "greedy_search".to_owned(),
        enable_endpoint: false,
        rule1_min_trailing_silence: 2.4,
        rule2_min_trailing_silence: 1.2,
        rule3_min_utterance_length: 20.0,
    })
}

/// Pre-loads `sherpa-onnx-c-api.dll` (and, via the altered search path, its
/// `onnxruntime.dll` sibling) so the delay-loaded FFI symbols resolve. The
/// library handle is intentionally never freed — it lives for the process.
#[cfg(windows)]
fn preload_sherpa_dll() -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(
            name: *const u16,
            file: *mut core::ffi::c_void,
            flags: u32,
        ) -> *mut core::ffi::c_void;
    }

    let dir = mcode_voice::sherpa_lib_dir()
        .ok_or("sherpa-onnx DLL dir not found; set MCODE_SHERPA_LIB_DIR")?;
    let dll = dir.join("sherpa-onnx-c-api.dll");
    if !dll.is_file() {
        return Err(format!("missing {}", dll.display()));
    }
    // LOAD_WITH_ALTERED_SEARCH_PATH (0x8): dependent DLLs resolve from the
    // DLL's own directory.
    let wide: Vec<u16> = dll
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0x8) };
    if handle.is_null() {
        return Err(format!("LoadLibraryExW failed for {}", dll.display()));
    }
    Ok(())
}

#[cfg(not(windows))]
fn preload_sherpa_dll() -> Result<(), String> {
    Ok(())
}

/// Recognizer thread: drains the ring, decodes, emits partials; on `dead`
/// flushes (`input_finished` + final decode) and publishes the final text.
/// Any failure emits `speech://error` and marks the session dead.
fn spawn_recognizer_thread(
    app: tauri::AppHandle,
    session_id: String,
    model_dir: PathBuf,
    handle: &DictationHandle,
) {
    let ring = Arc::clone(&handle.ring);
    let dead = Arc::clone(&handle.dead);
    let finished = Arc::clone(&handle.finished);
    let final_slot = Arc::clone(&handle.final_text);

    let spawn = std::thread::Builder::new()
        .name(format!("speech-recognizer-{session_id}"))
        .spawn(move || {
            let fail = |message: String| {
                eprintln!("[speech] recognizer unavailable: {message}");
                emit_json(
                    &app,
                    ERROR_EVENT,
                    &serde_json::json!({ "sessionId": session_id, "error": message }),
                );
                dead.store(true, Ordering::SeqCst);
                finished.store(true, Ordering::SeqCst);
            };

            if let Err(error) = preload_sherpa_dll() {
                fail(format!("sherpa DLL load failed: {error}"));
                return;
            }
            let config = match recognizer_config(&model_dir) {
                Ok(config) => config,
                Err(error) => {
                    fail(error);
                    return;
                }
            };
            let recognizer = match mcode_voice::FfiOnlineRecognizer::new(&config) {
                Ok(recognizer) => recognizer,
                Err(error) => {
                    fail(format!("recognizer creation failed: {error}"));
                    return;
                }
            };
            let mut stream = match recognizer.create_stream() {
                Ok(stream) => stream,
                Err(error) => {
                    fail(format!("stream creation failed: {error}"));
                    return;
                }
            };

            let mut last_partial = String::new();
            while !dead.load(Ordering::SeqCst) {
                let (chunk, sample_rate) = ring.drain();
                if !chunk.is_empty() {
                    stream.accept_waveform(sample_rate, &chunk);
                }
                stream.decode();
                match stream.result_text() {
                    Ok(text) if text != last_partial => {
                        last_partial = text.clone();
                        emit_json(
                            &app,
                            PARTIAL_EVENT,
                            &serde_json::json!({ "sessionId": session_id, "text": text }),
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        fail(format!("result read failed: {error}"));
                        return;
                    }
                }
                if chunk.is_empty() {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }

            // Flush: end of input, drain the decoder, publish the final text.
            stream.input_finished();
            stream.decode();
            let final_text = stream.result_text().unwrap_or_default();
            *final_slot.lock().expect("final text lock") = final_text;
            finished.store(true, Ordering::SeqCst);
        });
    if let Err(error) = spawn {
        eprintln!("[speech] failed to spawn recognizer thread: {error}");
        handle.dead.store(true, Ordering::SeqCst);
        handle.finished.store(true, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Download transport: reqwest blocking with Range-resume
// ---------------------------------------------------------------------------

fn download_client() -> Result<reqwest::blocking::Client, String> {
    // reqwest is built without a bundled crypto provider (`rustls-no-provider`,
    // matching tauri-plugin-updater's ring-based rustls); install the ring
    // provider once per process before the first client is built.
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("download client build failed: {error}"))
}

/// Fetches `[start, ∞)` of one catalog file. Returns a streaming reader plus
/// the absolute total size when known. A 200 answer to a non-zero range means
/// the server ignored `Range` — resuming would corrupt the hash, so fail into
/// the retry ladder instead.
fn fetch_byte_range(
    client: &reqwest::blocking::Client,
    url: &str,
    file: &ModelFileSpec,
    start: u64,
) -> Result<(Box<dyn Read + 'static>, Option<u64>), String> {
    let mut request = client.get(url);
    if start > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={start}-"));
    }
    let response = request
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("request for {} failed: {error}", file.name))?;

    let total = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let remaining = response
            .content_length()
            .unwrap_or(file.size_bytes.saturating_sub(start));
        Some(start + remaining)
    } else {
        if start > 0 {
            return Err(format!(
                "server ignored Range request for {} (resume unsupported)",
                file.name
            ));
        }
        Some(response.content_length().unwrap_or(file.size_bytes))
    };
    Ok((Box::new(response), total))
}

fn download_error_string(error: &DownloadError) -> String {
    match error {
        DownloadError::Cancelled(file) => format!("download cancelled: {file}"),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Returns the full local speech model catalog (`speech:getCatalog`).
#[tauri::command(rename_as = "ui_speechGetCatalog")]
pub(crate) fn ui_speech_get_catalog() -> Vec<SpeechModelDto> {
    CATALOG.iter().map(model_dto).collect()
}

/// Maps model id → download state (`speech:getModelStates`).
#[tauri::command(rename_as = "ui_speechGetModelStates")]
pub(crate) fn ui_speech_get_model_states(
    state: tauri::State<'_, VoiceState>,
) -> Result<HashMap<String, String>, String> {
    let mut states = HashMap::new();
    for entry in CATALOG {
        states.insert(entry.id.to_owned(), state.state_string(entry.id)?);
    }
    Ok(states)
}

/// Starts downloading a catalog model (`speech:downloadModel`). Returns
/// immediately; progress flows via `speech://downloadProgress`, completion
/// via `speech://modelReady`.
#[tauri::command(rename_as = "ui_speechDownloadModel")]
pub(crate) fn ui_speech_download_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, VoiceState>,
    model_id: String,
) -> Result<(), String> {
    let entry = state
        .catalog_entry(&model_id)
        .ok_or_else(|| format!("unknown model: {model_id}"))?;
    if state.downloads.state(&model_id) == ModelDownloadState::Downloading {
        return Err(format!("download already in progress: {model_id}"));
    }

    let requested_dir = state.model_dir(&model_id)?;
    // Non-ASCII model roots get the ASCII ProgramData mirror (sherpa-onnx
    // 1.12.x cannot load models from non-ASCII Windows paths); downloads
    // write to the resolved location so sessions can load from it.
    let model_dir = mcode_voice::session::resolve_model_dir(&requested_dir);
    std::fs::create_dir_all(&model_dir)
        .map_err(|error| format!("failed to create {}: {error}", model_dir.display()))?;

    let downloads = Arc::clone(&state.downloads);
    let progress_app = app.clone();
    let ready_model_id = model_id.clone();
    let client = Arc::new(download_client()?);

    std::thread::Builder::new()
        .name(format!("speech-download-{model_id}"))
        .spawn(move || {
            for file in entry.files {
                let url = mcode_voice::download::catalog_url(entry, file);
                let client = Arc::clone(&client);
                let result = download_file(
                    &downloads,
                    entry,
                    file,
                    &model_dir,
                    |spec, start| fetch_byte_range(&client, &url, spec, start),
                    |downloaded, total| {
                        emit_json(
                            &progress_app,
                            DOWNLOAD_PROGRESS_EVENT,
                            &serde_json::json!({
                                "modelId": entry.id,
                                "fileName": file.name,
                                "downloaded": downloaded,
                                "total": total,
                            }),
                        );
                    },
                );
                if let Err(error) = result {
                    eprintln!(
                        "[speech] download failed for {} / {}: {}",
                        entry.id,
                        file.name,
                        download_error_string(&error)
                    );
                    return;
                }
            }
            emit_json(
                &progress_app,
                MODEL_READY_EVENT,
                &serde_json::json!({ "modelId": ready_model_id }),
            );
        })
        .map_err(|error| format!("failed to spawn download thread: {error}"))?;
    Ok(())
}

/// Cancels an in-flight download (`speech:cancelDownload`).
#[tauri::command(rename_as = "ui_speechCancelDownload")]
pub(crate) fn ui_speech_cancel_download(
    state: tauri::State<'_, VoiceState>,
    model_id: String,
) -> Result<(), String> {
    if state.downloads.cancel(&model_id) {
        Ok(())
    } else {
        Err(format!("no download in progress for {model_id}"))
    }
}

/// Registers a dictation session (`speech:startDictation`). The recognizer
/// thread starts lazily on the first feed.
#[tauri::command(rename_as = "ui_speechStartDictation")]
pub(crate) fn ui_speech_start_dictation(
    app: tauri::AppHandle,
    state: tauri::State<'_, VoiceState>,
    session_id: String,
    owner: String,
    model_id: String,
) -> Result<(), String> {
    let entry = state
        .catalog_entry(&model_id)
        .ok_or_else(|| format!("unknown model: {model_id}"))?;
    if !entry.streaming {
        return Err(format!(
            "model {model_id} is not a streaming recognizer; dictation requires a streaming model"
        ));
    }
    let requested_dir = state.model_dir(&model_id)?;
    state
        .sessions
        .start(
            &owner,
            &session_id,
            &model_id,
            &requested_dir,
            speech_sink(&app),
        )
        .map_err(|error| error.to_string())
}

/// Feeds PCM f32 samples into a session (`speech:feedAudio`). The first feed
/// lazily spawns the recognizer thread.
#[tauri::command(rename_as = "ui_speechFeedAudio")]
pub(crate) fn ui_speech_feed_audio(
    app: tauri::AppHandle,
    state: tauri::State<'_, VoiceState>,
    session_id: String,
    samples: Vec<f32>,
    sample_rate: i32,
) -> Result<(), String> {
    let handle = {
        let mut dictations = state.dictations.lock().expect("dictations lock");
        if let Some(existing) = dictations.get(&session_id)
            && existing.dead.load(Ordering::SeqCst)
        {
            return Err(format!("dictation session {session_id} is dead"));
        }

        if !state
            .sessions
            .feed_audio(&session_id, &samples, sample_rate)
        {
            return Err(format!(
                "audio dropped for session {session_id}: unknown or stopping"
            ));
        }

        match dictations.get(&session_id) {
            Some(existing) => DictationHandle {
                ring: Arc::clone(&existing.ring),
                dead: Arc::clone(&existing.dead),
                finished: Arc::clone(&existing.finished),
                final_text: Arc::clone(&existing.final_text),
            },
            None => {
                let handle = DictationHandle {
                    ring: Arc::new(AudioRing::new()),
                    dead: Arc::new(AtomicBool::new(false)),
                    finished: Arc::new(AtomicBool::new(false)),
                    final_text: Arc::new(Mutex::new(String::new())),
                };
                let model_id = state
                    .sessions
                    .session_model(&session_id)
                    .ok_or_else(|| format!("unknown session: {session_id}"))?;
                let model_dir =
                    mcode_voice::session::resolve_model_dir(&state.model_dir(&model_id)?);
                spawn_recognizer_thread(app, session_id.clone(), model_dir, &handle);
                dictations.insert(
                    session_id.clone(),
                    DictationHandle {
                        ring: Arc::clone(&handle.ring),
                        dead: Arc::clone(&handle.dead),
                        finished: Arc::clone(&handle.finished),
                        final_text: Arc::clone(&handle.final_text),
                    },
                );
                handle
            }
        }
    };
    handle.ring.push(&samples, sample_rate);
    Ok(())
}

/// Stops a dictation session (`speech:stopDictation`): waits (bounded) for
/// the recognizer flush, feeds the final text through the session service,
/// and tears the session down.
#[tauri::command(rename_as = "ui_speechStopDictation")]
pub(crate) fn ui_speech_stop_dictation(
    state: tauri::State<'_, VoiceState>,
    session_id: String,
) -> Result<(), String> {
    let handle = state
        .dictations
        .lock()
        .expect("dictations lock")
        .remove(&session_id);

    let final_text = if let Some(handle) = handle {
        handle.dead.store(true, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + STOP_FLUSH_TIMEOUT;
        while !handle.finished.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(STOP_FLUSH_POLL);
        }
        if handle.finished.load(Ordering::SeqCst) {
            handle.final_text.lock().expect("final text lock").clone()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    if state.sessions.stop(&session_id, &final_text) {
        Ok(())
    } else {
        Err(format!("unknown dictation session: {session_id}"))
    }
}
