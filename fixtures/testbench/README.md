# Tiamat TestBench (§15.3)

Shippable sample workspaces for deterministic packaging and acceptance.

| Case | Path | Purpose |
|---|---|---|
| Rough notes only | `notes-only/` | Brainstorm intake |
| Small git web app | `web-app/` | Single-project implement journey |
| Multi-project | `multi-project/` | Sibling repos under one intake root |
| Dirty repository | `dirty-git/` | Staged/unstaged overlay reconstruction |
| Nested repository | `nested-repo/` | Inner git under outer project |
| Secret-looking files | `secret-risk/` | Redaction / never leak fixtures |
| Junction escape | `junction-escape/` | Symlink/junction boundary denial |
| Unicode names | `unicode-项目/` | Non-ASCII path inventory |
| Long paths | `long-path/` | `\\?\` paths beyond MAX_PATH |
| Executor gates | `executor-app/` | Unit + integration + E2E gate scripts |

## Materialize

```powershell
npm run testbench:materialize
```

Creates git baselines, nested repos, a Windows junction escape attempt, and a generated long-path leaf under `fixtures/testbench/.generated/` (gitignored).

## One-command demo

```powershell
npm run demo
```

Runs the deterministic fake-CLI full story without paid model calls.
