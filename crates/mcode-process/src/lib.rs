//! Process supervision: spawn, environment policy, registry, stop ladder.
//!
//! The supervisor owns every child it creates through a stable
//! [`ProcessHandle`]; OS pids are never authority. See `handle.rs` for the
//! spec/lifecycle vocabulary and `env.rs` for the load-bearing scrub list.

pub mod descendants;
pub mod env;
pub mod handle;
pub mod job;
pub mod ladder;
pub mod supervisor;

/// Raw pid/pgid type per platform (`libc::pid_t` on POSIX, `i32` on Windows).
#[cfg(unix)]
pub type Pgid = libc::pid_t;
#[cfg(not(unix))]
pub type Pgid = i32;

pub use descendants::list_descendants;
pub use env::{apply_environment, current_environment};
pub use handle::{
    EnvironmentPolicy, ExitCause, ExitObservation, ProcessHandle, ProcessSpec, ProcessState,
    SpecError,
};
pub use job::JobObject;
pub use supervisor::{
    ProcessEntry, ProcessSnapshot, ProcessSupervisor, Spawned, SupervisorConfig, SupervisorError,
};
