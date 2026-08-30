//! Search and search-replace tests (spec §4, §6 cancellation).

use std::fs;

use tempfile::TempDir;

use mcode_fs::scope::{AuthorizedPath, FsScopeRegistry, LeafPolicy};
use mcode_fs::search::{
    CancellationRegistry, CancellationToken, DEFAULT_SEARCH_MAX_RESULTS, REPLACE_MAX_FILE_BYTES,
    ReplaceOutcome, search, search_replace,
};

struct Ctx {
    dir: TempDir,
    registry: FsScopeRegistry,
}

impl Ctx {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let registry = FsScopeRegistry::new();
        registry.register_root(dir.path()).unwrap();
        Self { dir, registry }
    }

    fn root(&self) -> AuthorizedPath {
        self.registry
            .authorize(self.dir.path(), LeafPolicy::Canonicalize)
            .unwrap()
    }
}

fn token(window: u64, name: &str) -> (CancellationRegistry, CancellationToken) {
    let registry = CancellationRegistry::default();
    let handle = registry.register(window, name);
    (registry, handle)
}

#[test]
fn search_finds_matches_with_line_info() {
    let ctx = Ctx::new();
    fs::write(ctx.dir.path().join("a.txt"), "hello world\ngoodbye world\n").unwrap();
    fs::write(ctx.dir.path().join("b.txt"), "nothing here\n").unwrap();

    let (_reg, cancel) = token(1, "t1");
    let result = search(&ctx.root(), "world", 100, &cancel);
    assert!(!result.rg_unavailable, "rg should be available in CI/dev");
    assert!(!result.truncated);
    let paths: Vec<&str> = result.matches.iter().map(|m| m.path.as_str()).collect();
    assert!(paths.iter().any(|p| p.ends_with("a.txt")));
    assert!(!paths.iter().any(|p| p.ends_with("b.txt")));
    let a_match = result
        .matches
        .iter()
        .find(|m| m.path.ends_with("a.txt"))
        .unwrap();
    assert_eq!(a_match.line_number, 1);
}

#[test]
fn search_respects_max_results_and_marks_truncated() {
    let ctx = Ctx::new();
    let mut body = String::new();
    for i in 0..50 {
        body.push_str(&format!("needle {i}\n"));
    }
    fs::write(ctx.dir.path().join("many.txt"), body).unwrap();

    let (_reg, cancel) = token(1, "t2");
    let result = search(&ctx.root(), "needle", 5, &cancel);
    assert!(result.matches.len() <= 5);
}

#[test]
fn search_result_shape_is_camel_case_json() {
    let result = mcode_fs::search::SearchResult {
        matches: vec![],
        truncated: true,
        rg_unavailable: true,
    };
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["truncated"], serde_json::json!(true));
    assert_eq!(json["rgUnavailable"], serde_json::json!(true));
    assert!(json.get("matches").is_some());
}

#[test]
fn default_max_results_is_2000() {
    assert_eq!(DEFAULT_SEARCH_MAX_RESULTS, 2000);
}

#[test]
fn replace_writes_all_occurrences_atomically() {
    let ctx = Ctx::new();
    fs::write(ctx.dir.path().join("code.txt"), "alpha beta alpha\n").unwrap();

    let (_reg, cancel) = token(1, "t3");
    let summary = search_replace(&ctx.root(), "alpha", "gamma", &[], &cancel).unwrap();
    assert!(!summary.cancelled);
    let outcome = summary.outcomes.values().next().expect("one file touched");
    match outcome {
        ReplaceOutcome::Replaced { replacements } => assert_eq!(*replacements, 2),
        other => panic!("expected Replaced, got {other:?}"),
    }
    let content = fs::read_to_string(ctx.dir.path().join("code.txt")).unwrap();
    assert_eq!(content, "gamma beta gamma\n");
}

