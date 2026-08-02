# Tiamat

Tiamat turns rough project material into a tested implementation on Windows using a Tauri 2 desktop shell, Rust core, and React/TypeScript UI.

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
| `npm test` | Rust workspace + frontend unit + contract integration tests |
| `npm run test:rust` | `cargo test --workspace` |
| `npm run test:frontend` | Vitest unit tests |
| `npm run test:contracts` | Contract fixture integration tests |
| `npm run test:e2e` | Playwright dev-host launch smoke |
| `npm run fmt` | `cargo fmt` + Prettier |
| `npm run lint` | `clippy` + TypeScript check |
| `npm run ci` | CI-equivalent local verification (no E2E) |

## Repository layout

- `src/` — React/TypeScript UI (`domain`, `features`, `lib/tauri`)
- `src-tauri/` — Tauri host and Rust module stubs (`app`, `contracts`, `scheduler`, …)
- `crates/tiamat-contracts/` — versioned domain contracts and JSON Schema validation
- `schemas/` — canonical JSON Schemas
- `fixtures/contracts/` — valid and invalid compatibility fixtures
- `e2e/` — Playwright smoke tests against the dev host

P00 keeps orchestration as a fake/no-op scheduler stub. Real scheduling lands in later phases.

## Testing policy

Automated tests use deterministic fixtures only. No paid or live Cursor CLI calls are made in CI or default local test commands.

## Canonical plan

Implementation phases and acceptance evidence live in `MASTER-PLAN.md`.
