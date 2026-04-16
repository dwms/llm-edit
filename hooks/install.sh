#!/usr/bin/env bash
# Point this repo's git at hooks/. Idempotent — re-running just resets
# the config to the same value.
set -e
cd "$(dirname "$0")/.."
git config core.hooksPath hooks
chmod +x hooks/pre-commit hooks/prepare-commit-msg hooks/post-commit hooks/post-merge
echo "hooks installed (core.hooksPath = hooks)"
