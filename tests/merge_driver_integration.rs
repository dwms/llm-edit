// End-to-end test: create a tiny git repo with two branches that each
// carry `.llm-edit/` op logs, run `llm-edit git-merge theirs`, and
// assert the merged file has rename-propagated content AND the merged
// log has landed back on disk.
//
// This exercises #11's persistence path through an actual git workflow
// (not via the bench harness or hand-driven CLI script).

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn run(prog: &str, args: &[&str], cwd: &Path) {
    let out = Command::new(prog)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "{} {:?} in {} failed:\nstdout: {}\nstderr: {}",
        prog,
        args,
        cwd.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn run_llm_edit(args: &[&str], cwd: &Path) {
    let bin = env!("CARGO_BIN_EXE_llm-edit");
    run(bin, args, cwd);
}

#[test]
fn git_merge_propagates_rename_into_new_caller() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    // Base commit: source file + empty op log.
    fs::write(repo.join("app.py"), "def get(user_id):\n    return user_id\n").unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    // Branch `theirs`: add a caller that references the OLD name.
    run("git", &["checkout", "-b", "theirs", "-q"], repo);
    run_llm_edit(
        &[
            "apply",
            "--json",
            r#"{"op":"add_function","anchor":{"file":"app.py","path":["module"]},"after":{"kind":"function_def","name":"get"},"name":"caller","source":"def caller():\n    return get(user_id=1)\n"}"#,
        ],
        repo,
    );
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "theirs: add caller"], repo);

    // Back to `main`: rename user_id → account_id.
    run("git", &["checkout", "main", "-q"], repo);
    run_llm_edit(
        &[
            "rename-symbol",
            "--file",
            "app.py",
            "user_id",
            "account_id",
        ],
        repo,
    );
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "main: rename"], repo);

    // The integration point: our CLI does the whole-tree merge using
    // each side's `.llm-edit/log.jsonl`.
    run_llm_edit(&["git-merge", "theirs"], repo);

    let app = fs::read_to_string(repo.join("app.py")).unwrap();
    assert!(
        app.contains("def get(account_id):"),
        "get signature not renamed:\n{}",
        app
    );
    assert!(app.contains("def caller():"), "caller missing:\n{}", app);
    assert!(
        app.contains("return get(account_id=1)"),
        "caller call-site not rewritten through persisted rename:\n{}",
        app
    );
    assert!(
        !app.contains("user_id"),
        "stale user_id reference survived merge:\n{}",
        app
    );

    // Merged log carries both ops forward for the next generation.
    let log = fs::read_to_string(repo.join(".llm-edit/log.jsonl")).unwrap();
    assert!(
        log.contains("rename_symbol"),
        "merged log missing rename_symbol:\n{}",
        log
    );
    assert!(
        log.contains("add_function"),
        "merged log missing add_function:\n{}",
        log
    );

    // A merge commit exists with two parents.
    let log_out = Command::new("git")
        .args(["log", "--pretty=%P", "-1"])
        .current_dir(repo)
        .output()
        .unwrap();
    let parents = String::from_utf8(log_out.stdout).unwrap();
    assert_eq!(
        parents.trim().split_whitespace().count(),
        2,
        "merge commit should have two parents, got: {}",
        parents
    );
}