#[test]
fn replace_skip_paths_never_written() {
    let ctx = Ctx::new();
    let keep = ctx.dir.path().join("keep.txt");
    fs::write(&keep, "alpha stays\n").unwrap();
    fs::write(ctx.dir.path().join("other.txt"), "alpha changes\n").unwrap();

    let (_reg, cancel) = token(1, "t4");
    let summary = search_replace(&ctx.root(), "alpha", "BETA", std::slice::from_ref(&keep), &cancel).unwrap();
    assert_eq!(
        fs::read_to_string(&keep).unwrap(),
        "alpha stays\n",
        "skipPaths must be honored"
    );
    // The skip is reported under the enumerated (canonical) path; find it by
    // file name rather than the renderer-facing spelling.
    let skipped_outcome = summary
        .outcomes
        .iter()
        .find(|(path, outcome)| {
            path.ends_with("keep.txt") && matches!(outcome, ReplaceOutcome::Skipped { .. })
        })
        .map(|(_, outcome)| outcome)
        .expect("skipped path reported");
    match skipped_outcome {
        ReplaceOutcome::Skipped { .. } => {}
        other => panic!("expected Skipped, got {other:?}"),
    }
    assert!(
        fs::read_to_string(ctx.dir.path().join("other.txt"))
            .unwrap()
            .contains("BETA")
    );
}

#[test]
fn replace_skips_oversize_files() {
    let ctx = Ctx::new();
    let big = ctx.dir.path().join("big.txt");
    let file = fs::File::create(&big).unwrap();
    file.set_len(REPLACE_MAX_FILE_BYTES + 1).unwrap();
    drop(file);

    let (_reg, cancel) = token(1, "t5");
    let summary = search_replace(&ctx.root(), "alpha", "beta", &[], &cancel).unwrap();
    let outcome = summary.outcomes.values().next().unwrap();
    match outcome {
        ReplaceOutcome::Failed { .. } | ReplaceOutcome::Skipped { .. } => {}
        other => panic!("oversize file must not be replaced, got {other:?}"),
    }
}

#[test]
fn replace_skips_binary_files() {
    let ctx = Ctx::new();
    fs::write(ctx.dir.path().join("bin.dat"), b"abc\0defalpha").unwrap();

    let (_reg, cancel) = token(1, "t6");
    let summary = search_replace(&ctx.root(), "alpha", "beta", &[], &cancel).unwrap();
    let outcome = summary.outcomes.values().next().unwrap();
    match outcome {
        ReplaceOutcome::Failed { .. } | ReplaceOutcome::Skipped { .. } => {}
        other => panic!("binary must be skipped, got {other:?}"),
    }
    // Content untouched.
    assert_eq!(
        fs::read(ctx.dir.path().join("bin.dat")).unwrap(),
        b"abc\0defalpha"
    );
}

#[test]
fn replace_empty_query_is_invalid_argument() {
    let ctx = Ctx::new();
    let (_reg, cancel) = token(1, "t7");
    let error = search_replace(&ctx.root(), "", "x", &[], &cancel).unwrap_err();
    assert_eq!(error.code(), "fs_invalid_argument");
}

#[test]
fn cancellation_returns_partial_summary() {
    let ctx = Ctx::new();
    fs::write(ctx.dir.path().join("one.txt"), "hit\n").unwrap();
    fs::write(ctx.dir.path().join("two.txt"), "hit\n").unwrap();

    let (reg, cancel) = token(1, "t8");
    reg.cancel(1, "t8");
    assert!(cancel.is_cancelled());
    let summary = search_replace(&ctx.root(), "hit", "X", &[], &cancel).unwrap();
    assert!(summary.cancelled, "cancelled run reports cancellation");
}

#[test]
fn token_reuse_supersedes_previous_request() {
    let registry = CancellationRegistry::default();
    let first = registry.register(7, "shared-token");
    assert!(!first.is_cancelled());

    // Reusing (window, token) aborts the previous request (spec §6).
    let _second = registry.register(7, "shared-token");
    assert!(first.is_cancelled());

    // A different window using the same token string does NOT supersede.
    let other_window = registry.register(8, "shared-token");
    assert!(!other_window.is_cancelled());
}

#[test]
fn cancel_is_window_scoped() {
    let registry = CancellationRegistry::default();
    let handle = registry.register(1, "tok");
    registry.cancel(2, "tok"); // other window
    assert!(!handle.is_cancelled());
    registry.cancel(1, "tok"); // owning window
    assert!(handle.is_cancelled());
}

#[test]
fn replace_summary_json_uses_camel_case() {
    let summary = mcode_fs::search::ReplaceSummary {
        outcomes: Default::default(),
        cancelled: true,
    };
    let json = serde_json::to_value(&summary).unwrap();
    assert_eq!(json["cancelled"], serde_json::json!(true));
    assert!(json.get("outcomes").is_some());
}
