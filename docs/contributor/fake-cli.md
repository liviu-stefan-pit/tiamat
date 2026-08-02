# Fake Cursor CLI

Deterministic fixtures under `fixtures/cursor-cli/`. Never spend against paid models in automated tests.

## Point Tiamat at the fake

```powershell
$env:TIAMAT_CURSOR_CLI = (Resolve-Path fixtures\cursor-cli\fake-agent.cmd)
$env:TIAMAT_FAKE_CLI_MODE = "success"
```

Or configure the absolute path in Settings → Cursor CLI path (used by the new-user journey and packaged smoke).

## Modes

See `fixtures/cursor-cli/README.md`. Highlights:

| Mode | Purpose |
|---|---|
| `success` | Happy-path stream + chat ID + usage |
| `architect_valid` / `architect_repairable` | Plan-mode architect |
| `impl_success` / `impl_fail_tests` / `impl_escape` | Phase executor gates |
| `silent_hang` / `chatty_hang` / `partial_timeout` | Watchdog / resume |
| `child_tree` / `ignore_terminate` | Job Object cleanup |
| `secret_echo` / `flood_oversized` | Redaction / limits |
| `auth_failure` / `model_unavailable` | Preflight failures |

## Probe surface

Fake handles `--version`, `--help`, `--list-models`, `status`, `whoami` without spending.
