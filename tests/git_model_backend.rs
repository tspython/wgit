#![allow(dead_code)]

#[path = "../src/theme.rs"]
mod theme;

#[path = "../src/models.rs"]
mod models;

#[path = "../src/git_model.rs"]
mod git_model;

use git_model::{GitModel, StatusSectionKind, classify_status_xy, summarize_porcelain_status};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

// ── Unit tests (pre-existing) ────────────────────────────────────

#[test]
fn classifies_porcelain_xy_pairs() {
    assert_eq!(classify_status_xy(" M"), vec![StatusSectionKind::Unstaged]);
    assert_eq!(classify_status_xy("A "), vec![StatusSectionKind::Staged]);
    assert_eq!(classify_status_xy("??"), vec![StatusSectionKind::Untracked]);
}

#[test]
fn summarizes_grouped_status_counts() {
    let status = concat!(
        " M src/main.rs\n",
        "A  src/lib.rs\n",
        "MM src/config.toml\n",
        "?? Cargo.lock\n",
    );

    let summaries = summarize_porcelain_status(status);
    assert_eq!(summaries.len(), 3);
    assert_eq!(summaries[0].kind, StatusSectionKind::Staged);
    assert_eq!(summaries[0].count, 2);
    assert_eq!(summaries[1].kind, StatusSectionKind::Unstaged);
    assert_eq!(summaries[1].count, 1);
    assert_eq!(summaries[2].kind, StatusSectionKind::Untracked);
    assert_eq!(summaries[2].count, 1);
}

// ── Fixture-based integration tests ──────────────────────────────

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A wrapper that owns a temporary directory and removes it on drop.
struct TestRepo {
    path: PathBuf,
    model: GitModel,
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Helper: run a git command inside `dir`, panicking on failure.
fn git(dir: &PathBuf, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("failed to run git");
    if !out.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Create a fresh temporary git repo with one initial commit so HEAD exists.
fn create_test_repo() -> TestRepo {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "wgit_test_{}_{id}",
        std::process::id()
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("clean pre-existing temp dir");
    }
    fs::create_dir_all(&dir).expect("create temp dir");

    git(&dir, &["init", "-b", "main"]);
    git(&dir, &["config", "user.email", "test@test.com"]);
    git(&dir, &["config", "user.name", "Test"]);
    git(&dir, &["config", "commit.gpgsign", "false"]);

    // Create an initial commit so HEAD is valid.
    let init_file = dir.join(".gitkeep");
    fs::write(&init_file, "").expect("write .gitkeep");
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "initial commit"]);

    let model = GitModel::open_at(&dir).expect("open_at should succeed on valid repo");
    TestRepo { path: dir, model }
}

// ── Test cases ───────────────────────────────────────────────────

#[test]
fn test_open_valid_repo() {
    let repo = create_test_repo();
    // If we got here, open_at succeeded. Verify repo_root points at our dir.
    assert_eq!(
        repo.model.repo_root().canonicalize().unwrap(),
        repo.path.canonicalize().unwrap()
    );
}

#[test]
fn test_open_invalid_path() {
    let bad = std::env::temp_dir().join("wgit_nonexistent_repo_path");
    // Make sure it does NOT exist.
    let _ = fs::remove_dir_all(&bad);
    let result = GitModel::open_at(&bad);
    assert!(result.is_err(), "open_at on non-repo dir should fail");
}

#[test]
fn test_refresh_clean_repo() {
    let mut repo = create_test_repo();
    repo.model.refresh().expect("refresh should succeed");
    assert_eq!(
        repo.model.entries_len(),
        0,
        "clean repo should have no changed entries"
    );
}

#[test]
fn test_stage_new_file() {
    let mut repo = create_test_repo();

    // Create an untracked file.
    fs::write(repo.path.join("hello.txt"), "hello").expect("write file");
    repo.model.refresh().expect("refresh");
    assert!(
        repo.model.entries_len() > 0,
        "should see the new untracked file"
    );

    // Stage it.
    repo.model.stage_selected().expect("stage_selected");

    // After staging, the file should appear as staged (xy starts with a
    // non-space, non-? index letter).  We verify indirectly: run a porcelain
    // status and check for "A " prefix.
    let status_output = Command::new("git")
        .arg("-C")
        .arg(&repo.path)
        .args(["status", "--porcelain=v1"])
        .output()
        .expect("git status");
    let status = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        status.contains("A  hello.txt"),
        "file should be staged (got: {status})"
    );
}

#[test]
fn test_unstage_file() {
    let mut repo = create_test_repo();

    // Create and stage a file.
    fs::write(repo.path.join("unstage_me.txt"), "data").expect("write file");
    repo.model.refresh().expect("refresh");
    repo.model.stage_selected().expect("stage");

    // Unstage it.
    repo.model.unstage_selected().expect("unstage");

    // The file should now be untracked again.
    let status_output = Command::new("git")
        .arg("-C")
        .arg(&repo.path)
        .args(["status", "--porcelain=v1"])
        .output()
        .expect("git status");
    let status = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        status.contains("?? unstage_me.txt"),
        "file should be back to untracked (got: {status})"
    );
}

