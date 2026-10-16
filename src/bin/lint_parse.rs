// Pre-commit lint: take a list of file paths on the command line,
// run `parses_cleanly` on each (using the same tree-sitter grammars
// the merger uses), exit non-zero if any fail. Files with extensions
// we don't have a grammar for are silently skipped.
//
// Used by hooks/pre-commit to catch the kind of whitespace/structure
// corruption that an AST-merge bug would inject — keeps it from
// landing on master and metastasizing across subsequent merges.

use llm_edit::cli::parses_cleanly;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut bad = Vec::new();
    for arg in std::env::args().skip(1) {
        let path = PathBuf::from(&arg);
        let text = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("lint_parse: skip {}: {}", arg, e);
                continue;
            }
        };
        if !parses_cleanly(&path, &text) {
            bad.push(arg);
        }
    }
    if bad.is_empty() {
        ExitCode::from(0)
    } else {
        eprintln!(
            "lint_parse: {} file(s) failed tree-sitter parse:",
            bad.len()
        );
        for p in &bad {
            eprintln!("  {}", p);
        }
        ExitCode::from(1)
    }
}
