//! Worktree lifecycle: create, list (porcelain), safe removal, prune,
//! lock/unlock, and repair — all via `git worktree` argument vectors.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::git_binary::{GitError, GitRunner, TimeoutClass};
use crate::repo::Repo;

/// One worktree from `git worktree list --porcelain`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeEntry {
    /// Absolute path of the worktree (or bare repo).
    pub path: String,
    /// Resolved HEAD commit, when present.
    pub head: Option<String>,
    /// Branch ref (`refs/heads/main`), `detached`, or None for bare.
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    /// Locked with an optional reason (`locked <reason>`).
    pub locked_reason: Option<String>,
    /// Prunable with an optional reason (`prunable <reason>`, e.g. a gitdir
    /// whose directory no longer exists).
    pub prunable_reason: Option<String>,
}

/// Result of creating a worktree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedWorktree {
    pub path: PathBuf,
    pub branch: String,
    pub head: String,
}

/// Removal policy for `remove`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoveForce {
    /// Refuse when the worktree is dirty or locked.
    Never,
    /// `--force`: allows removing dirty/locked worktrees.
    Yes,
}

/// Worktree lifecycle operations bound to a main repository.
pub struct WorktreeService<'a> {
    repo: &'a Repo,
}

impl<'a> WorktreeService<'a> {
    pub fn new(repo: &'a Repo) -> Self {
        Self { repo }
    }

