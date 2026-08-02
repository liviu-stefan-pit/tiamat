#!/usr/bin/env node
/**
 * Sync version across package.json and src-tauri/tauri.conf.json.
 * Usage: node scripts/version.mjs 0.2.0
 */
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+/.test(version)) {
  console.error("Usage: node scripts/version.mjs <semver>");
  process.exit(1);
}

const repoRoot = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const pkgPath = join(repoRoot, "package.json");
const tauriPath = join(repoRoot, "src-tauri", "tauri.conf.json");
const cargoPath = join(repoRoot, "src-tauri", "Cargo.toml");

const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
pkg.version = version;
writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`);

const tauri = JSON.parse(readFileSync(tauriPath, "utf8"));
if (tauri.version !== undefined) tauri.version = version;
if (tauri.package?.version !== undefined) tauri.package.version = version;
writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);

let cargo = readFileSync(cargoPath, "utf8");
cargo = cargo.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`);
writeFileSync(cargoPath, cargo);

console.log(`Version set to ${version}`);
