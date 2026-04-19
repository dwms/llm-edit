#!/usr/bin/env bash
# Claude Code PreToolUse hook: when the agent uses Edit on a source
# file, remind it that llm-edit subcommands record ops for semantic
# merging. Does NOT block — just nudges.
#
# Receives JSON on stdin with tool_name, tool_input fields.

set -e

INPUT=$(cat)
TOOL=$(echo "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tool_name',''))" 2>/dev/null || echo "")

if [ "$TOOL" != "Edit" ] && [ "$TOOL" != "Write" ]; then
  exit 0
fi

FILE=$(echo "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tool_input',{}).get('file_path',''))" 2>/dev/null || echo "")
EXT="${FILE##*.}"

case "$EXT" in
  py|js|jsx|ts|tsx|rs|css|html|toml)
    echo "Tip: consider using llm-edit subcommands (rename-symbol, replace-in-anchor, add-function, etc.) instead of raw Edit — they record ops for semantic merging. See CLAUDE.md for the mapping."
    ;;
esac
