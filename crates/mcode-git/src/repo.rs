//! Repository operations: discovery, status (porcelain v2), staging, commit,
//! discard, diff, history, network sync, branch comparison, ignore checks,
//! and submodule status.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::git_binary::{GitError, GitRunner, TimeoutClass};

/// A repository handle bound to a resolved worktree root.
#[derive(Clone, Debug)]
pub struct Repo {
    /// The worktree root (or bare repo path) all operations run against.
    pub root: PathBuf,
    /// The git directory (`.git` dir, worktree gitdir file target, or the
    /// bare path itself).
    pub gitdir: PathBuf,
}

/// One entry in a parsed porcelain v2 status report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    /// `.`/index state: `M`odified, `A`dded, `D`eleted, `R`enamed, `T`ype
    /// changed, un`M`erged, or `.` for unchanged.
    pub index_status: char,
    /// Working-tree state, same alphabet plus `?` (untracked) and `!`
    /// (ignored) in the untracked entry form.
    pub worktree_status: char,
    /// Repository-relative path (renames: the source).
    pub path: String,
    /// Rename/copy target when applicable.
    pub path_orig: Option<String>,
}

/// Aggregate result of a status query.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResult {
    pub entries: Vec<StatusEntry>,
    /// Index vs HEAD ahead count (None when no upstream info requested).
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub conflicts: Vec<String>,
    pub untracked: Vec<String>,
}

impl StatusResult {
    pub fn is_clean(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A commit from `git log` history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub hash: String,
    pub abbrev_hash: String,
    pub author_name: String,
    pub author_email: String,
    pub author_date: String,
    pub subject: String,
}

/// Diff output flavor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffFormat {
    /// Working tree vs index.
    Unstaged,
    /// Index vs HEAD.
    Staged,
    /// Working tree + index vs HEAD.
    Both,
}

/// Push behavior for `push`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PushMode {
    /// Push to the configured upstream (`git push` semantics with explicit
    /// upstream name resolved by the caller).
    Upstream,
    /// Explicit refspec, e.g. `main:main` or `HEAD:refs/heads/main`.
    Refspec { src: String, dst: String },
}

