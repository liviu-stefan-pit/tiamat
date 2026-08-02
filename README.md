# Tiamat

Tiamat turns rough project material into a tested implementation on Windows using a Tauri 2 desktop shell, Rust core, and React/TypeScript UI.

User and contributor documentation: [`docs/README.md`](docs/README.md)  
Release prep (hashes, signing, checklist): [`docs/release/`](docs/release/)  
Changelog: [`CHANGELOG.md`](CHANGELOG.md) · License: [`LICENSE`](LICENSE)

## Prerequisites

- Windows 10/11
- [Node.js](https://nodejs.org/) 20+
- [Rust stable](https://rustup.rs/) with `cargo`
- [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (usually preinstalled on Windows 11)

## Fresh clone setup

```powershell
git clone <repository-url> tiamat
cd tiamat
npm run setup
```

`npm run setup` installs npm dependencies, verifies Rust/Node, and runs the CI verification suite locally.

## Development

```powershell
npm run tauri:dev
```

## Root commands

| Command | Purpose |
|---|---|
| `npm run setup` | Scripted fresh-clone bootstrap and verification |
| `npm run tauri:dev` | Desktop dev shell (Tauri + Vite) |
| `npm run tauri:build` | Build desktop bundle |
| `npm run package` | Build NSIS/MSI, stage hashes under `artifacts/packages` |
| `npm run demo` | One-command deterministic TestBench demo (fake CLI only) |
| `npm run testbench:materialize` | Materialize git/junction/long-path TestBench cases |
| `npm test` | Rust workspace + frontend unit + contract integration tests |
| `npm run test:rust` | `cargo test --workspace` |
| `npm run test:frontend` | Vitest unit tests |
| `npm run test:contracts` | Contract fixture integration tests |
| `npm run test:e2e` | Playwright dev-host acceptance (fake-only) |
| `npm run test:docs` | Docs/config unit tests + documented-command integration checks |
| `npm run test:packaged` | Isolated-profile clean smoke (no contributor APPDATA mutation) |
| `npm run test:cleanup-proof` | Zero-owned-process cleanup proof |
| `npm run test:canary` | Spending-consented real Cursor contract canary (local-live only) |
| `npm run fmt` | `cargo fmt` + Prettier |
| `npm run lint` | `clippy` + TypeScript check |
| `npm run ci` | CI-equivalent local verification (no E2E, fake-only) |

## Packaging / VM

Windows NSIS + MSI targets are configured in `src-tauri/tauri.conf.json`. Install/upgrade/uninstall matrix documentation and scripts live under `scripts/vm/` and must run in a disposable snapshotted Windows VM (see that README for privileges, reboot policy, and retained artifacts).

## Testing policy

Automated tests and `npm run ci` / default E2E use deterministic fixtures only. No paid or live Cursor CLI calls are made in CI.

The release-gate local-live canary (`npm run test:canary`) requires explicit spending consent env vars and is version-gated; it is never part of deterministic CI.

## Canonical plan

Implementation phases and acceptance evidence live in `MASTER-PLAN.md`.
