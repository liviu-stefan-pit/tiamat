# Fake Cursor CLI modes for Tiamat tests (P03+)
#
# Set TIAMAT_CURSOR_CLI to fixtures/cursor-cli/fake-agent.cmd (Windows) or
# fixtures/cursor-cli/fake-agent.sh (Unix), and select behavior with
# TIAMAT_FAKE_CLI_MODE.
#
# Modes:
# - success
# - nonzero_exit
# - malformed_mixed
# - silent_hang
# - chatty_hang
# - child_tree
# - ignore_terminate
# - partial_timeout
# - resume_success
# - model_unavailable
# - auth_failure
# - flood_oversized
# - secret_echo
# - architect_valid (plan-mode JSON plan; requires --mode plan, rejects --force)
# - architect_invalid (schema/semantic-invalid plan)
# - architect_repairable (invalid first, valid on --resume)
# - architect_no_sol (valid plan; model list omits SOL; architect prefers Grok High)
# - impl_success (writes src/feature.ts + immutable phase-result; ensures gate scripts)
# - impl_fail_tests (writes feature but unit gate exits 1)
# - impl_escape (writes ESCAPE_PROOF.txt outside write root)
# - impl_timeout_partial (partial src/partial.ts then hang)
# - impl_resume (completes feature after resume)
#
# Implementation env overrides:
# - TIAMAT_FAKE_WRITE_ROOT
# - TIAMAT_FAKE_MANAGED_RUN_ROOT
# - TIAMAT_FAKE_PHASE_ID
#
# Architect plan env overrides:
# - TIAMAT_FAKE_PLAN_RUN_ID
# - TIAMAT_FAKE_PLAN_PROJECT_ID
# - TIAMAT_FAKE_PLAN_WRITE_ROOT
# - TIAMAT_FAKE_PLAN_READ_ROOT
#
# Probe flags handled without spending: --version, --help, --list-models, status, whoami