/// Hybrid mode: one side has an unrecorded text edit alongside a
/// recorded op. The unrecorded edit must survive the merge (text
/// fallback for that file) while the other file's recorded op still
/// merges semantically.
#[test]
fn git_merge_hybrid_preserves_unrecorded_edits() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    fs::write(
        repo.join("app.py"),
        "def get():\n    return 1\n",
    )
    .unwrap();
    fs::write(
        repo.join("util.py"),
        "def helper():\n    return 2\n",
    )
    .unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    // theirs: add a function in util.py via the CLI (recorded op).
    run("git", &["checkout", "-b", "theirs", "-q"], repo);
    run_llm_edit(
        &[
            "apply",
            "--json",
            r#"{"op":"add_function","anchor":{"file":"util.py","path":["module"]},"after":{"kind":"function_def","name":"helper"},"name":"twice","source":"def twice():\n    return helper() * 2\n"}"#,
        ],
        repo,
    );
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "theirs: add twice"], repo);

    // main: recorded rename + UNrecorded comment edit on the SAME file.
    run("git", &["checkout", "main", "-q"], repo);
    run_llm_edit(
        &["rename-symbol", "--file", "app.py", "get", "fetch"],
        repo,
    );
    let mut app = fs::read_to_string(repo.join("app.py")).unwrap();
    app.push_str("# fetches the canonical id\n");
    fs::write(repo.join("app.py"), app).unwrap();
    run("git", &["add", "."], repo);
    run(
        "git",
        &["commit", "-q", "-m", "main: rename + comment"],
        repo,
    );

    // Merge.
    run_llm_edit(&["git-merge", "theirs"], repo);

    // app.py: rename + comment both present (text-fallback path).
    let app = fs::read_to_string(repo.join("app.py")).unwrap();
    assert!(
        app.contains("def fetch():"),
        "rename lost in fallback path:\n{}",
        app
    );
    assert!(
        app.contains("# fetches the canonical id"),
        "unrecorded comment lost — hybrid fallback failed to preserve it:\n{}",
        app
    );

    // util.py: semantic merge — both helper and twice present.
    let util = fs::read_to_string(repo.join("util.py")).unwrap();
    assert!(
        util.contains("def helper():") && util.contains("def twice():"),
        "util.py semantic merge dropped a function:\n{}",
        util
    );

    // Merged log carries the recorded ops forward.
    let log = fs::read_to_string(repo.join(".llm-edit/log.jsonl")).unwrap();
    assert!(log.contains("rename_symbol"), "log missing rename:\n{}", log);
    assert!(log.contains("add_function"), "log missing add_function:\n{}", log);
}

/// `llm-edit infer` derives a rename_symbol op from a textual edit.
/// The user edits a file directly (no CLI), runs `infer`, and the log
/// gains a rename op describing what they did. After this, future
/// merges treat the file as ops-covered (semantic path) instead of
/// falling back to text.
#[test]
fn infer_detects_single_identifier_rename() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    fs::write(
        repo.join("app.py"),
        "def get(user_id):\n    return user_id\n",
    )
    .unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    // Edit via plain file write — NOT through the CLI. Rename
    // `user_id` → `account_id` everywhere.
    fs::write(
        repo.join("app.py"),
        "def get(account_id):\n    return account_id\n",
    )
    .unwrap();

    // Run `llm-edit infer`. It should pick up the rename and append
    // a rename_symbol op to the log.
    run_llm_edit(&["infer"], repo);

    let log = fs::read_to_string(repo.join(".llm-edit/log.jsonl")).unwrap();
    assert!(
        log.contains("rename_symbol"),
        "infer didn't emit rename_symbol:\n{}",
        log
    );
    assert!(
        log.contains("\"old\":\"user_id\"") && log.contains("\"new\":\"account_id\""),
        "infer didn't capture the right rename:\n{}",
        log
    );
    assert!(
        log.contains("\"file\":\"app.py\""),
        "infer didn't include the file path:\n{}",
        log
    );

    // The file on disk should be unchanged from the user's edit.
    let app = fs::read_to_string(repo.join("app.py")).unwrap();
    assert_eq!(app, "def get(account_id):\n    return account_id\n");
}

/// `infer` covers the Python add_function / delete_function /
/// add_import patterns when the diff isn't a pure rename.
#[test]
fn infer_detects_python_structural_diffs() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    fs::write(
        repo.join("svc.py"),
        "import os\n\n\ndef foo():\n    return 1\n\n\ndef bar():\n    return 2\n",
    )
    .unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    // Edit: delete `foo`, add `baz` after `bar`, add `import sys`.
    fs::write(
        repo.join("svc.py"),
        "import os\nimport sys\n\n\ndef bar():\n    return 2\n\n\ndef baz():\n    return 3\n",
    )
    .unwrap();

    run_llm_edit(&["infer"], repo);

    let log = fs::read_to_string(repo.join(".llm-edit/log.jsonl")).unwrap();
    assert!(log.contains("\"op\":\"delete_function\""), "missing delete_function:\n{}", log);
    assert!(log.contains("\"op\":\"add_function\""), "missing add_function:\n{}", log);
    assert!(log.contains("\"op\":\"add_import\""), "missing add_import:\n{}", log);
    assert!(log.contains("\"name\":\"foo\"") || log.contains("\"foo\""), "delete should reference foo:\n{}", log);
    assert!(log.contains("\"name\":\"baz\""), "add should reference baz:\n{}", log);
    assert!(log.contains("\"module\":\"sys\""), "add_import should reference sys:\n{}", log);
}

