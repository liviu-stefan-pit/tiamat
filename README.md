# Tiamat

Tiamat is a desktop app that turns rough project material (notes, folders, half-built repos) into a tested implementation.

You pick what to build from, pick where the result should go, and hit **Run**. Tiamat plans the work, drives Cursor agents through the phases, runs tests, and streams everything into a live log. Your original files stay untouched.

## What it does

1. **Intake** — You give it files or folders. Tiamat scans them (projects, languages, warnings, blockers) and asks you to acknowledge trust before anything runs.
2. **Architect** — One planning pass with Cursor **Grok High** produces a phase plan under `.tiamat/` in a managed copy of your work.
3. **Implement** — Independent phases run as Cursor agents (Composer for tiny/mechanical work; Grok Low → Medium → High for everything else). Agents write only inside a managed run folder you chose.
4. **Verify** — Unit / integration / e2e gates from the plan must pass before a phase is marked done. Failures escalate model effort or stop with a clear log.
5. **Log** — Structured events (plan, phases, stdout/stderr) appear live so you can watch and stop at any time.

## How it works

```text
Input (your sources)  →  preflight + trust
Output folder         →  managed run-{id}/ copy (sources stay read-only)
Architect (Grok High) →  plan.json + MASTER-PLAN.md
Scheduler (DAG)       →  non-overlapping phases in parallel when safe
Cursor agents         →  implement + tests inside the managed tree
Live log              →  events streamed from the Rust process host
```

Under the hood: Tauri 2 desktop shell, Rust orchestrator (process host, DAG scheduler, verification), React UI with three panes — **Input**, **Output**, **Log**.

Containment is “normal mode”: owned copies, process groups / Job Objects, and allowlists — not a hostile-code sandbox. See [docs/user/containment-limits.md](docs/user/containment-limits.md).

## What you need

| Requirement | Details |
|---|---|
| OS | **Windows 10/11** or **Linux** (Ubuntu 24.04–class with WebKitGTK 4.1) |
| Cursor | [Cursor](https://cursor.com/) installed, with the `agent` CLI on `PATH` and signed in |
| Models | Cursor **Composer** and **Grok** (Tiamat does not use SOL or other families) |
| Account | Live runs use your Cursor account / spend; demos can use the fake CLI instead |

For building from source you also need:

- [Node.js](https://nodejs.org/) 20+
- [Rust stable](https://rustup.rs/) (`cargo`, `rustfmt`, `clippy`)
- Windows: [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (usually already installed)
- Linux: `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libgtk-3-dev`, `patchelf`

## Get started (users)

### Install a release build

1. Download the latest package from [GitHub Releases](https://github.com/liviu-stefan-pit/tiamat/releases) (Windows NSIS/MSI or Linux AppImage/deb).
2. Install / run it and launch **Tiamat**.
3. Confirm Cursor’s `agent` CLI works in a terminal (`agent --version` or equivalent).

### First real run

1. **Input** — Drop or pick the folder/files to build from. Review blockers/warnings and check the trust box.
2. **Output** — Choose a folder. Tiamat creates `run-{uuid}/` under it; that is the only place agents write.
3. Click **Run**. Watch the **Log**. Use **Stop** to cancel.

When it finishes, open the managed run folder under your output path. Promote or copy out what you want — sources you selected as input were never mutated.

More detail: [docs/user/first-run.md](docs/user/first-run.md) · [docs/user/install.md](docs/user/install.md)

## Get started (from source)

```bash
git clone https://github.com/liviu-stefan-pit/tiamat.git
cd tiamat
npm run setup          # deps + toolchain check + local CI suite
npm run tauri:dev      # open the desktop app
```

Practice without spending Cursor quota (fake agent):

```bash
# Point Tiamat at the deterministic fake CLI, then use fixtures/testbench/executor-app as input
export TIAMAT_CURSOR_CLI="node|$(pwd)/fixtures/cursor-cli/fake-agent.mjs"   # Linux/macOS-style
npm run tauri:dev
```

On Windows you can use `fixtures/cursor-cli/fake-agent.cmd` the same way, or run `npm run demo` for the scripted fake-CLI story.

## Models (short)

| Role | Model |
|---|---|
| Architect | `cursor-grok-4.5-high` |
| Simple / mechanical phases | `composer-2.5` |
| Normal → hard work / reviews | `cursor-grok-4.5-low` → `medium` → `high` |

Timeouts default to **30 min** architect / **20 min** per phase (`TIAMAT_ARCHITECT_TIMEOUT_MS`, `TIAMAT_PHASE_TIMEOUT_MS`). Full policy: [docs/user/model-and-cost.md](docs/user/model-and-cost.md).

## Useful commands

| Command | Purpose |
|---|---|
| `npm run tauri:dev` | Run the desktop app in development |
| `npm run tauri:build` / `npm run package` | Build / stage installers under `artifacts/packages/` |
| `npm test` | Rust + frontend + contract tests (fake-only) |
| `npm run test:e2e` | Playwright three-pane smoke |
| `npm run ci` | Local CI gate (format, lint, tests, docs) |
| `npm run demo` | Deterministic fake-CLI demo (Windows-oriented script) |

CI builds Windows + Linux on every push/PR (`.github/workflows/ci.yml`).

### Publish installers

```bash
npm run version -- 0.2.0   # sync versions
# commit, then:
git tag v0.2.0
git push origin main --tags
```

That runs **Release**: creates a GitHub Release for the tag, builds NSIS/MSI (Windows) and deb/AppImage (Linux), and uploads all artifacts to that release. Details: [docs/contributor/release.md](docs/contributor/release.md).

## Docs

- User + contributor guides: [`docs/README.md`](docs/README.md)
- Known limits: [`docs/user/known-limits.md`](docs/user/known-limits.md)
- Release / signing: [`docs/release/`](docs/release/)
- Design history: [`MASTER-PLAN.md`](MASTER-PLAN.md)

License: [`LICENSE`](LICENSE) · Changelog: [`CHANGELOG.md`](CHANGELOG.md)