    fn runner(&self) -> Result<&'static GitRunner, GitError> {
        GitRunner::shared()
    }

    fn run(&self, class: TimeoutClass, args: &[&OsStr]) -> Result<Vec<u8>, GitError> {
        self.runner()?.run(&self.repo.root, class.duration(), args)
    }

    /// Create a worktree at `path`, optionally with a new branch `-b branch`
    /// and a base ref. The path must not exist or must be an empty directory.
    pub fn create(
        &self,
        path: &Path,
        branch: Option<&str>,
        base_ref: Option<&str>,
    ) -> Result<CreatedWorktree, GitError> {
        self.runner()?.require_worktree_support()?;

        // Safety: refuse existing non-empty destinations.
        match std::fs::read_dir(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(GitError::GitOperationFailed {
                    command: format!("stat {}", path.display()),
                    stderr_redacted: e.to_string(),
                });
            }
            Ok(mut entries) => {
                if entries.next().is_some() {
                    return Err(GitError::GitOperationFailed {
                        command: "worktree add".to_string(),
                        stderr_redacted: "<path exists and is not empty>".to_string(),
                    });
                }
            }
        }

        let mut args: Vec<&OsStr> = vec![OsStr::new("worktree"), OsStr::new("add")];
        let branch_flag = branch.map(|_| "-b".to_string());
        if let Some(flag) = &branch_flag {
            args.push(OsStr::new(flag));
        }
        let branch_arg = branch.map(OsStr::new);
        if let Some(b) = branch_arg {
            args.push(b);
        }
        args.push(path.as_os_str());
        if let Some(base) = base_ref {
            args.push(OsStr::new(base));
        }
        self.run(TimeoutClass::Mutate, &args)?;

        // Resolve created HEAD from the new worktree itself.
        let head = self.resolve_head(path)?;
        let branch_name = branch
            .map(str::to_string)
            .unwrap_or_else(|| infer_branch_name(path));
        Ok(CreatedWorktree {
            path: path.to_path_buf(),
            branch: branch_name,
            head,
        })
    }

    /// List all worktrees via porcelain output.
    pub fn list(&self) -> Result<Vec<WorktreeEntry>, GitError> {
        self.runner()?.require_worktree_support()?;
        let out = self.run(
            TimeoutClass::Fast,
            &[
                OsStr::new("worktree"),
                OsStr::new("list"),
                OsStr::new("--porcelain"),
            ],
        )?;
        parse_worktree_porcelain(&out)
    }

    /// Remove a worktree with safety gates:
    /// - refuse the main worktree (it IS the repository),
    /// - refuse bare worktrees,
    /// - refuse dirty worktrees unless `force` (dirty = porcelain entries),
    /// - refuse locked worktrees unless `force`.
    pub fn remove(&self, path: &Path, force: RemoveForce) -> Result<(), GitError> {
        self.runner()?.require_worktree_support()?;

        let entries = self.list()?;
        let target = entries
            .iter()
            .find(|e| same_path(&e.path, path))
            .ok_or_else(|| GitError::GitOperationFailed {
                command: "worktree remove".to_string(),
                stderr_redacted: "<path is not a registered worktree>".to_string(),
            })?;

        if target.bare {
            return Err(GitError::GitOperationFailed {
                command: "worktree remove".to_string(),
                stderr_redacted: "<cannot remove a bare repository>".to_string(),
            });
        }

        // The main worktree is the one whose gitdir is the repo's own .git.
        let main_path = self.repo.root.to_string_lossy().replace('\\', "/");
        if target.path.replace('\\', "/") == main_path {
            return Err(GitError::GitOperationFailed {
                command: "worktree remove".to_string(),
                stderr_redacted: "<cannot remove the main worktree>".to_string(),
            });
        }

        if force == RemoveForce::Never {
            if target.locked_reason.is_some() {
                return Err(GitError::GitOperationFailed {
                    command: "worktree remove".to_string(),
                    stderr_redacted: "<worktree is locked>".to_string(),
                });
            }
            // Dirty check: status inside the target worktree.
            let target_repo = Repo::discover(path)?;
            let status = target_repo.status(false)?;
            if !status.is_clean() {
                return Err(GitError::GitOperationFailed {
                    command: "worktree remove".to_string(),
                    stderr_redacted: "<worktree contains modified or untracked files>".to_string(),
                });
            }
        }

        let mut args: Vec<&OsStr> = vec![OsStr::new("worktree"), OsStr::new("remove")];
        if force == RemoveForce::Yes {
            args.push(OsStr::new("--force"));
        }
        args.push(path.as_os_str());
        self.run(TimeoutClass::Mutate, &args).map(|_| ())
    }

    /// Lock a worktree (optional reason).
    pub fn lock(&self, path: &Path, reason: Option<&str>) -> Result<(), GitError> {
        let mut args: Vec<&OsStr> = vec![OsStr::new("worktree"), OsStr::new("lock")];
        if let Some(r) = reason {
            args.push(OsStr::new("--reason"));
            args.push(OsStr::new(r));
        }
        args.push(path.as_os_str());
        self.run(TimeoutClass::Fast, &args).map(|_| ())
    }

    /// Unlock a worktree.
    pub fn unlock(&self, path: &Path) -> Result<(), GitError> {
        self.run(
            TimeoutClass::Fast,
            &[
                OsStr::new("worktree"),
                OsStr::new("unlock"),
                path.as_os_str(),
            ],
        )
        .map(|_| ())
    }

    /// Prune stale worktree admin entries; git's `-v` report goes to stderr,
    /// so we derive the report from the stale-entry snapshot before pruning.
    pub fn prune(&self) -> Result<Option<String>, GitError> {
        self.runner()?.require_worktree_support()?;

        // Snapshot the stale entries first (stderr capture is unavailable on
        // success in `run`, so we derive the report from the entry delta).
        let before = self.list()?;
        let stale_before: Vec<String> = before
            .iter()
            .filter_map(|e| e.prunable_reason.clone().map(|_| e.path.clone()))
            .collect();

        self.run(
            TimeoutClass::Fast,
            &[
                OsStr::new("worktree"),
                OsStr::new("prune"),
                OsStr::new("-v"),
            ],
        )?;

        if stale_before.is_empty() {
            return Ok(None);
        }
        let report = stale_before
            .iter()
            .map(|p| format!("Removing worktrees/{p}"))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Some(report))
    }
    pub fn repair(&self) -> Result<(), GitError> {
        self.runner()?.require_worktree_support()?;
        self.run(
            TimeoutClass::Fast,
            &[OsStr::new("worktree"), OsStr::new("repair")],
        )
        .map(|_| ())
    }

    /// Orphan-gitdir proof: list worktrees whose admin gitdir points at a
    /// directory that no longer exists (the `prunable` porcelain reason).
    /// These are exactly the entries a subsequent `prune` will remove.
    pub fn orphan_gitdirs(&self) -> Result<Vec<String>, GitError> {
        let entries = self.list()?;
        Ok(entries
            .into_iter()
            .filter_map(|e| {
                e.prunable_reason.map(|reason| {
                    // Both known prunable reasons ("gitdir file points to
                    // non-existent location" and "missing") surface the same
                    // orphaned worktree path for pruning.
                    let _ = reason;
                    e.path
                })
            })
            .collect())
    }

    fn resolve_head(&self, worktree_path: &Path) -> Result<String, GitError> {
        let runner = self.runner()?;
        let out = runner.run(
            worktree_path,
            TimeoutClass::Fast.duration(),
            &[OsStr::new("rev-parse"), OsStr::new("HEAD")],
        )?;
        Ok(String::from_utf8_lossy(&out).trim().to_string())
    }
}

