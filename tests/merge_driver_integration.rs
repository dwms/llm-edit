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
