#!/usr/bin/env bash
# Claude Code PreToolUse hook on Bash: when the agent runs
# `git merge ...` or `git rebase ...`, block the call and tell it to
# use `llm-edit git-merge` / `llm-edit git-rebase` instead. Those are
# the entry points that read both sides' op logs and merge
# semantically; raw git would lose the structured-op path.
#
# Escape hatch: prefix the command with `LLM_EDIT_BYPASS=1` to run
# raw git (needed for e.g. `git merge --abort` recovery).
#
# Exit 0 — pass the command through.
# Exit 2 — block; stderr is shown to the agent as the reason.

set -e

INPUT=$(cat)

CMD=$(python3 - "$INPUT" <<'PY'
import json, sys
print((json.loads(sys.argv[1]).get("tool_input", {}) or {}).get("command", "") or "", end="")
PY
)

# Bypass: explicit opt-out via env-var prefix.
if [[ "$CMD" == LLM_EDIT_BYPASS=1* ]]; then
  exit 0
fi

# Already routed through llm-edit (or invoking it directly) — let it through.
if [[ "$CMD" == *llm-edit* ]]; then
  exit 0
fi

# Match `git merge` or `git rebase` as a command (start of line, or
# after a shell separator). Avoids matching e.g. `git mergetool` or
# substrings inside quoted strings used as data.
if [[ "$CMD" =~ (^|[\;\&\|][[:space:]]*)git[[:space:]]+merge([[:space:]]|$) ]]; then
  cat >&2 <<EOF
Use \`llm-edit git-merge\` instead of \`git merge\` so the structured-op merger runs and both sides' .llm-edit/log.jsonl are composed semantically. Raw git merge would fall back to text diff3 and lose the op log path.

  llm-edit git-merge BRANCH

Escape hatch (e.g. for --abort): prefix with LLM_EDIT_BYPASS=1.
EOF
  exit 2
fi

if [[ "$CMD" =~ (^|[\;\&\|][[:space:]]*)git[[:space:]]+rebase([[:space:]]|$) ]]; then
  cat >&2 <<EOF
Use \`llm-edit git-rebase\` instead of \`git rebase\` so each replayed commit is composed via the structured-op merger.

  llm-edit git-rebase ONTO          # or --continue / --abort / -i

Escape hatch: prefix with LLM_EDIT_BYPASS=1.
EOF
  exit 2
fi

exit 0