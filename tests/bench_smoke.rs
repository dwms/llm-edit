// Smoke test for the bench harness: build a tiny synthetic corpus
// (one positive case, one negative case) in a tempdir, invoke the
// `bench` binary against it, and assert the output table reflects the
// expected scoring.
//
// Cases:
//   - Positive: a one-line rename that llm-edit handles via structured
//     ops but diff3 handles cleanly too (both CORRECT).
//   - Negative: a conflicting rename that llm-edit flags as a conflict
//     (matches conflict.txt) but diff3 silently merges clean one way
//     (diff3 WRONG, llm-edit CORRECT).

use std::fs;
use std::path::Path;
use std::process::Command;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn make_positive_case(root: &Path) {
    // Bucket/case layout: <root>/bucket-positive/case1/{base,agent_a,agent_b,expected}/app.py
    let case = root.join("bucket-positive").join("case1");
    let base = "def get_user(user_id):\n    return user_id\n";
    let a_text = "def get_user(account_id):\n    return account_id\n";
    let b_text = "def get_user(account_id):\n    return account_id\n";
    let expected = "def get_user(account_id):\n    return account_id\n";
    write(&case.join("base/app.py"), base);
    write(&case.join("agent_a/app.py"), a_text);
    write(&case.join("agent_b/app.py"), b_text);
    write(&case.join("expected/app.py"), expected);

    let log_a = "{\"op\":\"rename_symbol\",\"scope\":\"module\",\"file\":\"app.py\",\"old\":\"user_id\",\"new\":\"account_id\",\"kinds\":[\"parameter\",\"identifier\"]}\n";
    // B applies the same rename — both agents converge.
    let log_b = "{\"op\":\"rename_symbol\",\"scope\":\"module\",\"file\":\"app.py\",\"old\":\"user_id\",\"new\":\"account_id\",\"kinds\":[\"parameter\",\"identifier\"]}\n";
    write(&case.join("agent_a/.llm-edit/log.jsonl"), log_a);
    write(&case.join("agent_b/.llm-edit/log.jsonl"), log_b);
}

fn make_negative_case(root: &Path) {
    // Bucket/case layout: <root>/bucket-negative/case1/{base,agent_a,agent_b}/app.py + conflict.txt
    let case = root.join("bucket-negative").join("case1");
    let base = "def get_user(user_id):\n    return user_id\n";
    // A renames to account_id; B renames to member_id — conflicting.
    let a_text = "def get_user(account_id):\n    return account_id\n";
    let b_text = "def get_user(member_id):\n    return member_id\n";
    write(&case.join("base/app.py"), base);
    write(&case.join("agent_a/app.py"), a_text);
    write(&case.join("agent_b/app.py"), b_text);

    let log_a = "{\"op\":\"rename_symbol\",\"scope\":\"module\",\"file\":\"app.py\",\"old\":\"user_id\",\"new\":\"account_id\",\"kinds\":[\"parameter\",\"identifier\"]}\n";
    let log_b = "{\"op\":\"rename_symbol\",\"scope\":\"module\",\"file\":\"app.py\",\"old\":\"user_id\",\"new\":\"member_id\",\"kinds\":[\"parameter\",\"identifier\"]}\n";
    write(&case.join("agent_a/.llm-edit/log.jsonl"), log_a);
    write(&case.join("agent_b/.llm-edit/log.jsonl"), log_b);

    let conflict = "{\"kind\":\"rename_symbol_mismatch\",\"file\":\"app.py\",\"old\":\"user_id\",\"a_new\":\"account_id\",\"b_new\":\"member_id\"}\n";
    write(&case.join("conflict.txt"), conflict);
}

#[test]
fn bench_on_synthetic_corpus_scores_correctly() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    make_positive_case(root);
    make_negative_case(root);

    let bin = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(bin).arg(root).output().expect("spawn bench");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Expect two buckets and a TOTAL line. llm-edit should be 2/2,
    // diff3 should be 1/2 (positive clean, negative silently merged
    // from one side without conflict markers).
    assert!(
        stdout.contains("bucket-positive"),
        "stdout missing positive bucket:\n{}\n--- stderr ---\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bucket-negative"),
        "stdout missing negative bucket:\n{}",
        stdout
    );
    assert!(
        stdout.contains("TOTAL"),
        "stdout missing TOTAL line:\n{}",
        stdout
    );
    // llm-edit should nail both cases:
    assert!(
        stdout.contains("2/2"),
        "expected llm-edit 2/2 somewhere:\n{}",
        stdout
    );
    // diff3 should get at best 1/2 (the negative should NOT be 2/2 diff3-wise).
    // We assert the gap is >= 0.50 (one case gap / 2 total) — plenty to
    // confirm the scorer distinguishes the two tools.
    let gap_line = stdout
        .lines()
        .find(|l| l.starts_with("TOTAL"))
        .expect("TOTAL line present");
    assert!(
        gap_line.contains("gap=+0.50")
            || gap_line.contains("gap=+1.00")
            || gap_line.contains("gap=+0."),
        "unexpected TOTAL line: {}",
        gap_line
    );
}

#[test]
fn bench_on_empty_corpus_prints_message_and_exits_zero() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bin = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(bin)
        .arg(tmp.path())
        .output()
        .expect("spawn bench");
    assert!(output.status.success(), "empty corpus should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("no benchmark corpus yet"),
        "missing empty-corpus message: {}",
        stdout
    );
}
