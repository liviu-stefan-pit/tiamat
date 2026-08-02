#!/usr/bin/env bash
# Unix wrapper for the deterministic fake Cursor CLI.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
exec node "$SCRIPT_DIR/fake-agent.mjs" "$@"
