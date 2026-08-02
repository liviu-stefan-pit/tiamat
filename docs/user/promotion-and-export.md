# Isolated output promotion and export

Successful work stays in Tiamat-owned managed clones/branches until you explicitly promote or export it. Sources are never force-pushed, rewritten, or deleted by Tiamat.

## What you see

- **Isolated workspace** panel: managed root, per-project write roots, promotion status (`unpromoted` / `exported` / `promoted`), checkpoint count, source fingerprint status, **Export** / **Promote** actions.
- **Open output** control: opens the managed output path when available.
- **Completion summary**: promotion instructions, cleanup confirmation, and the same **Export** / **Promote** actions.

## Export (safe portable copy)

Export copies a managed project to a destination you choose and writes `tiamat-export.json` metadata. The source repository is not modified. After export, promotion status becomes `exported`.

## Promote

Promotion records that you have accepted the managed result for merge/use outside Tiamat. Tiamat does **not** automatically push branches, open PRs, or merge into your original repository — that remains your decision in your own VCS tools.

## Uninstall retention

**Policy:** uninstall must retain unpromoted managed workspaces under the app workspaces root (`%APPDATA%\com.tiamat.desktop\tiamat\workspaces\`). Promote or export first if you need a durable copy outside the managed tree.

**Unsigned-dev:** installer retain hooks may not be fully wired yet; see [../contributor/packaging.md](../contributor/packaging.md). Do not assume AppData wipe is blocked until a release explicitly ships and verifies those hooks.

## Next

[Containment limits](containment-limits.md)