/// `llm-edit git-rebase` replays the current branch's commits onto a
/// new base, with each commit's recorded ops getting rewritten through
/// the new base's own ops.
#[test]
fn git_rebase_replays_commits_with_op_propagation() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    fs::write(
        repo.join("app.py"),
        "def get(user_id):\n    return user_id\n",
    )
    .unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    // Branch `feat` adds a caller that uses the OLD name.
    run("git", &["checkout", "-b", "feat", "-q"], repo);
    run_llm_edit(
        &[
            "apply",
            "--json",
            r#"{"op":"add_function","anchor":{"file":"app.py","path":["module"]},"after":{"kind":"function_def","name":"get"},"name":"caller","source":"def caller():\n    return get(user_id=1)\n"}"#,
        ],
        repo,
    );
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "feat: add caller"], repo);

    // Back to main: rename user_id → account_id.
    run("git", &["checkout", "main", "-q"], repo);
    run_llm_edit(
        &["rename-symbol", "--file", "app.py", "user_id", "account_id"],
        repo,
    );
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "main: rename"], repo);

    // Switch to feat and rebase onto main.
    run("git", &["checkout", "feat", "-q"], repo);
    run_llm_edit(&["git-rebase", "main"], repo);

    // After rebase, app.py should contain BOTH the rename (from main)
    // AND the caller (from feat, with rename propagated into its body).
    let app = fs::read_to_string(repo.join("app.py")).unwrap();
    assert!(
        app.contains("def get(account_id):"),
        "rename not preserved post-rebase:\n{}",
        app
    );
    assert!(app.contains("def caller():"), "caller missing:\n{}", app);
    assert!(
        app.contains("return get(account_id=1)"),
        "caller's call-site not rewritten through propagated rename:\n{}",
        app
    );

    // History should be linear: main's tip, then feat's commit on top.
    let history = Command::new("git")
        .args(["log", "--format=%s", "feat"])
        .current_dir(repo)
        .output()
        .unwrap();
    let history_text = String::from_utf8(history.stdout).unwrap();
    let commits: Vec<&str> = history_text.lines().collect();
    assert_eq!(
        commits,
        vec!["feat: add caller", "main: rename", "base"],
        "rebase produced wrong commit history:\n{}",
        history_text
    );
}