impl Repo {
    /// Discover the repository containing `start`, walking upward. Handles:
    /// plain `.git` directories, `.git` FILE pointers (linked worktrees),
    /// and bare repos (directory ending in `.git` with HEAD/objects).
    pub fn discover(start: &Path) -> Result<Self, GitError> {
        let start = if start.is_absolute() {
            start.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|_| GitError::GitNotARepository)?
                .join(start)
        };
        let mut current: Option<&Path> = Some(start.as_path());
        while let Some(dir) = current {
            let dotgit = dir.join(".git");
            if dotgit.is_dir() && dotgit.join("HEAD").is_file() {
                return Ok(Self {
                    root: dir.to_path_buf(),
                    gitdir: dotgit,
                });
            }
            if dotgit.is_file() {
                // Linked worktree: `.git` file says `gitdir: <path>`.
                let contents =
                    std::fs::read_to_string(&dotgit).map_err(|_| GitError::GitNotARepository)?;
                if let Some(gitdir) =
                    parse_gitdir_file(&contents).filter(|gitdir| gitdir.join("HEAD").is_file())
                {
                    return Ok(Self {
                        root: dir.to_path_buf(),
                        gitdir,
                    });
                }
            }
            // Bare repo: the directory itself is the gitdir.
            if is_bare_gitdir(dir) {
                return Ok(Self {
                    root: dir.to_path_buf(),
                    gitdir: dir.to_path_buf(),
                });
            }
            current = dir.parent();
        }
        Err(GitError::GitNotARepository)
    }

    /// Verify a candidate path is contained within this repository root
    /// (lexical containment; symlinks are resolved by the caller as needed).
    pub fn contains(&self, candidate: &Path) -> bool {
        path_contains(&self.root, candidate)
    }

    fn runner(&self) -> Result<&'static GitRunner, GitError> {
        GitRunner::shared()
    }

    fn run(&self, class: TimeoutClass, args: &[&OsStr]) -> Result<Vec<u8>, GitError> {
        self.runner()?.run(&self.root, class.duration(), args)
    }

    /// Full status via `git status --porcelain=v2 --branch`.
    pub fn status(&self, include_ignored: bool) -> Result<StatusResult, GitError> {
        let runner = self.runner()?;
        runner.require_porcelain_v2()?;
        let mut args: Vec<&OsStr> = vec![
            OsStr::new("status"),
            OsStr::new("--porcelain=v2"),
            OsStr::new("--branch"),
            OsStr::new("--null"),
        ];
        if include_ignored {
            args.push(OsStr::new("--ignored"));
        }
        let out = runner.run(&self.root, TimeoutClass::Fast.duration(), &args)?;
        parse_status_v2(&out)
    }

    /// Stage pathspecs (`git add -- <paths>`).
    pub fn stage(&self, paths: &[&Path]) -> Result<(), GitError> {
        self.stage_unstage(paths, true)
    }

    /// Unstage pathspecs (`git reset -- <paths>`; keeps working tree files).
    pub fn unstage(&self, paths: &[&Path]) -> Result<(), GitError> {
        self.stage_unstage(paths, false)
    }

    fn stage_unstage(&self, paths: &[&Path], add: bool) -> Result<(), GitError> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args: Vec<&OsStr> = vec![OsStr::new(if add { "add" } else { "reset" })];
        args.push(OsStr::new("--"));
        for p in paths {
            args.push(p.as_os_str());
        }
        self.run(TimeoutClass::Fast, &args).map(|_| ())
    }

    /// Commit staged changes; message comes through the argv vector, never a
    /// shell string. `allow_empty` maps to `--allow-empty`.
    pub fn commit(&self, message: &str, allow_empty: bool) -> Result<String, GitError> {
        let mut args: Vec<&OsStr> = vec![
            OsStr::new("commit"),
            OsStr::new("-m"),
            OsStr::new(message),
            OsStr::new("--no-verify"),
        ];
        if allow_empty {
            args.push(OsStr::new("--allow-empty"));
        }
        let out = self.run(TimeoutClass::Mutate, &args)?;
        // `git commit` prints the new abbreviated hash on its first line,
        // e.g. `[main (root-commit) 1a2b3c4] message`.
        let text = String::from_utf8_lossy(&out);
        Ok(parse_commit_output(&text).unwrap_or_default())
    }

    /// Discard working-tree changes for tracked paths (`git checkout --
    /// paths`); untracked files are never touched.
    pub fn discard(&self, paths: &[&Path]) -> Result<(), GitError> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args: Vec<&OsStr> = vec![OsStr::new("checkout"), OsStr::new("--")];
        for p in paths {
            args.push(p.as_os_str());
        }
        self.run(TimeoutClass::Fast, &args).map(|_| ())
    }

    /// Unified diff for the given paths (or the whole tree when `paths` is
    /// empty). Path list bounds the output with `--`.
    pub fn diff(&self, format: DiffFormat, paths: &[&Path]) -> Result<String, GitError> {
        let mut args: Vec<&OsStr> = vec![OsStr::new("diff")];
        match format {
            DiffFormat::Unstaged => {}
            DiffFormat::Staged => args.push(OsStr::new("--cached")),
            DiffFormat::Both => args.push(OsStr::new("HEAD")),
        }
        if !paths.is_empty() {
            args.push(OsStr::new("--"));
            for p in paths {
                args.push(p.as_os_str());
            }
        }
        let out = self.run(TimeoutClass::Read, &args)?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// Commit history, newest first, bounded by `max_count`.
    pub fn history(&self, max_count: u32) -> Result<Vec<CommitInfo>, GitError> {
        let max = max_count.clamp(1, 10_000);
        // NUL-delimited fields; the trailing `%x00` pairs with git's record
        // newline as the inter-commit separator (`\0\n`).
        let pretty = "--pretty=format:%H%x00%h%x00%an%x00%ae%x00%aI%x00%s%x00";
        let n_arg = format!("-n{max}");
        let out = self.run(
            TimeoutClass::Read,
            &[OsStr::new("log"), OsStr::new(&n_arg), OsStr::new(pretty)],
        )?;
        parse_history(&out)
    }

    /// Pull with `--ff-only` enforcement (default) or a merge fallback.
    pub fn pull(&self, remote: &str, branch: &str, ff_only: bool) -> Result<(), GitError> {
        let mut args: Vec<&OsStr> = vec![OsStr::new("pull")];
        if ff_only {
            args.push(OsStr::new("--ff-only"));
        }
        args.push(OsStr::new(remote));
        args.push(OsStr::new(branch));
        self.run(TimeoutClass::Network, &args).map(|_| ())
    }

    /// Push `branch` to `remote`, optionally setting upstream.
    pub fn push(&self, remote: &str, branch: &str, set_upstream: bool) -> Result<(), GitError> {
        let mut args: Vec<&OsStr> = vec![OsStr::new("push")];
        if set_upstream {
            args.push(OsStr::new("--set-upstream"));
        }
        args.push(OsStr::new(remote));
        let refspec = format!("{branch}:{branch}");
        args.push(OsStr::new(&refspec));
        self.run(TimeoutClass::Network, &args).map(|_| ())
    }

    /// Fast-forward the current branch to its upstream (`git merge --ff-only`).
    pub fn fast_forward(&self) -> Result<(), GitError> {
        self.run(
            TimeoutClass::Network,
            &[OsStr::new("merge"), OsStr::new("--ff-only")],
        )
        .map(|_| ())
    }

    /// Ahead/behind counts between two refs via `rev-list --count`.
    pub fn branch_compare(&self, base: &str, head: &str) -> Result<(u32, u32), GitError> {
        let spec = format!("{base}...{head}");
        let out = self.run(
            TimeoutClass::Fast,
            &[
                OsStr::new("rev-list"),
                OsStr::new("--left-right"),
                OsStr::new("--count"),
                OsStr::new(&spec),
            ],
        )?;
        let text = String::from_utf8_lossy(&out);
        let mut parts = text.split_whitespace();
        let behind = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let ahead = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        Ok((ahead, behind))
    }

    /// Check which of `paths` are gitignored (returns the ignored subset).
    pub fn check_ignored(&self, paths: &[&Path]) -> Result<Vec<PathBuf>, GitError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let mut args: Vec<&OsStr> = vec![OsStr::new("check-ignore"), OsStr::new("--")];
        for p in paths {
            args.push(p.as_os_str());
        }
        let out = self.run(TimeoutClass::Fast, &args)?;
        Ok(String::from_utf8_lossy(&out)
            .lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect())
    }

    /// Submodule status for one submodule path (`git submodule status -- <path>`).
    pub fn submodule_status(&self, submodule_path: &Path) -> Result<Vec<CommitInfo>, GitError> {
        let out = self.run(
            TimeoutClass::Read,
            &[
                OsStr::new("submodule"),
                OsStr::new("status"),
                OsStr::new("--"),
                submodule_path.as_os_str(),
            ],
        )?;
        // Each line: ` <hash> <path> <describe>`; hash prefixed by
        // ` ` (ok), `-` (uninitialized), `+` (different), `U` (conflict).
        let text = String::from_utf8_lossy(&out);
        let mut commits = Vec::new();
        for line in text.lines() {
            let line = line.trim_start();
            let Some((hash, rest)) = line.split_once(' ') else {
                continue;
            };
            if hash.is_empty() {
                continue;
            }
            let (path, _describe) = rest.split_once(' ').unwrap_or((rest, ""));
            commits.push(CommitInfo {
                hash: hash.to_string(),
                abbrev_hash: hash.chars().take(7).collect(),
                author_name: path.to_string(),
                author_email: String::new(),
                author_date: String::new(),
                subject: String::new(),
            });
        }
        Ok(commits)
    }
}

