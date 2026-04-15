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
