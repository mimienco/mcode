//! High-level safe wrapper around the raw sherpa-onnx FFI.
//!
//! # Thread-safety (documented assumptions)
//!
//! The v1.12.37 C API does not publish an explicit thread-safety contract in
//! `c-api.h`. Observed upstream usage (examples + Node addon internals) shows
//! a recognizer and its streams being driven from a single worker thread; the
//! ONNX Runtime sessions inside the recognizer are internally synchronized
//! (intra-op thread pools), but `OnlineStream` decode state is not designed
//! for concurrent mutation. This wrapper therefore:
//! - makes [`FfiOnlineRecognizer`] `Send` (move between threads, one owner);
//! - does **not** implement `Sync`: concurrent use of `&self` methods from
//!   multiple threads on the same recognizer/stream pair is not guaranteed
//!   safe by the header and is forbidden here;
//! - documents that a recognizer and all streams created from it must be
//!   used from one thread at a time (the stt-worker model).

use std::ffi::{CStr, CString};
use std::path::Path;

pub mod ffi;

// Note: tests intentionally do NOT link against the DLL. The wrapper's
// entry points are exercised by the #[ignore]d model test with the DLL
// loaded at runtime via LoadLibraryExW; the always-on unit tests (layout,
// wav parsing) never touch FFI symbols, so `cargo test -p mcode-voice`
// links and passes with no sherpa DLL present.

/// Error type for the voice FFI wrapper.
#[derive(Debug)]
pub enum SherpaError {
    /// A string field contained an interior NUL byte and could not be
    /// converted to a `CString`.
    InteriorNul(&'static str),
    /// `SherpaOnnxCreateOnlineRecognizer` returned NULL.
    RecognizerCreationFailed,
    /// The recognizer or stream handle was used after being consumed.
    HandleClosed,
    /// The recognizer returned a NULL result snapshot.
    NullResult,
}

impl std::fmt::Display for SherpaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SherpaError::InteriorNul(field) => {
                write!(f, "config field `{field}` contains interior NUL")
            }
            SherpaError::RecognizerCreationFailed => {
                write!(f, "SherpaOnnxCreateOnlineRecognizer returned NULL")
            }
            SherpaError::HandleClosed => write!(f, "handle used after close"),
            SherpaError::NullResult => write!(f, "recognizer returned NULL result"),
        }
    }
}

impl std::error::Error for SherpaError {}

/// The spike-scoped streaming recognizer configuration.
///
/// Only the fields the stt-worker actually sets are surfaced; everything else
/// is zero/NULL-initialized per the header's memset-0 contract. Layout is
/// pinned to the v1.12.37 C header by `layout_tests` in `ffi.rs`.
#[derive(Debug, Clone)]
pub struct OnlineRecognizerConfig {
    pub sample_rate: i32,
    pub feature_dim: i32,
    pub transducer: Option<TransducerPaths>,
    pub paraformer: Option<ParaformerPaths>,
    pub tokens_path: String,
    pub num_threads: i32,
    pub debug: bool,
    pub provider: String,
    pub decoding_method: String,
    pub enable_endpoint: bool,
    pub rule1_min_trailing_silence: f32,
    pub rule2_min_trailing_silence: f32,
    pub rule3_min_utterance_length: f32,
}

/// Streaming transducer model file paths.
#[derive(Debug, Clone)]
pub struct TransducerPaths {
    pub encoder: String,
    pub decoder: String,
    pub joiner: String,
}

/// Streaming paraformer model file paths.
#[derive(Debug, Clone)]
pub struct ParaformerPaths {
    pub encoder: String,
    pub decoder: String,
}

/// Streaming (online) recognizer backed by `sherpa-onnx-c-api.dll`.
///
/// `Send` but not `Sync` — see the module docs for the thread-safety contract.
pub struct FfiOnlineRecognizer {
    /// Invariant: non-null while `Some` in `handle`.
    handle: Option<ffi::SherpaOnnxOnlineRecognizerHandle>,
    /// Owns the CStrings whose pointers were passed to the FFI at creation;
    /// must outlive the recognizer. Empty after `close()`.
    _strings: Vec<CString>,
}

