//! Diagnostics-surface helpers for the Tauri port: telemetry consent
//! mutation guards and burst caps, ported from the Electron main process.
//!
//! Ports (electron main):
//! - `src/main/telemetry/burst-cap.ts` — consent-mutation session bucket
//! - `src/main/ipc/telemetry.ts:deriveOptInVia` — `via` derivation from
//!   main-owned state
//! - `src/main/telemetry/client.ts:setOptIn` / `persistBannerAcknowledgeWithoutEmitting`
//!   — opt-in persistence contracts
//!
//! Reuses `mcode-app::telemetry_consent::resolve_consent` for the pure
//! consent resolution; this crate owns the mutable session state and the
//! persistence seam over `settings.telemetry`.

pub mod telemetry;

pub use telemetry::{ConsentMutationGate, OptInVia, TelemetrySettings};
