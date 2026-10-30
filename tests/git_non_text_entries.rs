// git-merge and git-rebase on trees that hold more than text files:
// submodule pointers (mode 160000), symlinks, binary blobs, executable
// bits, and deletions. The semantic merger only sees text; everything
// else has to come through the merge unchanged or take the side that
// changed it, the way `git merge` does.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

fn exec(prog: &str, args: &[&str], cwd: &Path) -> Output {
    Command::new(prog)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn")
}

fn run(prog: &str, args: &[&str], cwd: &Path) -> String {
    let out = exec(prog, args, cwd);
    assert!(
        out.status.success(),
        "{} {:?} failed:\nstdout: {}\nstderr: {}",
        prog,
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn llm_edit(args: &[&str], cwd: &Path) -> Output {
    exec(env!("CARGO_BIN_EXE_llm-edit"), args, cwd)
}

fn git_bytes(args: &[&str], cwd: &Path) -> Vec<u8> {
    let out = exec("git", args, cwd);
    assert!(out.status.success(), "git {:?} failed", args);
    out.stdout
}

/// `mode type oid` of `path` in `rev`, or "" when absent.
fn ls_tree(repo: &Path, rev: &str, path: &str) -> String {
    let line = run("git", &["ls-tree", rev, "--", path], repo);
    line.split('\t').next().unwrap_or("").to_string()
}

fn set_gitlink(repo: &Path, path: &str, sha: &str) {
    run(
        "git",
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},{}", sha, path),
        ],
        repo,
    );
}

const BASE_BIN: &[u8] = &[0x00, 0x9f, 0x92, 0x96, 0xff, 0x00, 0x01];
const THEIR_BIN: &[u8] = &[0x00, 0xfe, 0xfd, 0x10, 0x80, 0x81];
const PHOTO: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0xc3];

/// Repo with a text file, two binaries, a symlink, a file theirs will
/// delete, an unrelated tracked file, and a gitlink at `vendor/sub`.
/// Submodule pointers are commit ids that are not in the repo, as in a
/// real superproject whose submodule history lives elsewhere.
const SUB_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SUB_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SUB_C: &str = "cccccccccccccccccccccccccccccccccccccccc";

fn init_repo(repo: &Path) {
    run("git", &["init", "-q", "--initial-branch=main"], repo);
    run("git", &["config", "user.email", "test@test"], repo);
    run("git", &["config", "user.name", "test"], repo);
    fs::write(repo.join("app.py"), "def get():\n    return 1\n").unwrap();
    fs::write(repo.join("notes.md"), "notes\n").unwrap();
    fs::write(repo.join("old.txt"), "going away\n").unwrap();
    fs::write(repo.join("logo.bin"), BASE_BIN).unwrap();
    fs::write(repo.join("photo.bin"), PHOTO).unwrap();
    std::os::unix::fs::symlink("app.py", repo.join("link.py")).unwrap();
    run("git", &["add", "."], repo);
    set_gitlink(repo, "vendor/sub", SUB_A);
    run("git", &["commit", "-q", "-m", "base"], repo);
}