// Safety: the raw handle is a plain pointer to a sherpa-onnx recognizer. The
// C API publishes no thread-affinity requirement; all `extern "C"` entry
// points take `&self`-shaped const pointers, and the stt-worker already moves
// the equivalent JS object across the worker-thread boundary. We opt into
// `Send` (single-owner move) and explicitly withhold `Sync`.
unsafe impl Send for FfiOnlineRecognizer {}

impl FfiOnlineRecognizer {
    /// Create a streaming recognizer from `config`.
    ///
    /// Mirrors the C usage pattern: zero-initialized struct, fill the
    /// spike-relevant fields, call `SherpaOnnxCreateOnlineRecognizer`.
    pub fn new(config: &OnlineRecognizerConfig) -> Result<Self, SherpaError> {
        let strings = Strings::build(config)?;
        let raw = ffi::SherpaOnnxOnlineRecognizerConfig {
            feat_config: ffi::SherpaOnnxFeatureConfig {
                sample_rate: config.sample_rate,
                feature_dim: config.feature_dim,
            },
            model_config: ffi::SherpaOnnxOnlineModelConfig {
                transducer: match &config.transducer {
                    Some(_) => ffi::SherpaOnnxOnlineTransducerModelConfig {
                        encoder: strings.encoder(),
                        decoder: strings.decoder(),
                        joiner: strings.joiner(),
                    },
                    None => ffi::SherpaOnnxOnlineTransducerModelConfig {
                        encoder: std::ptr::null(),
                        decoder: std::ptr::null(),
                        joiner: std::ptr::null(),
                    },
                },
                paraformer: match &config.paraformer {
                    Some(_) => ffi::SherpaOnnxOnlineParaformerModelConfig {
                        encoder: strings.paraformer_encoder(),
                        decoder: strings.paraformer_decoder(),
                    },
                    None => ffi::SherpaOnnxOnlineParaformerModelConfig {
                        encoder: std::ptr::null(),
                        decoder: std::ptr::null(),
                    },
                },
                zipformer2_ctc_model: std::ptr::null(),
                tokens: strings.tokens(),
                num_threads: config.num_threads,
                provider: strings.provider(),
                debug: i32::from(config.debug),
                model_type: std::ptr::null(),
                modeling_unit: std::ptr::null(),
                bpe_vocab: std::ptr::null(),
                tokens_buf: std::ptr::null(),
                tokens_buf_size: 0,
                nemo_ctc_model: std::ptr::null(),
                t_one_ctc_model: std::ptr::null(),
            },
            decoding_method: strings.decoding_method(),
            max_active_paths: 0,
            enable_endpoint: i32::from(config.enable_endpoint),
            rule1_min_trailing_silence: config.rule1_min_trailing_silence,
            rule2_min_trailing_silence: config.rule2_min_trailing_silence,
            rule3_min_utterance_length: config.rule3_min_utterance_length,
            hotwords_file: std::ptr::null(),
            hotwords_score: 0.0,
            ctc_fst_decoder_config: ffi::SherpaOnnxOnlineCtcFstDecoderConfig {
                graph: std::ptr::null(),
                max_active: 0,
            },
            rule_fsts: std::ptr::null(),
            rule_fars: std::ptr::null(),
            blank_penalty: 0.0,
            hotwords_buf: std::ptr::null(),
            hotwords_buf_size: 0,
            hr: ffi::SherpaOnnxHomophoneReplacerConfig {
                dict_dir: std::ptr::null(),
                lexicon: std::ptr::null(),
                rule_fsts: std::ptr::null(),
            },
        };

        // Safety: `raw` borrows only from `strings`, which outlives the call;
        // the callee copies the config.
        let handle = unsafe { ffi::SherpaOnnxCreateOnlineRecognizer(&raw) };
        if handle.is_null() {
            return Err(SherpaError::RecognizerCreationFailed);
        }
        Ok(Self {
            handle: Some(handle),
            _strings: strings.into_inner(),
        })
    }

    /// Create a new decoding stream for this recognizer.
    pub fn create_stream(&self) -> Result<FfiOnlineStream, SherpaError> {
        let handle = self.handle.ok_or(SherpaError::HandleClosed)?;
        // Safety: `handle` is a live recognizer pointer.
        let stream = unsafe { ffi::SherpaOnnxCreateOnlineStream(handle) };
        if stream.is_null() {
            return Err(SherpaError::RecognizerCreationFailed);
        }
        Ok(FfiOnlineStream {
            handle: Some(stream),
            recognizer: handle,
        })
    }

