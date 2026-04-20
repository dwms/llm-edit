#!/usr/bin/env bash
# Claude Code PostToolUse hook: after any Edit/Write touches a source
# file, run `llm-edit infer` to backfill structured ops from the
# resulting diff. This keeps .llm-edit/log.jsonl live without the
# agent ever having to think about subcommands.
#
# Best-effort: silent if the binary isn't built or infer fails.

set -e

INPUT=$(cat)
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
LLM_BIN="${LLM_EDIT_BIN:-$REPO_ROOT/target/release/llm-edit}"

[[ -x "$LLM_BIN" ]] || exit 0

FILE=$(python3 - "$INPUT" <<'PY'
import json, sys
ti = (json.loads(sys.argv[1]).get("tool_input", {}) or {})
print(ti.get("file_path", "") or "")
PY
)

case "${FILE##*.}" in
  py|js|jsx|mjs|cjs|ts|tsx|rs) ;;
  *) exit 0 ;;
esac

# infer walks `git diff --name-only HEAD` itself; we just kick it.
"$LLM_BIN" --cwd "$REPO_ROOT" infer >/dev/null 2>&1 || true
exit 0
