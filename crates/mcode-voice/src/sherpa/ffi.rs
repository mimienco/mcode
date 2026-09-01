//! Raw FFI bindings to the sherpa-onnx flat C API (`sherpa-onnx-c-api.dll`).
//!
//! Struct layouts are mirrored exactly from the upstream sherpa-onnx
//! `sherpa-onnx/c-api/c-api.h` at **v1.12.37**, matching the version of the
//! prebuilt DLLs shipped in `node_modules/sherpa-onnx-win-x64/`
//! (`sherpa-onnx-c-api.dll` 4.2 MB, v1.12.37 per package.json).
//!
//! Field order, types, and sizes here MUST match the C header; the
//! `SherpaOnnxOnlineRecognizerConfig` nested-struct layout is pinned by a
//! compile-time `size_of` assertion in `layout_tests`.
//!
//! Only the Online (streaming) ASR surface is bound — the spike's scope.
//! Offline / sense-voice recognition is out of scope.

use std::os::raw::{c_char, c_float, c_int};

/// `SherpaOnnxOnlineTransducerModelConfig` — encoder/decoder/joiner paths.
///
/// Layout verified against sherpa-onnx v1.12.37 C API: three `const char *`
/// fields, 24 bytes on x64.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SherpaOnnxOnlineTransducerModelConfig {
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub joiner: *const c_char,
}

/// `SherpaOnnxOnlineParaformerModelConfig` — encoder/decoder paths.
///
/// Layout verified against sherpa-onnx v1.12.37 C API: two `const char *`
/// fields, 16 bytes on x64.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SherpaOnnxOnlineParaformerModelConfig {
    pub encoder: *const c_char,
    pub decoder: *const c_char,
}

/// `SherpaOnnxFeatureConfig` — sample rate + feature dimension.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SherpaOnnxFeatureConfig {
    pub sample_rate: c_int,
    pub feature_dim: c_int,
}

/// `SherpaOnnxOnlineModelConfig` — shared streaming model configuration.
///
/// Mirrors the full v1.12.37 field order (transducer, paraformer, zipformer2_ctc,
/// tokens, num_threads, provider, debug, model_type, modeling_unit, bpe_vocab,
/// tokens_buf, tokens_buf_size, nemo_ctc, t_one_ctc). Zipformer2-CTC, NeMo-CTC,
/// and T-One-CTC are single-`model`-pointer structs bound inline as raw pointer
/// fields so the overall offset arithmetic matches the C ABI without needing
/// dedicated wrapper types for models the spike never configures.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SherpaOnnxOnlineModelConfig {
    pub transducer: SherpaOnnxOnlineTransducerModelConfig,
    pub paraformer: SherpaOnnxOnlineParaformerModelConfig,
    /// `SherpaOnnxOnlineZipformer2CtcModelConfig.model`
    pub zipformer2_ctc_model: *const c_char,
    pub tokens: *const c_char,
    pub num_threads: c_int,
    pub provider: *const c_char,
    pub debug: c_int,
    pub model_type: *const c_char,
    pub modeling_unit: *const c_char,
    pub bpe_vocab: *const c_char,
    pub tokens_buf: *const c_char,
    pub tokens_buf_size: c_int,
    /// `SherpaOnnxOnlineNemoCtcModelConfig.model`
    pub nemo_ctc_model: *const c_char,
    /// `SherpaOnnxOnlineToneCtcModelConfig.model`
    pub t_one_ctc_model: *const c_char,
}

/// `SherpaOnnxOnlineCtcFstDecoderConfig` — graph path + max_active.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SherpaOnnxOnlineCtcFstDecoderConfig {
    pub graph: *const c_char,
    pub max_active: c_int,
}

/// `SherpaOnnxHomophoneReplacerConfig` — dict_dir/lexicon/rule_fsts.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SherpaOnnxHomophoneReplacerConfig {
    pub dict_dir: *const c_char,
    pub lexicon: *const c_char,
    pub rule_fsts: *const c_char,
}

