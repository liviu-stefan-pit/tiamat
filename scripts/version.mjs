#!/usr/bin/env node
/**
 * Sync version across package.json, tauri.conf.json, and Cargo workspace.
 * Usage: node scripts/version.mjs 0.2.0
 */
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("Usage: node scripts/version.mjs <semver>   e.g. 0.2.0");
  process.exit(1);
}

const repoRoot = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const pkgPath = join(repoRoot, "package.json");
const tauriPath = join(repoRoot, "src-tauri", "tauri.conf.json");
const workspaceCargoPath = join(repoRoot, "Cargo.toml");

const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
pkg.version = version;
writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`);

const tauri = JSON.parse(readFileSync(tauriPath, "utf8"));
tauri.version = version;
writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);

let workspace = readFileSync(workspaceCargoPath, "utf8");
if (!/^\[workspace\.package\][\s\S]*?^version\s*=/m.test(workspace)) {
  console.error("Could not find [workspace.package] version in Cargo.toml");
  process.exit(1);
}
workspace = workspace.replace(
  /(\[workspace\.package\][\s\S]*?^version\s*=\s*)"[^"]+"/m,
  `$1"${version}"`,
);
writeFileSync(workspaceCargoPath, workspace);

console.log(`Version set to ${version}`);
console.log("Next: commit, then tag and push:");
console.log(`  git tag v${version}`);
console.log(`  git push origin main --tags`);