#[test]
fn git_merge_carries_submodule_binary_symlink_mode_and_delete() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    init_repo(repo);

    // theirs: move the submodule pointer, replace a binary, delete a
    // file, add an executable script.
    run("git", &["checkout", "-q", "-b", "theirs"], repo);
    set_gitlink(repo, "vendor/sub", SUB_B);
    fs::write(repo.join("logo.bin"), THEIR_BIN).unwrap();
    run("git", &["rm", "-q", "old.txt"], repo);
    fs::write(repo.join("tool.sh"), "#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(repo.join("tool.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    run("git", &["add", "logo.bin", "tool.sh"], repo);
    run("git", &["commit", "-q", "-m", "theirs"], repo);
    let theirs = run("git", &["rev-parse", "HEAD"], repo);

    // ours: a text edit, then leave unrelated local state around that
    // the merge must not commit.
    run("git", &["checkout", "-q", "main"], repo);
    fs::write(repo.join("app.py"), "def get():\n    return 2\n").unwrap();
    run("git", &["commit", "-q", "-am", "ours"], repo);
    fs::write(repo.join("notes.md"), "local edit\n").unwrap();
    fs::write(repo.join("junk.txt"), "untracked\n").unwrap();

    let out = llm_edit(&["git-merge", "theirs"], repo);
    assert!(
        out.status.success(),
        "git-merge failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(run("git", &["rev-parse", "HEAD^2"], repo), theirs);
    assert_eq!(
        ls_tree(repo, "HEAD", "vendor/sub"),
        format!("160000 commit {}", SUB_B)
    );
    assert_eq!(git_bytes(&["show", "HEAD:logo.bin"], repo), THEIR_BIN);
    assert_eq!(fs::read(repo.join("logo.bin")).unwrap(), THEIR_BIN);
    assert_eq!(git_bytes(&["show", "HEAD:photo.bin"], repo), PHOTO);
    assert_eq!(fs::read(repo.join("photo.bin")).unwrap(), PHOTO);
    assert!(ls_tree(repo, "HEAD", "link.py").starts_with("120000 blob"));
    assert!(
        fs::symlink_metadata(repo.join("link.py"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(ls_tree(repo, "HEAD", "old.txt"), "");
    assert!(!repo.join("old.txt").exists());
    assert!(ls_tree(repo, "HEAD", "tool.sh").starts_with("100755 blob"));
    assert_eq!(
        run("git", &["show", "HEAD:app.py"], repo),
        "def get():\n    return 2"
    );

    // Local state is untouched and uncommitted.
    assert_eq!(ls_tree(repo, "HEAD", "junk.txt"), "");
    assert_eq!(run("git", &["show", "HEAD:notes.md"], repo), "notes");
    assert_eq!(
        fs::read_to_string(repo.join("notes.md")).unwrap(),
        "local edit\n"
    );
    assert_eq!(
        run("git", &["status", "--porcelain", "--", "notes.md"], repo),
        "M notes.md"
    );
}

#[test]
fn git_merge_reports_submodule_and_modify_delete_conflicts_as_unmerged() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    init_repo(repo);

    run("git", &["checkout", "-q", "-b", "theirs"], repo);
    set_gitlink(repo, "vendor/sub", SUB_B);
    run("git", &["rm", "-q", "old.txt"], repo);
    run("git", &["commit", "-q", "-m", "theirs"], repo);
    let theirs = run("git", &["rev-parse", "HEAD"], repo);

    run("git", &["checkout", "-q", "main"], repo);
    set_gitlink(repo, "vendor/sub", SUB_C);
    fs::write(repo.join("old.txt"), "kept and edited\n").unwrap();
    run("git", &["commit", "-q", "-am", "ours"], repo);
    let ours = run("git", &["rev-parse", "HEAD"], repo);

    let out = llm_edit(&["git-merge", "theirs"], repo);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(run("git", &["rev-parse", "HEAD"], repo), ours);
    assert_eq!(run("git", &["rev-parse", "MERGE_HEAD"], repo), theirs);
    let unmerged = run("git", &["ls-files", "-u"], repo);
    let stages = |path: &str| -> Vec<String> {
        unmerged
            .lines()
            .filter(|l| l.ends_with(&format!("\t{}", path)))
            .map(|l| l.split_whitespace().nth(2).unwrap().to_string())
            .collect()
    };
    assert_eq!(stages("vendor/sub"), ["1", "2", "3"]);
    assert_eq!(stages("old.txt"), ["1", "2"]);
    assert_eq!(
        fs::read_to_string(repo.join("old.txt")).unwrap(),
        "kept and edited\n"
    );
}

#[test]
fn git_merge_refuses_to_overwrite_local_changes() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    init_repo(repo);

    run("git", &["checkout", "-q", "-b", "theirs"], repo);
    fs::write(repo.join("notes.md"), "theirs\n").unwrap();
    run("git", &["commit", "-q", "-am", "theirs"], repo);

    run("git", &["checkout", "-q", "main"], repo);
    fs::write(repo.join("app.py"), "def get():\n    return 2\n").unwrap();
    run("git", &["commit", "-q", "-am", "ours"], repo);
    let ours = run("git", &["rev-parse", "HEAD"], repo);
    fs::write(repo.join("notes.md"), "uncommitted\n").unwrap();

    let out = llm_edit(&["git-merge", "theirs"], repo);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("notes.md"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(run("git", &["rev-parse", "HEAD"], repo), ours);
    assert_eq!(
        fs::read_to_string(repo.join("notes.md")).unwrap(),
        "uncommitted\n"
    );
}

#[test]
fn git_rebase_replays_submodule_binary_and_delete() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    init_repo(repo);

    run("git", &["checkout", "-q", "-b", "feature"], repo);
    set_gitlink(repo, "vendor/sub", SUB_B);
    fs::write(repo.join("logo.bin"), THEIR_BIN).unwrap();
    run("git", &["rm", "-q", "old.txt"], repo);
    run("git", &["add", "logo.bin"], repo);
    run("git", &["commit", "-q", "-m", "feature"], repo);

    run("git", &["checkout", "-q", "main"], repo);
    fs::write(repo.join("app.py"), "def get():\n    return 2\n").unwrap();
    run("git", &["commit", "-q", "-am", "main moves on"], repo);
    let main = run("git", &["rev-parse", "HEAD"], repo);

    run("git", &["checkout", "-q", "feature"], repo);
    let out = llm_edit(&["git-rebase", "main"], repo);
    assert!(
        out.status.success(),
        "git-rebase failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(run("git", &["rev-parse", "HEAD^"], repo), main);
    assert_eq!(
        ls_tree(repo, "HEAD", "vendor/sub"),
        format!("160000 commit {}", SUB_B)
    );
    assert_eq!(git_bytes(&["show", "HEAD:logo.bin"], repo), THEIR_BIN);
    assert_eq!(git_bytes(&["show", "HEAD:photo.bin"], repo), PHOTO);
    assert!(ls_tree(repo, "HEAD", "link.py").starts_with("120000 blob"));
    assert_eq!(ls_tree(repo, "HEAD", "old.txt"), "");
    assert_eq!(
        run("git", &["show", "HEAD:app.py"], repo),
        "def get():\n    return 2"
    );
}

#[test]
fn git_merge_takes_one_sided_changes_verbatim() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    init_repo(repo);
    // Already unparseable before the merge, like an HTML fixture or
    // generated code: the merge must not call it a conflict.
    let broken_base = "def f(:\n    return 1\n";
    let broken_theirs = "def f(:\n    return 2\n\nclass {\n";
    fs::write(repo.join("broken.py"), broken_base).unwrap();
    run("git", &["add", "broken.py"], repo);
    run("git", &["commit", "-q", "-m", "broken"], repo);

    run("git", &["checkout", "-q", "-b", "theirs"], repo);
    fs::write(repo.join("broken.py"), broken_theirs).unwrap();
    run("git", &["commit", "-q", "-am", "theirs"], repo);

    run("git", &["checkout", "-q", "main"], repo);
    fs::write(repo.join("app.py"), "def get():\n    return 2\n").unwrap();
    run("git", &["commit", "-q", "-am", "ours"], repo);

    let out = llm_edit(&["git-merge", "theirs"], repo);
    assert!(
        out.status.success(),
        "git-merge failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read_to_string(repo.join("broken.py")).unwrap(),
        broken_theirs
    );
    assert_eq!(
        run("git", &["rev-parse", "HEAD:broken.py"], repo),
        run("git", &["rev-parse", "theirs:broken.py"], repo)
    );
}