/// `SherpaOnnxOnlineRecognizerConfig` — full streaming recognizer config.
///
/// Field order matches v1.12.37 exactly:
/// feat_config, model_config, decoding_method, max_active_paths,
/// enable_endpoint, rule1/rule2/rule3, hotwords_file, hotwords_score,
/// ctc_fst_decoder_config, rule_fsts, rule_fars, blank_penalty,
/// hotwords_buf, hotwords_buf_size, hr.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SherpaOnnxOnlineRecognizerConfig {
    pub feat_config: SherpaOnnxFeatureConfig,
    pub model_config: SherpaOnnxOnlineModelConfig,
    pub decoding_method: *const c_char,
    pub max_active_paths: c_int,
    pub enable_endpoint: c_int,
    pub rule1_min_trailing_silence: c_float,
    pub rule2_min_trailing_silence: c_float,
    pub rule3_min_utterance_length: c_float,
    pub hotwords_file: *const c_char,
    pub hotwords_score: c_float,
    pub ctc_fst_decoder_config: SherpaOnnxOnlineCtcFstDecoderConfig,
    pub rule_fsts: *const c_char,
    pub rule_fars: *const c_char,
    pub blank_penalty: c_float,
    pub hotwords_buf: *const c_char,
    pub hotwords_buf_size: c_int,
    pub hr: SherpaOnnxHomophoneReplacerConfig,
}

/// `SherpaOnnxOnlineRecognizerResult` — snapshot returned by
/// `SherpaOnnxGetOnlineStreamResult`.
///
/// Mirrors v1.12.37: text, tokens, tokens_arr, timestamps, count, json.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SherpaOnnxOnlineRecognizerResult {
    pub text: *const c_char,
    pub tokens: *const c_char,
    pub tokens_arr: *const *const c_char,
    pub timestamps: *const c_float,
    pub count: c_int,
    pub json: *const c_char,
}

pub type SherpaOnnxOnlineRecognizerHandle = *const SherpaOnnxOnlineRecognizerOpaque;
pub type SherpaOnnxOnlineStreamHandle = *const SherpaOnnxOnlineStreamOpaque;