    /// sherpa-onnx version string reported by the loaded DLL.
    pub fn version(&self) -> Result<String, SherpaError> {
        self.handle.ok_or(SherpaError::HandleClosed)?;
        // Safety: returns a pointer to static library-owned memory.
        let ptr = unsafe { ffi::SherpaOnnxGetVersionStr() };
        if ptr.is_null() {
            return Err(SherpaError::NullResult);
        }
        // Safety: NUL-terminated static string per the header contract.
        Ok(unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned())
    }
}

impl Drop for FfiOnlineRecognizer {
    fn drop(&mut self) {
        self.close();
    }
}

impl FfiOnlineRecognizer {
    /// Destroy the underlying recognizer. Called by `Drop`; safe to call
    /// manually to force teardown order before dependent state is released.
    pub fn close(&mut self) {
        if let Some(handle) = self.handle.take() {
            // Safety: handle came from `SherpaOnnxCreateOnlineRecognizer` and
            // has not been destroyed yet.
            unsafe { ffi::SherpaOnnxDestroyOnlineRecognizer(handle) };
        }
    }
}

/// A decoding stream bound to its creating recognizer.
///
/// The recognizer reference is held for the lifetime of FFI calls that need
/// it (`decode`, `is_endpoint`, `reset`); the C API requires the recognizer
/// pointer for those entry points.
pub struct FfiOnlineStream {
    /// Invariant: non-null while `Some`; recognizer handle outlives stream.
    handle: Option<ffi::SherpaOnnxOnlineStreamHandle>,
    recognizer: ffi::SherpaOnnxOnlineRecognizerHandle,
}

impl FfiOnlineStream {
    /// Append mono f32 samples in `[-1, 1]` at `sample_rate` Hz.
    pub fn accept_waveform(&mut self, sample_rate: i32, samples: &[f32]) {
        let Some(handle) = self.handle else {
            return;
        };
        // Safety: `samples` is a valid pointer for `samples.len()` floats for
        // the duration of the call; the callee does not retain the pointer.
        unsafe {
            ffi::SherpaOnnxOnlineStreamAcceptWaveform(
                handle,
                sample_rate,
                samples.as_ptr(),
                samples.len() as i32,
            );
        }
    }

    /// Whether enough frames are buffered to run a decode step.
    pub fn is_ready(&self) -> bool {
        let Some(handle) = self.handle else {
            return false;
        };
        // Safety: live recognizer/stream pair.
        unsafe { ffi::SherpaOnnxIsOnlineStreamReady(self.recognizer, handle) != 0 }
    }

    /// Run one decode step. Only call when [`is_ready`](Self::is_ready) is
    /// true; [`decode`](Self::decode) wraps this in the ready-loop contract.
    pub fn decode_step(&self) {
        let Some(handle) = self.handle else {
            return;
        };
        // Safety: live recognizer/stream pair; header requires is_ready() first.
        unsafe { ffi::SherpaOnnxDecodeOnlineStream(self.recognizer, handle) };
    }

    /// Decode until no more buffered frames remain (`while is_ready`).
    pub fn decode(&self) {
        while self.is_ready() {
            self.decode_step();
        }
    }

    /// Signal end of input; no more samples may be accepted afterwards.
    pub fn input_finished(&mut self) {
        let Some(handle) = self.handle else {
            return;
        };
        // Safety: live stream handle.
        unsafe { ffi::SherpaOnnxOnlineStreamInputFinished(handle) };
    }