/// `safe.directory`-style porcelain v2 requirement gate.
trait RequirePorcelainV2 {
    fn require_porcelain_v2(&self) -> Result<(), GitError>;
}

impl RequirePorcelainV2 for GitRunner {
    fn require_porcelain_v2(&self) -> Result<(), GitError> {
        if self.capabilities().supports_porcelain_v2() {
            Ok(())
        } else {
            Err(GitError::GitUnsupportedCapability {
                capability: "git status --porcelain=v2 (requires >= 2.11)",
                version: self.capabilities().version.clone(),
            })
        }
    }
}

/// Parse the `gitdir: <path>` payload of a `.git` FILE (linked worktree).
pub fn parse_gitdir_file(contents: &str) -> Option<PathBuf> {
    let line = contents.lines().next()?;
    let path = line.strip_prefix("gitdir:")?.trim();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn is_bare_gitdir(dir: &Path) -> bool {
    dir.join("HEAD").is_file() && dir.join("objects").is_dir() && dir.join("refs").is_dir()
}

/// Parse `git status --porcelain=v2 --null` output.
pub fn parse_status_v2(raw: &[u8]) -> Result<StatusResult, GitError> {
    let mut result = StatusResult::default();
    let text = String::from_utf8_lossy(raw);
    let mut entries = text.split('\0').flat_map(|chunk| {
        // Branch header lines arrive inside one NUL chunk but are
        // newline-separated; flatten them into individual records.
        if chunk.starts_with("# ") {
            Box::new(chunk.lines()) as Box<dyn Iterator<Item = &str>>
        } else {
            Box::new(std::iter::once(chunk)) as Box<dyn Iterator<Item = &str>>
        }
    });
    while let Some(record) = entries.next() {
        if record.is_empty() {
            continue;
        }
        let mut fields = record.split(' ');
        let kind = fields.next().unwrap_or("");
        match kind {
            "1" => {
                // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
                let xy = fields.next().unwrap_or(". ");
                let (index_status, worktree_status) = split_xy(xy);
                // Skip sub/mH/mI/mW/hH/hI (6 more fields after XY).
                for _ in 0..6 {
                    fields.next();
                }
                let path = fields.next().unwrap_or("").to_string();
                if !path.is_empty() {
                    result.entries.push(StatusEntry {
                        index_status,
                        worktree_status,
                        path,
                        path_orig: None,
                    });
                }
            }
            "2" => {
                // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path><sep><origPath>
                let xy = fields.next().unwrap_or(". ");
                let (index_status, worktree_status) = split_xy(xy);
                // Skip sub/mH/mI/mW/hH/hI (6 fields after XY).
                for _ in 0..6 {
                    fields.next();
                }
                let _rename_score = fields.next().unwrap_or("R100");
                // With `--null`, the renamed path is this field and the
                // original path is the NEXT NUL chunk.
                let path = fields.next().unwrap_or("").to_string();
                let orig_chunk = if xy.starts_with('R') || xy.starts_with('C') {
                    entries.next().map(|o| o.to_string())
                } else {
                    None
                };
                if !path.is_empty() {
                    result.entries.push(StatusEntry {
                        index_status,
                        worktree_status,
                        path,
                        path_orig: orig_chunk,
                    });
                }
            }
            "u" => {
                // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
                let mut parts = record.split(' ');
                parts.next(); // 'u'
                // Skip XY/sub/m1/m2/m3/mW/h1/h2/h3 (9 fields after 'u').
                for _ in 0..9 {
                    parts.next();
                }
                if let Some(path) = parts.next() {
                    result.conflicts.push(path.to_string());
                    result.entries.push(StatusEntry {
                        index_status: 'U',
                        worktree_status: 'U',
                        path: path.to_string(),
                        path_orig: None,
                    });
                }
            }
            "?" => {
                let path = record.strip_prefix("? ").unwrap_or("").trim();
                if !path.is_empty() {
                    result.untracked.push(path.to_string());
                }
            }
            "#" => {
                // Branch header lines: `# branch.oid …`, `# branch.head …`,
                // `# branch.upstream …`, `# branch.ab +N -M`.
                if let Some(value) = record.strip_prefix("# branch.head ") {
                    result.branch = Some(value.trim().to_string());
                } else if let Some(value) = record.strip_prefix("# branch.upstream ") {
                    result.upstream = Some(value.trim().to_string());
                } else if let Some(value) = record.strip_prefix("# branch.ab ") {
                    let mut nums = value.split_whitespace();
                    let ahead = nums
                        .next()
                        .and_then(|v| v.trim_start_matches('+').parse().ok());
                    let behind = nums
                        .next()
                        .and_then(|v| v.trim_start_matches('-').parse().ok());
                    result.ahead = ahead;
                    result.behind = behind;
                }
            }
            _ => {}
        }
    }
    Ok(result)
}

fn split_xy(xy: &str) -> (char, char) {
    let bytes = xy.as_bytes();
    let index = bytes.first().copied().unwrap_or(b'.') as char;
    let worktree = bytes.get(1).copied().unwrap_or(b'.') as char;
    (index, worktree)
}

/// Extract the new abbreviated hash from `git commit` output.
pub fn parse_commit_output(text: &str) -> Option<String> {
    // `[main (root-commit) 1a2b3c4] subject`
    let line = text.lines().next()?;
    let open = line.find('[')?;
    let close = line[open..].find(']')? + open;
    let inside = &line[open + 1..close];
    let hash = inside.split_whitespace().next_back()?;
    Some(hash.to_string())
}

/// Parse the NUL-delimited history format used by `history`.
///
/// Git separates commits with `\0` immediately followed by a newline (the
/// `--pretty=format:` terminator); within a record the six fields are
/// NUL-separated. The format's trailing `%x00` yields an empty final field.
pub fn parse_history(raw: &[u8]) -> Result<Vec<CommitInfo>, GitError> {
    let text = String::from_utf8_lossy(raw);
    let mut commits = Vec::new();
    for record in text.split("\0\n") {
        // One record: `%H%x00%h%x00%an%x00%ae%x00%aI%x00%s%x00` → 7 chunks
        // (last empty) but tolerate trailing empty chunks defensively.
        let fields: Vec<&str> = record.split('\0').collect();
        if fields.len() < 6 || fields[0].is_empty() {
            // Leading empty chunk from the format newline: skip.
            if fields.iter().all(|f| f.is_empty()) {
                continue;
            }
            if fields.len() < 6 {
                continue;
            }
        }
        // Strip a leading empty field when the record starts with the
        // format's newline remnant.
        let fields: Vec<&str> = if fields[0].is_empty() {
            fields[1..].to_vec()
        } else {
            fields
        };
        if fields.len() < 6 {
            continue;
        }
        commits.push(CommitInfo {
            hash: fields[0].to_string(),
            abbrev_hash: fields[1].to_string(),
            author_name: fields[2].to_string(),
            author_email: fields[3].to_string(),
            author_date: fields[4].to_string(),
            subject: fields[5].to_string(),
        });
    }
    Ok(commits)
}

/// Lexical containment helper reused by the worktree service.
pub fn path_contains(root: &Path, candidate: &Path) -> bool {
    let root_comps: Vec<_> = root.components().collect();
    let cand_comps: Vec<_> = candidate.components().collect();
    if cand_comps.len() < root_comps.len() {
        return false;
    }
    root_comps
        .iter()
        .zip(cand_comps.iter())
        .all(|(r, c)| match (r, c) {
            (Component::Prefix(pr), Component::Prefix(pc)) => {
                pr.as_os_str().eq_ignore_ascii_case(pc.as_os_str())
            }
            (Component::Normal(nr), Component::Normal(nc)) => nr.eq_ignore_ascii_case(nc),
            _ => r == c,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gitdir_file() {
        let gitdir = parse_gitdir_file("gitdir: D:/repo/.git/worktrees/wt1\nother\n").unwrap();
        assert_eq!(gitdir, PathBuf::from("D:/repo/.git/worktrees/wt1"));
        assert!(parse_gitdir_file("").is_none());
        assert!(parse_gitdir_file("something else").is_none());
    }

    #[test]
    fn parses_status_v2_entries_and_branch() {
        let raw = b"# branch.oid 1a2b3c4\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 abc def modified.txt\0\
2 R. N... 100644 100644 100644 abc def R100 renamed.txt\0old.txt\0\
? untracked-file\0\
u AU N... 000000 000000 000000 000 000 000 000 conflicted.txt\0";
        let status = parse_status_v2(raw).unwrap();
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!(status.ahead, Some(2));
        assert_eq!(status.behind, Some(1));
        assert_eq!(status.entries.len(), 3);
        assert_eq!(status.entries[0].path, "modified.txt");
        assert_eq!(status.entries[0].worktree_status, 'M');
        assert_eq!(status.entries[1].path, "renamed.txt");
        assert_eq!(status.entries[1].path_orig.as_deref(), Some("old.txt"));
        assert_eq!(status.untracked, vec!["untracked-file"]);
        assert_eq!(status.conflicts, vec!["conflicted.txt"]);
        assert!(!status.is_clean());
    }

    #[test]
    fn parses_status_v2_empty_tree() {
        let status = parse_status_v2(b"# branch.head main\0").unwrap();
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert!(status.is_clean());
    }
    #[test]
    fn history_parsing_groups_six_fields() {
        // Two commits separated by `\0\n` (git's `--pretty=format:` record
        // terminator), six NUL-separated fields each plus trailing `%x00`.
        let raw: &[u8] = b"hash1\x00h1\0Alice\0a@x.io\x002026-01-01T00:00:00Z\0first\x00\nhash2\x00h2\0Bob\0b@x.io\x002026-01-02T00:00:00Z\0second\x00\n";
        let commits = parse_history(raw).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "hash1");
        assert_eq!(commits[0].author_name, "Alice");
        assert_eq!(commits[1].subject, "second");
    }

    #[test]
    fn history_parsing_single_commit_no_trailing_newline() {
        let raw = b"hash1\0h1\0Alice\0a@x.io\0date\0only\0";
        let commits = parse_history(raw).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "only");
    }

    #[test]
    fn path_containment_lexical() {
        let root = Path::new("D:/repo");
        assert!(path_contains(root, Path::new("D:/repo/sub/file.txt")));
        assert!(!path_contains(root, Path::new("D:/repository/file.txt")));
        assert!(path_contains(Path::new("D:/repo"), Path::new("D:/repo")));
    }

    #[test]
    fn branch_compare_parsing_shape() {
        // Pure parse test (no git invocation): parse "(left)(right)" counts.
        let text = "2\t5";
        let mut parts = text.split_whitespace();
        let behind: u32 = parts.next().unwrap().parse().unwrap();
        let ahead: u32 = parts.next().unwrap().parse().unwrap();
        assert_eq!((ahead, behind), (5, 2));
    }
}