#[repr(C)]
pub struct SherpaOnnxOnlineRecognizerOpaque {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SherpaOnnxOnlineStreamOpaque {
    _private: [u8; 0],
}

unsafe extern "C" {
    pub fn SherpaOnnxCreateOnlineRecognizer(
        config: *const SherpaOnnxOnlineRecognizerConfig,
    ) -> SherpaOnnxOnlineRecognizerHandle;
    pub fn SherpaOnnxDestroyOnlineRecognizer(recognizer: SherpaOnnxOnlineRecognizerHandle);
    pub fn SherpaOnnxCreateOnlineStream(
        recognizer: SherpaOnnxOnlineRecognizerHandle,
    ) -> SherpaOnnxOnlineStreamHandle;
    pub fn SherpaOnnxDestroyOnlineStream(stream: SherpaOnnxOnlineStreamHandle);
    pub fn SherpaOnnxOnlineStreamAcceptWaveform(
        stream: SherpaOnnxOnlineStreamHandle,
        sample_rate: c_int,
        samples: *const c_float,
        n: c_int,
    );
    pub fn SherpaOnnxIsOnlineStreamReady(
        recognizer: SherpaOnnxOnlineRecognizerHandle,
        stream: SherpaOnnxOnlineStreamHandle,
    ) -> c_int;
    pub fn SherpaOnnxDecodeOnlineStream(
        recognizer: SherpaOnnxOnlineRecognizerHandle,
        stream: SherpaOnnxOnlineStreamHandle,
    );
    pub fn SherpaOnnxGetOnlineStreamResult(
        recognizer: SherpaOnnxOnlineRecognizerHandle,
        stream: SherpaOnnxOnlineStreamHandle,
    ) -> *const SherpaOnnxOnlineRecognizerResult;
    pub fn SherpaOnnxDestroyOnlineRecognizerResult(r: *const SherpaOnnxOnlineRecognizerResult);
    pub fn SherpaOnnxOnlineStreamReset(
        recognizer: SherpaOnnxOnlineRecognizerHandle,
        stream: SherpaOnnxOnlineStreamHandle,
    );
    pub fn SherpaOnnxOnlineStreamInputFinished(stream: SherpaOnnxOnlineStreamHandle);
    pub fn SherpaOnnxOnlineStreamIsEndpoint(
        recognizer: SherpaOnnxOnlineRecognizerHandle,
        stream: SherpaOnnxOnlineStreamHandle,
    ) -> c_int;
    pub fn SherpaOnnxGetVersionStr() -> *const c_char;
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use std::mem::{size_of, size_of_val};

    #[test]
    fn transducer_config_layout() {
        assert_eq!(size_of::<SherpaOnnxOnlineTransducerModelConfig>(), 24);
        let c = SherpaOnnxOnlineTransducerModelConfig {
            encoder: std::ptr::null(),
            decoder: std::ptr::null(),
            joiner: std::ptr::null(),
        };
        assert_eq!(size_of_val(&c.encoder), 8);
        assert_eq!(size_of_val(&c.decoder), 8);
        assert_eq!(size_of_val(&c.joiner), 8);
    }

    #[test]
    fn paraformer_config_layout() {
        assert_eq!(size_of::<SherpaOnnxOnlineParaformerModelConfig>(), 16);
    }

    #[test]
    fn feature_config_layout() {
        assert_eq!(size_of::<SherpaOnnxFeatureConfig>(), 8);
    }

    #[test]
    fn model_config_field_offsets_match_header_order() {
        // Cumulative offset check on x64: three nested 8-byte-aligned pointer
        // groups and int32 fields must line up exactly with the C header.
        let _c = SherpaOnnxOnlineModelConfig {
            transducer: SherpaOnnxOnlineTransducerModelConfig {
                encoder: std::ptr::null(),
                decoder: std::ptr::null(),
                joiner: std::ptr::null(),
            },
            paraformer: SherpaOnnxOnlineParaformerModelConfig {
                encoder: std::ptr::null(),
                decoder: std::ptr::null(),
            },
            zipformer2_ctc_model: std::ptr::null(),
            tokens: std::ptr::null(),
            num_threads: 0,
            provider: std::ptr::null(),
            debug: 0,
            model_type: std::ptr::null(),
            modeling_unit: std::ptr::null(),
            bpe_vocab: std::ptr::null(),
            tokens_buf: std::ptr::null(),
            tokens_buf_size: 0,
            nemo_ctc_model: std::ptr::null(),
            t_one_ctc_model: std::ptr::null(),
        };
        use std::mem::offset_of;
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, transducer), 0);
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, paraformer), 24);
        assert_eq!(
            offset_of!(SherpaOnnxOnlineModelConfig, zipformer2_ctc_model),
            40
        );
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, tokens), 48);
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, num_threads), 56);
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, provider), 64);
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, debug), 72);
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, model_type), 80);
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, modeling_unit), 88);
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, bpe_vocab), 96);
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, tokens_buf), 104);
        assert_eq!(
            offset_of!(SherpaOnnxOnlineModelConfig, tokens_buf_size),
            112
        );
        assert_eq!(offset_of!(SherpaOnnxOnlineModelConfig, nemo_ctc_model), 120);
        assert_eq!(
            offset_of!(SherpaOnnxOnlineModelConfig, t_one_ctc_model),
            128
        );
        assert_eq!(size_of::<SherpaOnnxOnlineModelConfig>(), 136);
    }

    #[test]
    fn recognizer_config_size_and_offsets() {
        // SherpaOnnxOnlineRecognizerConfig on x64:
        //   feat_config (0..8), model_config (8..144), decoding_method (144..152),
        //   max_active_paths (152..156), enable_endpoint (156..160),
        //   rule1 (160..164), rule2 (164..168), rule3 (168..172), (pad),
        //   hotwords_file (176..184), hotwords_score (184..188), (pad),
        //   ctc_fst_decoder_config (192..208), rule_fsts (208..216),
        //   rule_fars (216..224), blank_penalty (224..228), (pad),
        //   hotwords_buf (232..240), hotwords_buf_size (240..244), (pad),
        //   hr (248..272)
        assert_eq!(size_of::<SherpaOnnxOnlineRecognizerConfig>(), 272);

        let _c = SherpaOnnxOnlineRecognizerConfig {
            feat_config: SherpaOnnxFeatureConfig {
                sample_rate: 0,
                feature_dim: 0,
            },
            model_config: SherpaOnnxOnlineModelConfig {
                transducer: SherpaOnnxOnlineTransducerModelConfig {
                    encoder: std::ptr::null(),
                    decoder: std::ptr::null(),
                    joiner: std::ptr::null(),
                },
                paraformer: SherpaOnnxOnlineParaformerModelConfig {
                    encoder: std::ptr::null(),
                    decoder: std::ptr::null(),
                },
                zipformer2_ctc_model: std::ptr::null(),
                tokens: std::ptr::null(),
                num_threads: 0,
                provider: std::ptr::null(),
                debug: 0,
                model_type: std::ptr::null(),
                modeling_unit: std::ptr::null(),
                bpe_vocab: std::ptr::null(),
                tokens_buf: std::ptr::null(),
                tokens_buf_size: 0,
                nemo_ctc_model: std::ptr::null(),
                t_one_ctc_model: std::ptr::null(),
            },
            decoding_method: std::ptr::null(),
            max_active_paths: 0,
            enable_endpoint: 0,
            rule1_min_trailing_silence: 0.0,
            rule2_min_trailing_silence: 0.0,
            rule3_min_utterance_length: 0.0,
            hotwords_file: std::ptr::null(),
            hotwords_score: 0.0,
            ctc_fst_decoder_config: SherpaOnnxOnlineCtcFstDecoderConfig {
                graph: std::ptr::null(),
                max_active: 0,
            },
            rule_fsts: std::ptr::null(),
            rule_fars: std::ptr::null(),
            blank_penalty: 0.0,
            hotwords_buf: std::ptr::null(),
            hotwords_buf_size: 0,
            hr: SherpaOnnxHomophoneReplacerConfig {
                dict_dir: std::ptr::null(),
                lexicon: std::ptr::null(),
                rule_fsts: std::ptr::null(),
            },
        };
        use std::mem::offset_of;
        assert_eq!(offset_of!(SherpaOnnxOnlineRecognizerConfig, feat_config), 0);
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, model_config),
            8
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, decoding_method),
            144
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, max_active_paths),
            152
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, enable_endpoint),
            156
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, rule1_min_trailing_silence),
            160
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, rule2_min_trailing_silence),
            164
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, rule3_min_utterance_length),
            168
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, hotwords_file),
            176
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, hotwords_score),
            184
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, ctc_fst_decoder_config),
            192
        );
        assert_eq!(offset_of!(SherpaOnnxOnlineRecognizerConfig, rule_fsts), 208);
        assert_eq!(offset_of!(SherpaOnnxOnlineRecognizerConfig, rule_fars), 216);
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, blank_penalty),
            224
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, hotwords_buf),
            232
        );
        assert_eq!(
            offset_of!(SherpaOnnxOnlineRecognizerConfig, hotwords_buf_size),
            240
        );
        assert_eq!(offset_of!(SherpaOnnxOnlineRecognizerConfig, hr), 248);
        assert_eq!(size_of::<SherpaOnnxOnlineRecognizerConfig>(), 272);
    }

    #[test]
    fn result_struct_layout() {
        // text (0..8), tokens (8..16), tokens_arr (16..24), timestamps (24..32),
        // count (32..36), (pad), json (40..48)
        assert_eq!(size_of::<SherpaOnnxOnlineRecognizerResult>(), 48);
    }
}