    /// Snapshot the current recognition text.
    pub fn result_text(&self) -> Result<String, SherpaError> {
        let handle = self.handle.ok_or(SherpaError::HandleClosed)?;
        // Safety: live recognizer/stream pair.
        let result = unsafe { ffi::SherpaOnnxGetOnlineStreamResult(self.recognizer, handle) };
        if result.is_null() {
            return Err(SherpaError::NullResult);
        }
        // Safety: result came from `GetOnlineStreamResult`; destroyed below
        // before any early return.
        let r = unsafe { &*result };
        let text = if r.text.is_null() {
            String::new()
        } else {
            // Safety: NUL-terminated for the lifetime of the result object.
            unsafe { CStr::from_ptr(r.text) }
                .to_string_lossy()
                .into_owned()
        };
        // Safety: pointer came from `GetOnlineStreamResult` and was not freed above.
        unsafe { ffi::SherpaOnnxDestroyOnlineRecognizerResult(result) };
        Ok(text)
    }

    /// Whether endpoint detection has triggered.
    pub fn is_endpoint(&self) -> bool {
        let Some(handle) = self.handle else {
            return false;
        };
        // Safety: live recognizer/stream pair.
        unsafe { ffi::SherpaOnnxOnlineStreamIsEndpoint(self.recognizer, handle) != 0 }
    }

    /// Clear decoder state after an endpoint so the stream can be reused.
    pub fn reset(&self) {
        let Some(handle) = self.handle else {
            return;
        };
        // Safety: live recognizer/stream pair.
        unsafe { ffi::SherpaOnnxOnlineStreamReset(self.recognizer, handle) };
    }

    /// Destroy the stream handle. Called by `Drop`.
    pub fn close(&mut self) {
        if let Some(handle) = self.handle.take() {
            // Safety: handle came from `SherpaOnnxCreateOnlineStream` and has
            // not been destroyed yet.
            unsafe { ffi::SherpaOnnxDestroyOnlineStream(handle) };
        }
    }
}

impl Drop for FfiOnlineStream {
    fn drop(&mut self) {
        self.close();
    }
}

/// Builds and owns the `CString`s backing every string pointer handed to the
/// FFI for one recognizer construction.
struct Strings {
    inner: Vec<(StringKey, CString)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringKey {
    TransducerEncoder,
    TransducerDecoder,
    TransducerJoiner,
    ParaformerEncoder,
    ParaformerDecoder,
    Tokens,
    Provider,
    DecodingMethod,
}

impl Strings {
    fn build(config: &OnlineRecognizerConfig) -> Result<Self, SherpaError> {
        let mut items: Vec<(StringKey, CString)> = Vec::with_capacity(6);
        let mut push = |key: StringKey, value: &str| -> Result<(), SherpaError> {
            CString::new(value)
                .map(|c| items.push((key, c)))
                .map_err(|_| SherpaError::InteriorNul(key.field_name()))
        };

        if let Some(t) = &config.transducer {
            push(StringKey::TransducerEncoder, &t.encoder)?;
            push(StringKey::TransducerDecoder, &t.decoder)?;
            push(StringKey::TransducerJoiner, &t.joiner)?;
        }
        if let Some(p) = &config.paraformer {
            push(StringKey::ParaformerEncoder, &p.encoder)?;
            push(StringKey::ParaformerDecoder, &p.decoder)?;
        }
        push(StringKey::Tokens, &config.tokens_path)?;
        push(StringKey::Provider, &config.provider)?;
        push(StringKey::DecodingMethod, &config.decoding_method)?;
        Ok(Self { inner: items })
    }

    fn lookup(&self, key: StringKey) -> *const std::os::raw::c_char {
        self.inner
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(std::ptr::null(), |(_, c)| c.as_ptr())
    }

    fn encoder(&self) -> *const std::os::raw::c_char {
        self.lookup(StringKey::TransducerEncoder)
    }
    fn decoder(&self) -> *const std::os::raw::c_char {
        self.lookup(StringKey::TransducerDecoder)
    }
    fn joiner(&self) -> *const std::os::raw::c_char {
        self.lookup(StringKey::TransducerJoiner)
    }
    fn paraformer_encoder(&self) -> *const std::os::raw::c_char {
        self.lookup(StringKey::ParaformerEncoder)
    }
    fn paraformer_decoder(&self) -> *const std::os::raw::c_char {
        self.lookup(StringKey::ParaformerDecoder)
    }
    fn tokens(&self) -> *const std::os::raw::c_char {
        self.lookup(StringKey::Tokens)
    }
    fn provider(&self) -> *const std::os::raw::c_char {
        self.lookup(StringKey::Provider)
    }
    fn decoding_method(&self) -> *const std::os::raw::c_char {
        self.lookup(StringKey::DecodingMethod)
    }