/// `git-rebase --continue` after a text-fallback conflict: the user
/// resolves the overlapping edit manually, then --continue finishes
/// the rebase with the expected linear history.
#[test]
fn git_rebase_continue_after_text_conflict() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    // Base: a simple Python file with an empty op log.
    fs::write(
        repo.join("app.py"),
        "def get():\n    return 1\n",
    )
    .unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    // feat: unrecorded edit — change return value to 20.
    run("git", &["checkout", "-b", "feat", "-q"], repo);
    fs::write(repo.join("app.py"), "def get():\n    return 20\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "feat: bump return"], repo);

    // main: unrecorded edit on the SAME line — change return value to 10.
    run("git", &["checkout", "main", "-q"], repo);
    fs::write(repo.join("app.py"), "def get():\n    return 10\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "main: different bump"], repo);

    // Start rebase of feat onto main — overlapping edits -> text conflict.
    run("git", &["checkout", "feat", "-q"], repo);
    let bin = env!("CARGO_BIN_EXE_llm-edit");
    let out = Command::new(bin)
        .args(["git-rebase", "main"])
        .current_dir(repo)
        .output()
        .expect("spawn");
    // Expect exit code 1 — conflict.
    assert!(
        !out.status.success(),
        "rebase was expected to conflict, stdout: {} stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // State file should exist.
    let state_path = repo.join(".git/llm-edit-rebase/state.json");
    assert!(state_path.exists(), "state file missing after conflict");

    // Working tree should contain diff3 output — we just check for the
    // conflict marker so we know the conflicted file was materialized.
    let conflicted = fs::read_to_string(repo.join("app.py")).unwrap();
    assert!(
        conflicted.contains("<<<<<<<"),
        "working tree not populated with diff3 output:\n{}",
        conflicted,
    );

    // User resolves: pick feat's value (20).
    fs::write(repo.join("app.py"), "def get():\n    return 20\n").unwrap();

    // --continue
    run_llm_edit(&["git-rebase", "--continue"], repo);

    // State file gone.
    assert!(!state_path.exists(), "state file not cleaned up after continue");

    // HEAD on feat, app.py resolved.
    let branch = Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(branch.stdout).unwrap().trim(),
        "feat",
        "HEAD not reattached to feat"
    );
    let app = fs::read_to_string(repo.join("app.py")).unwrap();
    assert_eq!(app, "def get():\n    return 20\n");

    // Linear history: feat's commit on top of main's.
    let history = Command::new("git")
        .args(["log", "--format=%s", "feat"])
        .current_dir(repo)
        .output()
        .unwrap();
    let subjects: Vec<String> = String::from_utf8(history.stdout)
        .unwrap()
        .lines()
        .map(|l| l.to_string())
        .collect();
    assert_eq!(
        subjects,
        vec!["feat: bump return", "main: different bump", "base"],
        "wrong history after --continue",
    );
}

/// `git-rebase --abort` after a text-fallback conflict: original branch
/// is restored to its pre-rebase tip and HEAD is re-attached to it.
#[test]
fn git_rebase_abort_restores_original() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    fs::write(
        repo.join("app.py"),
        "def get():\n    return 1\n",
    )
    .unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    run("git", &["checkout", "-b", "feat", "-q"], repo);
    fs::write(repo.join("app.py"), "def get():\n    return 20\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "feat: bump return"], repo);

    run("git", &["checkout", "main", "-q"], repo);
    fs::write(repo.join("app.py"), "def get():\n    return 10\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "main: different bump"], repo);

    run("git", &["checkout", "feat", "-q"], repo);
    let feat_sha_before = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "feat"])
            .current_dir(repo)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    // Trigger conflict.
    let bin = env!("CARGO_BIN_EXE_llm-edit");
    let out = Command::new(bin)
        .args(["git-rebase", "main"])
        .current_dir(repo)
        .output()
        .expect("spawn");
    assert!(!out.status.success(), "rebase should have conflicted");
    assert!(repo.join(".git/llm-edit-rebase/state.json").exists());

    // Abort.
    run_llm_edit(&["git-rebase", "--abort"], repo);

    // State gone.
    assert!(!repo.join(".git/llm-edit-rebase/state.json").exists());

    // feat branch ref restored.
    let feat_sha_after = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "feat"])
            .current_dir(repo)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    assert_eq!(
        feat_sha_before, feat_sha_after,
        "feat branch ref not restored to pre-rebase SHA"
    );

    // HEAD re-attached to feat.
    let branch = Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(branch.stdout).unwrap().trim(),
        "feat",
        "HEAD not re-attached to feat after abort"
    );

    // Working tree back to feat's content.
    let app = fs::read_to_string(repo.join("app.py")).unwrap();
    assert_eq!(app, "def get():\n    return 20\n");
}

// ---- interactive rebase helpers --------------------------------------

/// Write an executable shell script that will be handed to llm-edit via
/// `LLM_EDIT_REBASE_EDITOR`. The script receives the file to edit as
/// its first (and only) positional arg.
fn write_editor_script(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{}\n", body)).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }
    path
}

