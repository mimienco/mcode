//! Scoped filesystem core for the Tauri migration.
//!
//! Splits the `fs:*` surface into authorization (`scope`), operations
//! (`ops`), search and replace (`search`), and watching (`watcher`),
//! mirroring the behavior contract in `docs/migration/FS_CONTRACT_SPEC.md`.

pub mod ops;
pub mod scope;
pub mod search;
pub mod watcher;