#[test]
fn test_stage_all() {
    let mut repo = create_test_repo();

    // Create multiple untracked files.
    fs::write(repo.path.join("a.txt"), "aaa").expect("write a");
    fs::write(repo.path.join("b.txt"), "bbb").expect("write b");
    fs::write(repo.path.join("c.txt"), "ccc").expect("write c");
    repo.model.refresh().expect("refresh");
    assert_eq!(repo.model.entries_len(), 3, "should see 3 untracked files");

    repo.model.stage_all().expect("stage_all");

    // All files should now be staged.
    let status_output = Command::new("git")
        .arg("-C")
        .arg(&repo.path)
        .args(["status", "--porcelain=v1"])
        .output()
        .expect("git status");
    let status = String::from_utf8_lossy(&status_output.stdout);
    for name in &["a.txt", "b.txt", "c.txt"] {
        assert!(
            status.contains(&format!("A  {name}")),
            "{name} should be staged (got: {status})"
        );
    }
}

#[test]
fn test_unstage_all() {
    let mut repo = create_test_repo();

    fs::write(repo.path.join("x.txt"), "x").expect("write x");
    fs::write(repo.path.join("y.txt"), "y").expect("write y");
    repo.model.refresh().expect("refresh");
    repo.model.stage_all().expect("stage_all");
    repo.model.unstage_all().expect("unstage_all");

    // All files should be back to untracked.
    let status_output = Command::new("git")
        .arg("-C")
        .arg(&repo.path)
        .args(["status", "--porcelain=v1"])
        .output()
        .expect("git status");
    let status = String::from_utf8_lossy(&status_output.stdout);
    for name in &["x.txt", "y.txt"] {
        assert!(
            status.contains(&format!("?? {name}")),
            "{name} should be untracked after unstage_all (got: {status})"
        );
    }
}

#[test]
fn test_commit() {
    let mut repo = create_test_repo();

    fs::write(repo.path.join("committed.txt"), "content").expect("write");
    repo.model.refresh().expect("refresh");
    repo.model.stage_all().expect("stage_all");
    repo.model.commit("add committed.txt").expect("commit");

    assert_eq!(
        repo.model.entries_len(),
        0,
        "working tree should be clean after commit"
    );

    // Verify the commit message via git log.
    let log = Command::new("git")
        .arg("-C")
        .arg(&repo.path)
        .args(["log", "--oneline", "-1"])
        .output()
        .expect("git log");
    let log_msg = String::from_utf8_lossy(&log.stdout);
    assert!(
        log_msg.contains("add committed.txt"),
        "commit message should appear in log (got: {log_msg})"
    );
}

#[test]
fn test_branch_name() {
    let repo = create_test_repo();
    let branch = repo.model.branch();
    assert!(
        branch == "main" || branch == "master",
        "branch should be 'main' or 'master', got '{branch}'"
    );
}

#[test]
fn test_entries_len() {
    let mut repo = create_test_repo();

    assert_eq!(repo.model.entries_len(), 0, "clean repo has 0 entries");

    fs::write(repo.path.join("one.txt"), "1").expect("write");
    fs::write(repo.path.join("two.txt"), "2").expect("write");
    repo.model.refresh().expect("refresh");

    assert_eq!(
        repo.model.entries_len(),
        2,
        "should report 2 changed files"
    );
}

#[test]
fn test_move_selection() {
    let mut repo = create_test_repo();

    fs::write(repo.path.join("f1.txt"), "1").expect("write");
    fs::write(repo.path.join("f2.txt"), "2").expect("write");
    fs::write(repo.path.join("f3.txt"), "3").expect("write");
    repo.model.refresh().expect("refresh");

    assert_eq!(repo.model.selected_index(), 0, "initial selection is 0");

    repo.model.move_selection(1).expect("move +1");
    assert_eq!(repo.model.selected_index(), 1);

    repo.model.move_selection(1).expect("move +1");
    assert_eq!(repo.model.selected_index(), 2);

    // Moving past the end should clamp.
    repo.model.move_selection(1).expect("move +1 (clamped)");
    assert_eq!(repo.model.selected_index(), 2, "should clamp at last entry");

    // Move backward.
    repo.model.move_selection(-2).expect("move -2");
    assert_eq!(repo.model.selected_index(), 0);

    // Moving before the start should clamp.
    repo.model.move_selection(-1).expect("move -1 (clamped)");
    assert_eq!(
        repo.model.selected_index(),
        0,
        "should clamp at first entry"
    );
}
