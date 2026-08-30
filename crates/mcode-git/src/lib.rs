//! `mcode-git`: controlled git-CLI execution layer for the Tauri rewrite.
//!
//! All subprocess invocations use argument vectors (never shell strings),
//! bounded stdout, per-operation-class timeouts, and typed errors with stable
//! snake_case codes for the renderer.

mod git_binary;
mod platform_paths;
mod repo;
mod worktree;

pub use git_binary::{GitError, GitRunner, TimeoutClass};
pub use platform_paths::{
    WslPath, maybe_long_path, needs_long_path, normalize_drive_letter, to_wsl_text, windows_to_wsl,
    wsl_to_windows,
};
pub use repo::{CommitInfo, DiffFormat, PushMode, Repo};
pub use worktree::{CreatedWorktree, RemoveForce, WorktreeEntry, WorktreeService};
