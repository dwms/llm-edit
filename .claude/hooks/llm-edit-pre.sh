#!/usr/bin/env bash
# Claude Code PreToolUse hook: intercept Edit/Write calls whose intent
# is unambiguous (replace_all rename of a bare identifier; Write to a
# non-existent path) and route them through llm-edit so the structured
# op is recorded with full intent rather than guessed from a diff.
#
# Ambiguous edits are passed through; the post hook runs `llm-edit
# infer` to backfill ops from the resulting diff.
#
# Receives JSON on stdin with tool_name, tool_input fields.
# Exit 0   — let Claude run the tool normally.
# Exit 2   — tell Claude the tool was handled here; stderr is shown
#            to the agent as the reason.

set -e

INPUT=$(cat)
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
LLM_BIN="${LLM_EDIT_BIN:-$REPO_ROOT/target/release/llm-edit}"

# If the binary isn't built, do nothing — first-time setup needs to
# build before hooks can help.
[[ -x "$LLM_BIN" ]] || exit 0

read TOOL FILE OLD NEW REPLACE_ALL CONTENT < <(python3 - "$INPUT" <<'PY'
import json, sys
d = json.loads(sys.argv[1])
ti = d.get("tool_input", {}) or {}
def s(v): return "" if v is None else str(v).replace("\t", " ").replace("\n", " ")
print(
    s(d.get("tool_name", "")),
    s(ti.get("file_path", "")),
    s(ti.get("old_string", "")),
    s(ti.get("new_string", "")),
    s(ti.get("replace_all", "")),
    "<<HAS_CONTENT>>" if "content" in ti else "",
    sep="\t",
)
PY
)

# Re-extract content / strings without the lossy tab-flattening above
# when we actually need their literal value.
extract() {
  python3 - "$INPUT" "$1" <<'PY'
import json, sys
print((json.loads(sys.argv[1]).get("tool_input", {}) or {}).get(sys.argv[2], "") or "", end="")
PY
}

case "$TOOL" in
  Write)
    [[ -z "$FILE" ]] && exit 0
    # Only intercept when the target file does not yet exist — that's
    # an unambiguous add_file. Overwrites of existing files fall
    # through to inference.
    if [[ ! -e "$FILE" ]]; then
      CONTENT="$(extract content)"
      if "$LLM_BIN" add-file --file "$FILE" --content "$CONTENT" >&2; then
        echo "llm-edit add-file recorded the op and created $FILE; the Write was skipped to avoid double-application." >&2
        exit 2
      fi
    fi
    ;;
  Edit)
    # Only intercept whole-identifier renames with replace_all=true.
    # That's the one case where the Edit input carries intent the
    # post-hoc diff cannot recover (N independent identical text
    # replacements look identical to a project-scope rename).
    if [[ "$REPLACE_ALL" == "True" || "$REPLACE_ALL" == "true" ]]; then
      OLD="$(extract old_string)"
      NEW="$(extract new_string)"
      if [[ "$OLD" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] && [[ "$NEW" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] && [[ -n "$FILE" ]]; then
        if "$LLM_BIN" rename-symbol --file "$FILE" --scope module "$OLD" "$NEW" >&2; then
          echo "llm-edit rename-symbol recorded the op and applied the rename in $FILE; the Edit was skipped." >&2
          exit 2
        fi
      fi
    fi
    ;;
esac

exit 0
