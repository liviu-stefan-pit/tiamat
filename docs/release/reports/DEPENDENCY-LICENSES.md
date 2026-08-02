# Dependency licenses

Product license: **MIT** (`LICENSE`).

Generated for Tiamat **0.1.0** on **2026-08-02**. Direct dependencies only; transitive crates inherit their upstream licenses from crates.io / npm.

## First-party

| Component | License |
|---|---|
| Tiamat (this repository) | MIT |

## JavaScript / TypeScript (direct)

| Package | Role | Declared license (upstream) |
|---|---|---|
| `@tauri-apps/api` | runtime | Apache-2.0 OR MIT |
| `@tauri-apps/plugin-global-shortcut` | runtime | Apache-2.0 OR MIT |
| `@tauri-apps/plugin-opener` | runtime | Apache-2.0 OR MIT |
| `@xyflow/react` | runtime | MIT |
| `react` / `react-dom` | runtime | MIT |
| `@tauri-apps/cli` | dev | Apache-2.0 OR MIT |
| `vite` / `@vitejs/plugin-react` | dev | MIT |
| `vitest` | dev | MIT |
| `@playwright/test` | dev | Apache-2.0 |
| `typescript` | dev | Apache-2.0 |
| `prettier` | dev | MIT |
| Testing Library / jest-axe / jsdom | dev | MIT |

## Rust (workspace direct)

| Crate | Role | Declared license (upstream) |
|---|---|---|
| `serde` / `serde_json` | serialize | MIT OR Apache-2.0 |
| `thiserror` | errors | MIT OR Apache-2.0 |
| `uuid` | ids | Apache-2.0 OR MIT |
| `chrono` | time | MIT OR Apache-2.0 |
| `jsonschema` | validation | MIT |
| `rusqlite` | SQLite | MIT |
| `sha2` / `hex` | hashing | MIT OR Apache-2.0 |
| `tempfile` | tests | MIT OR Apache-2.0 |
| `tauri` (+ plugins) | desktop shell | Apache-2.0 OR MIT |
| `tokio` | async runtime | MIT |

## Disposition

All listed direct dependencies use OSI-approved permissive licenses compatible with distributing Tiamat under MIT. No GPL/AGPL direct dependencies were introduced for 0.1.0. Regenerating a full transitive SBOM is recommended before a public signed release (P13+).
