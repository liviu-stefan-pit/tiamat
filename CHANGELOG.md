# Changelog

All notable changes to Tiamat are documented in this file.

## [0.1.0] — 2026-08-02

### Added

- Windows desktop shell (Tauri 2 + React/TypeScript + Rust) with SQLite WAL store and event replay.
- Intake, trust preflight, isolated owned workspaces, architect planner, DAG scheduler, and phase executor.
- Windows Job Object process host, watchdog, global abort (`Ctrl+Shift+F12`), and zero-owned-process cleanup proof.
- Recovery/security hardening, secret redaction, Normal-mode containment policy, and fault injection fixtures.
- Deterministic fake Cursor CLI, TestBench suite, one-command demo, NSIS/MSI packaging scripts, and disposable VM install matrix docs.
- End-user and contributor documentation with validated commands, release checklist, signing disposition, package hashes, license and vulnerability reports.

### Security

- Source inputs are never modified by unattended runs; fingerprints must remain unchanged.
- Unpromoted managed workspaces are retained across uninstall.
- Development packages ship unsigned (`unsigned-dev`); see `docs/release/SIGNING.md`.

### Known limits

See `docs/release/KNOWN-LIMITATIONS.md`.