/// Parse `git worktree list --porcelain` output into entries.
pub fn parse_worktree_porcelain(raw: &[u8]) -> Result<Vec<WorktreeEntry>, GitError> {
    let text = String::from_utf8_lossy(raw);
    let mut entries = Vec::new();
    let mut current = WorktreeEntry::default();
    let mut have_current = false;
    for line in text.lines() {
        if line.is_empty() {
            if have_current {
                entries.push(std::mem::take(&mut current));
                have_current = false;
            }
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => {
                if have_current {
                    entries.push(std::mem::take(&mut current));
                }
                current = WorktreeEntry {
                    path: value.to_string(),
                    ..WorktreeEntry::default()
                };
                have_current = true;
            }
            "HEAD" => current.head = Some(value.to_string()),
            "branch" => current.branch = Some(value.to_string()),
            "bare" => current.bare = true,
            "detached" => current.detached = true,
            "locked" => {
                current.locked_reason = if value.is_empty() {
                    Some(String::new())
                } else {
                    Some(value.to_string())
                }
            }
            "prunable" => {
                current.prunable_reason = if value.is_empty() {
                    Some(String::new())
                } else {
                    Some(value.to_string())
                }
            }
            _ => {}
        }
    }
    if have_current {
        entries.push(current);
    }
    Ok(entries)
}

fn same_path(a: &str, b: &Path) -> bool {
    let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_string();
    norm(a) == norm(&b.to_string_lossy())
}

fn infer_branch_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_basic_list() {
        let raw = b"worktree /repo\nHEAD abc123\nbranch refs/heads/main\n\n\
worktree /wt1\nHEAD def456\ndetached\n\n";
        let entries = parse_worktree_porcelain(raw).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "/repo");
        assert_eq!(entries[0].head.as_deref(), Some("abc123"));
        assert_eq!(entries[0].branch.as_deref(), Some("refs/heads/main"));
        assert!(!entries[0].detached);
        assert_eq!(entries[1].path, "/wt1");
        assert!(entries[1].detached);
        assert!(entries[1].branch.is_none());
    }

    #[test]
    fn parses_porcelain_bare_locked_prunable() {
        let raw = b"worktree /bare.git\nbare\n\n\
worktree /wt2\nHEAD aaa\nbranch refs/heads/feature\nlocked why not\n\n\
worktree /wt3\nHEAD bbb\nbranch refs/heads/x\nprunable gitdir file points to non-existent location\n\n";
        let entries = parse_worktree_porcelain(raw).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].bare);
        assert!(!entries[0].detached);
        assert_eq!(entries[1].locked_reason.as_deref(), Some("why not"));
        assert!(
            entries[2]
                .prunable_reason
                .as_deref()
                .unwrap()
                .contains("non-existent")
        );
    }

    #[test]
    fn parses_porcelain_locked_without_reason() {
        let raw = b"worktree /wt\nHEAD aaa\nbranch refs/heads/main\nlocked\n\n";
        let entries = parse_worktree_porcelain(raw).unwrap();
        assert_eq!(entries[0].locked_reason, Some(String::new()));
    }

    #[test]
    fn empty_porcelain_yields_no_entries() {
        assert!(parse_worktree_porcelain(b"").unwrap().is_empty());
    }

    #[test]
    fn same_path_normalizes_separators() {
        assert!(same_path(r"D:\repo\wt", Path::new("D:/repo/wt")));
        assert!(same_path("D:/repo/wt/", Path::new(r"D:\repo\wt")));
        assert!(!same_path("D:/repo/wt", Path::new("D:/other")));
    }
}
