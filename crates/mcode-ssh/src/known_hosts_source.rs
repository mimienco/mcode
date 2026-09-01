//! Port of `ssh-known-hosts-source.ts`'s fallback ladder
//! (`defaultKnownHostsFiles`): the user's real OpenSSH host files. Reading
//! them is the entire migration story — most developers already verified
//! their hosts through `ssh` and `git`. Read-only (decision D1): writing to a
//! file shared with every other SSH tool on the machine is out of scope;
//! accepted keys land in the MCode store instead.
//!
//! v1 scope: this is the fallback used when no `ssh -G` config resolution has
//! run. The original treats an empty `ssh -G` report as "fall back rather
//! than read nothing" (`ssh-known-hosts-source.ts:26-31`); this module is
//! that fallback.

use std::path::{Path, PathBuf};

/// The user's `known_hosts` files in OpenSSH's per-user order. `None` home
/// resolution yields an empty list — "no trust source", which the verifier
/// then reports as first contact rather than a fabricated known-host state.
pub fn default_known_hosts_files() -> Vec<PathBuf> {
    default_known_hosts_files_in(home_dir().as_deref())
}

/// Pure core of [`default_known_hosts_files`], testable without process env
/// mutation (which is `unsafe` under edition 2024 and racy across tests).
pub(crate) fn default_known_hosts_files_in(home: Option<&Path>) -> Vec<PathBuf> {
    let Some(home) = home else {
        return Vec::new();
    };
    vec![
        home.join(".ssh").join("known_hosts"),
        home.join(".ssh").join("known_hosts2"),
    ]
}

/// `os.homedir()` semantics: `USERPROFILE` on Windows, `HOME` elsewhere. An
/// empty value is treated as unset — a literal empty path would join to
/// `.ssh/known_hosts` relative to the cwd.
fn home_dir() -> Option<PathBuf> {
    if let Some(profile) = std::env::var_os("USERPROFILE")
        && !profile.is_empty()
    {
        return Some(PathBuf::from(profile));
    }
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yields_both_user_files_in_openssh_order() {
        let files = default_known_hosts_files_in(Some(Path::new("/home/dev")));
        assert_eq!(
            files,
            vec![
                PathBuf::from("/home/dev/.ssh/known_hosts"),
                PathBuf::from("/home/dev/.ssh/known_hosts2")
            ]
        );
    }

    #[test]
    fn no_home_yields_no_trust_source() {
        assert!(default_known_hosts_files_in(None).is_empty());
    }
}