    fn into_inner(self) -> Vec<CString> {
        self.inner.into_iter().map(|(_, c)| c).collect()
    }
}

impl StringKey {
    fn field_name(self) -> &'static str {
        match self {
            StringKey::TransducerEncoder => "transducer.encoder",
            StringKey::TransducerDecoder => "transducer.decoder",
            StringKey::TransducerJoiner => "transducer.joiner",
            StringKey::ParaformerEncoder => "paraformer.encoder",
            StringKey::ParaformerDecoder => "paraformer.decoder",
            StringKey::Tokens => "tokens",
            StringKey::Provider => "provider",
            StringKey::DecodingMethod => "decoding_method",
        }
    }
}

/// Read a PCM16 mono WAV file with a small hand-rolled parser.
///
/// Returns `(sample_rate, samples)` with samples normalized to `[-1, 1]`.
/// Supports the canonical 44-byte canonical PCM16 WAV header and also
/// tolerates a `LIST` chunk before `data` (common in files written by
/// sherpa-onnx tooling).
pub fn read_wav_pcm16_mono(path: &Path) -> Result<(i32, Vec<f32>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_wav_pcm16_mono(&bytes)
}

/// Parse an in-memory PCM16 mono WAV. See [`read_wav_pcm16_mono`].
pub fn parse_wav_pcm16_mono(bytes: &[u8]) -> Result<(i32, Vec<f32>), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".to_string());
    }

    let mut pos = 12usize;
    let mut sample_rate: Option<i32> = None;
    let mut channels: Option<u16> = None;
    let mut bits: Option<u16> = None;
    let mut format_tag: Option<u16> = None;
    let mut data: Option<&[u8]> = None;

    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body = pos + 8;
        if body + size > bytes.len() {
            return Err(format!("chunk `{}` truncated", String::from_utf8_lossy(id)));
        }
        match id {
            b"fmt " => {
                if size < 16 {
                    return Err("fmt chunk shorter than 16 bytes".to_string());
                }
                format_tag = Some(u16::from_le_bytes([bytes[body], bytes[body + 1]]));
                channels = Some(u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]));
                sample_rate = Some(i32::from_le_bytes([
                    bytes[body + 4],
                    bytes[body + 5],
                    bytes[body + 6],
                    bytes[body + 7],
                ]));
                bits = Some(u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]));
            }
            b"data" => data = Some(&bytes[body..body + size]),
            _ => {}
        }
        // Chunks are word-aligned; odd sizes carry one padding byte.
        pos = body + size + (size & 1);
    }

    let rate = sample_rate.ok_or("missing fmt chunk")?;
    let channels = channels.ok_or("missing fmt chunk")?;
    let bits = bits.ok_or("missing fmt chunk")?;
    let tag = format_tag.ok_or("missing fmt chunk")?;
    let data = data.ok_or("missing data chunk")?;

    if tag != 1 {
        return Err(format!(
            "unsupported WAV format tag {tag} (expected PCM = 1)"
        ));
    }
    if bits != 16 {
        return Err(format!("unsupported bit depth {bits} (expected PCM16)"));
    }
    if channels != 1 {
        return Err(format!("expected mono WAV, got {channels} channels"));
    }
    let samples = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c) as f32 / 32768.0)
        .collect();
    Ok((rate, samples))
}

/// Locate the directory containing the sherpa-onnx DLLs.
///
/// Resolution order:
/// 1. `MCODE_SHERPA_LIB_DIR` (explicit override);
/// 2. `<workspace root>/node_modules/sherpa-onnx-win-x64`, discovered by
///    walking up from `CARGO_MANIFEST_DIR` until a `node_modules` ancestor
///    directory exists (workspace root = the directory owning `Cargo.toml`
///    with a `[workspace]` table).
pub fn sherpa_lib_dir() -> Option<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("MCODE_SHERPA_LIB_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut dir = manifest.as_path();
    while let Some(parent) = dir.parent() {
        dir = parent;
        let candidate = dir.join("node_modules").join("sherpa-onnx-win-x64");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

use std::path::PathBuf;