/// Set up a linear 3-commit feat branch (a, middle, c) on top of `main`
/// with no overlapping changes so rebase onto main is conflict-free.
fn setup_three_commit_branch(repo: &Path) {
    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    fs::write(repo.join("a.txt"), "base\n").unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    run("git", &["checkout", "-b", "feat", "-q"], repo);

    fs::write(repo.join("a.txt"), "base\nfirst\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "feat: first"], repo);

    fs::write(repo.join("b.txt"), "middle\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "feat: middle"], repo);

    fs::write(repo.join("c.txt"), "last\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "feat: last"], repo);
}

fn run_llm_edit_with_env(args: &[&str], cwd: &Path, env: &[(&str, &str)]) {
    let bin = env!("CARGO_BIN_EXE_llm-edit");
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn");
    assert!(
        out.status.success(),
        "{:?} in {} failed:\nstdout: {}\nstderr: {}",
        args,
        cwd.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn commit_subjects(repo: &Path, rev: &str) -> Vec<String> {
    let out = Command::new("git")
        .args(["log", "--format=%s", rev])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

/// `git-rebase -i` with the middle commit marked `drop`: resulting
/// branch has 2 commits, no trace of the middle one.
#[test]
fn git_rebase_interactive_drop() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    setup_three_commit_branch(repo);

    // Add a commit to main so the rebase actually moves commits.
    run("git", &["checkout", "main", "-q"], repo);
    fs::write(repo.join("main.txt"), "main\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "main: extra"], repo);
    run("git", &["checkout", "feat", "-q"], repo);

    // Editor: rewrite `pick <sha> feat: middle` -> `drop <sha> feat: middle`.
    let editor = write_editor_script(
        repo,
        "editor.sh",
        r#"sed -i.bak -e 's/^pick \([a-f0-9]*\) feat: middle/drop \1 feat: middle/' "$1""#,
    );

    run_llm_edit_with_env(
        &["git-rebase", "-i", "main"],
        repo,
        &[("LLM_EDIT_REBASE_EDITOR", editor.to_str().unwrap())],
    );

    let subjects = commit_subjects(repo, "feat");
    assert_eq!(
        subjects,
        vec![
            "feat: last".to_string(),
            "feat: first".to_string(),
            "main: extra".to_string(),
            "base".to_string(),
        ],
        "dropped commit should not appear in history: {:?}",
        subjects
    );
    // b.txt should no longer exist (it was introduced by the dropped commit).
    assert!(!repo.join("b.txt").exists(), "dropped commit's file still present");
    assert!(repo.join("c.txt").exists(), "later commit's file missing");
}

/// `git-rebase -i` with the last commit marked `squash`: combined
/// message edited; resulting branch has 2 feat commits collapsed into 1.
#[test]
fn git_rebase_interactive_squash() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    setup_three_commit_branch(repo);

    run("git", &["checkout", "main", "-q"], repo);
    fs::write(repo.join("main.txt"), "main\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "main: extra"], repo);
    run("git", &["checkout", "feat", "-q"], repo);

    // First editor invocation: rewrite the todo to squash `feat: last`
    // into `feat: middle`. Second editor invocation: commit message
    // prompt for the squash — overwrite with a known combined message.
    // Use a marker file to distinguish the two calls.
    let editor = write_editor_script(
        repo,
        "editor.sh",
        r#"
marker="$(dirname "$1")/.editor-call-count"
if [ ! -f "$marker" ]; then
    echo 1 > "$marker"
    sed -i.bak -e 's/^pick \([a-f0-9]*\) feat: last/squash \1 feat: last/' "$1"
else
    echo "combined middle and last" > "$1"
fi
"#,
    );

    run_llm_edit_with_env(
        &["git-rebase", "-i", "main"],
        repo,
        &[("LLM_EDIT_REBASE_EDITOR", editor.to_str().unwrap())],
    );

    let subjects = commit_subjects(repo, "feat");
    assert_eq!(
        subjects,
        vec![
            "combined middle and last".to_string(),
            "feat: first".to_string(),
            "main: extra".to_string(),
            "base".to_string(),
        ],
        "squash should collapse middle+last into one commit: {:?}",
        subjects
    );
    // Both files from squashed commits should be present.
    assert!(repo.join("b.txt").exists(), "middle's file missing after squash");
    assert!(repo.join("c.txt").exists(), "last's file missing after squash");
}

/// `git-rebase -i` with `fixup`: same tree result as squash, but the
/// prior commit's message is kept verbatim — no editor prompt for the
/// combined message.
#[test]
fn git_rebase_interactive_fixup() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    setup_three_commit_branch(repo);

    run("git", &["checkout", "main", "-q"], repo);
    fs::write(repo.join("main.txt"), "main\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "main: extra"], repo);
    run("git", &["checkout", "feat", "-q"], repo);

    let editor = write_editor_script(
        repo,
        "editor.sh",
        r#"sed -i.bak -e 's/^pick \([a-f0-9]*\) feat: last/fixup \1 feat: last/' "$1""#,
    );

    run_llm_edit_with_env(
        &["git-rebase", "-i", "main"],
        repo,
        &[("LLM_EDIT_REBASE_EDITOR", editor.to_str().unwrap())],
    );

    let subjects = commit_subjects(repo, "feat");
    assert_eq!(
        subjects,
        vec![
            "feat: middle".to_string(),
            "feat: first".to_string(),
            "main: extra".to_string(),
            "base".to_string(),
        ],
        "fixup should keep prior's message: {:?}",
        subjects
    );
    assert!(repo.join("b.txt").exists());
    assert!(repo.join("c.txt").exists());
}

/// `git-rebase -i` with `reword`: single commit, rename its message.
#[test]
fn git_rebase_interactive_reword() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    fs::write(repo.join("a.txt"), "base\n").unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    // Advance main so rebase does real work.
    fs::write(repo.join("main.txt"), "main\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "main: extra"], repo);

    run("git", &["checkout", "-b", "feat", "HEAD~1", "-q"], repo);
    fs::write(repo.join("a.txt"), "base\nfeat\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "feat: original message"], repo);

    // Two-call editor: first rewrites the todo (pick -> reword),
    // second supplies the new commit message.
    let editor = write_editor_script(
        repo,
        "editor.sh",
        r#"
marker="$(dirname "$1")/.editor-call-count"
if [ ! -f "$marker" ]; then
    echo 1 > "$marker"
    sed -i.bak -e 's/^pick \([a-f0-9]*\) feat: original message/reword \1 feat: original message/' "$1"
else
    echo "feat: reworded message" > "$1"
fi
"#,
    );

    run_llm_edit_with_env(
        &["git-rebase", "-i", "main"],
        repo,
        &[("LLM_EDIT_REBASE_EDITOR", editor.to_str().unwrap())],
    );

    let subjects = commit_subjects(repo, "feat");
    assert_eq!(
        subjects,
        vec![
            "feat: reworded message".to_string(),
            "main: extra".to_string(),
            "base".to_string(),
        ],
        "reword should replace the commit message: {:?}",
        subjects
    );
}

/// `infer` derives a `replace_in_anchor` op for a single substring
/// change inside a function body.
#[test]
fn infer_detects_replace_in_anchor() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    fs::write(
        repo.join("calc.py"),
        "def total(value):\n    return value * 2\n",
    )
    .unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    // Edit the body: change the multiplier.
    fs::write(
        repo.join("calc.py"),
        "def total(value):\n    return value * 3\n",
    )
    .unwrap();

    run_llm_edit(&["infer"], repo);

    let log = fs::read_to_string(repo.join(".llm-edit/log.jsonl")).unwrap();
    assert!(
        log.contains("\"op\":\"replace_in_anchor\""),
        "missing replace_in_anchor:\n{}",
        log
    );
    assert!(log.contains("\"old\":\"2\""), "wrong old:\n{}", log);
    assert!(log.contains("\"new\":\"3\""), "wrong new:\n{}", log);
    assert!(log.contains("\"name\":\"total\""), "wrong fn:\n{}", log);
}

/// Run llm-edit with env vars and return the exit code instead of
/// asserting success.
fn run_llm_edit_with_env_code(args: &[&str], cwd: &Path, env: &[(&str, &str)]) -> i32 {
    let bin = env!("CARGO_BIN_EXE_llm-edit");
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn");
    out.status.code().unwrap_or(-1)
}

/// `git-rebase -i` with the middle commit marked `edit`: the rebase
/// pauses (exit 1), the user modifies a file, then `--continue` amends
/// the paused commit and replays the remaining commits.
#[test]
fn git_rebase_interactive_edit() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    setup_three_commit_branch(repo);

    // Add a commit to main so the rebase actually moves commits.
    run("git", &["checkout", "main", "-q"], repo);
    fs::write(repo.join("main.txt"), "main\n").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "main: extra"], repo);
    run("git", &["checkout", "feat", "-q"], repo);

    // Editor: rewrite `pick <sha> feat: middle` -> `edit <sha> feat: middle`.
    let editor = write_editor_script(
        repo,
        "editor.sh",
        r#"sed -i.bak -e 's/^pick \([a-f0-9]*\) feat: middle/edit \1 feat: middle/' "$1""#,
    );
    let env = [("LLM_EDIT_REBASE_EDITOR", editor.to_str().unwrap())];

    // First invocation: should pause (exit 1) at the middle commit.
    let code = run_llm_edit_with_env_code(&["git-rebase", "-i", "main"], repo, &env);
    assert_eq!(code, 1, "rebase should pause with exit code 1 for edit");

    // State file should have paused_for_edit=true.
    let state_json = fs::read_to_string(
        repo.join(".git/llm-edit-rebase/state.json"),
    )
    .expect("state.json should exist");
    assert!(
        state_json.contains("\"paused_for_edit\": true")
            || state_json.contains("\"paused_for_edit\":true"),
        "state.json should have paused_for_edit=true:\n{}",
        state_json
    );

    // The working tree should have b.txt (from the middle commit).
    assert!(
        repo.join("b.txt").exists(),
        "b.txt should be in working tree after edit pause"
    );

    // Modify b.txt — this is the user's edit.
    fs::write(repo.join("b.txt"), "middle-amended\n").unwrap();

    // Continue the rebase.
    run_llm_edit_with_env(
        &["git-rebase", "--continue"],
        repo,
        &[],
    );

    // The branch should now be fully rebased.
    let subjects = commit_subjects(repo, "feat");
    assert_eq!(
        subjects,
        vec![
            "feat: last".to_string(),
            "feat: middle".to_string(),
            "feat: first".to_string(),
            "main: extra".to_string(),
            "base".to_string(),
        ],
        "all commits should be present after edit+continue: {:?}",
        subjects
    );

    // The amended content should be in the middle commit's tree.
    let b_content = fs::read_to_string(repo.join("b.txt")).unwrap();
    assert_eq!(
        b_content, "middle-amended\n",
        "b.txt should reflect the amendment"
    );

    // c.txt from the last commit should still be present.
    assert!(repo.join("c.txt").exists(), "last commit's file missing after edit+continue");
}

/// Dogfood guarantee: AST merge cleanly resolves two non-overlapping
/// edits to different functions in the same Rust file. Without
/// AST-aware merging, plain diff3 on a large Rust file like our own
/// src/cli.rs has historically dropped braces or whole declarations
/// when both branches edited (different) functions in the same file.
///
/// This test locks in the fix from #46 (tree-sitter-rust 0.24): if the
/// parser can't read the file, this collapses back to text diff3 and
/// the assertion below detects the regression.
#[test]
fn git_merge_ast_resolves_disjoint_rust_function_edits() {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);

    let base = "fn alpha() -> i32 {\n    1\n}\n\nfn beta() -> i32 {\n    2\n}\n\nfn gamma() -> i32 {\n    3\n}\n";
    fs::write(repo.join("lib.rs"), base).unwrap();
    fs::create_dir_all(repo.join(".llm-edit")).unwrap();
    fs::write(repo.join(".llm-edit/log.jsonl"), "").unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "base"], repo);

    // theirs edits gamma's body.
    run("git", &["checkout", "-b", "theirs", "-q"], repo);
    let theirs = "fn alpha() -> i32 {\n    1\n}\n\nfn beta() -> i32 {\n    2\n}\n\nfn gamma() -> i32 {\n    33\n}\n";
    fs::write(repo.join("lib.rs"), theirs).unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "theirs: edit gamma"], repo);

    // main edits alpha's body.
    run("git", &["checkout", "main", "-q"], repo);
    let ours = "fn alpha() -> i32 {\n    11\n}\n\nfn beta() -> i32 {\n    2\n}\n\nfn gamma() -> i32 {\n    3\n}\n";
    fs::write(repo.join("lib.rs"), ours).unwrap();
    run("git", &["add", "."], repo);
    run("git", &["commit", "-q", "-m", "ours: edit alpha"], repo);

    run_llm_edit(&["git-merge", "theirs"], repo);

    let merged = fs::read_to_string(repo.join("lib.rs")).unwrap();
    assert!(
        !merged.contains("<<<<<<<"),
        "AST merge should produce no conflict markers on disjoint Rust function edits:\n{}",
        merged
    );
    assert!(merged.contains("11"), "alpha edit lost:\n{}", merged);
    assert!(merged.contains("33"), "gamma edit lost:\n{}", merged);
    // Sanity: beta untouched.
    assert!(merged.contains("fn beta() -> i32 {\n    2\n}"), "beta corrupted:\n{}", merged);
}
