#!/usr/bin/env node
/**
 * Integration validator for documented commands, links, secrets, and hashes.
 * Runs against the repo docs — no paid Cursor calls.
 */
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  assertPackageHashesMatch,
  extractFencedCommands,
  extractMarkdownLinks,
  findForbiddenSecrets,
  normalizeNpmRunCommand,
  parseDocsManifest,
  validateDocumentedNpmScripts,
  validateRelativeLinks,
} from "../../tools/docs/docs-tooling.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../..");

function readJson(rel) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, rel), "utf8"));
}

function walkTextFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === "dist") continue;
      out.push(...walkTextFiles(full));
    } else if (/\.(tsx?|jsx?|css|md)$/.test(entry.name)) {
      out.push(fs.readFileSync(full, "utf8"));
    }
  }
  return out;
}

function main() {
  const errors = [];
  const manifestRaw = readJson("docs/config/docs-manifest.json");
  const parsed = parseDocsManifest(manifestRaw);
  if (!parsed.ok) {
    console.error("manifest parse failed:", parsed.errors);
    process.exit(1);
  }
  const manifest = parsed.value;
  const packageJson = readJson("package.json");

  errors.push(...validateDocumentedNpmScripts(manifest, packageJson));

  const scriptNames = new Set(Object.keys(packageJson.scripts ?? {}));
  for (const guide of manifest.guides) {
    const abs = path.join(repoRoot, guide.path);
    if (!fs.existsSync(abs)) {
      errors.push(`missing guide file: ${guide.path}`);
      continue;
    }
    const text = fs.readFileSync(abs, "utf8");
    for (const secret of findForbiddenSecrets(
      text,
      manifest.forbiddenExamples ?? [],
    )) {
      errors.push(`forbidden secret example in ${guide.path}: ${secret}`);
    }
    errors.push(
      ...validateRelativeLinks(
        guide.path,
        extractMarkdownLinks(text),
        (target) => fs.existsSync(target),
        (fromFile, href) =>
          path.resolve(path.dirname(path.join(repoRoot, fromFile)), href),
      ),
    );
    for (const cmd of extractFencedCommands(text)) {
      const name = normalizeNpmRunCommand(cmd);
      if (name && !scriptNames.has(name)) {
        errors.push(
          `missing npm script referenced in ${guide.path}: ${cmd} (${name})`,
        );
      }
    }
  }

  const srcBlob = [
    ...walkTextFiles(path.join(repoRoot, "src")),
    ...walkTextFiles(path.join(repoRoot, "e2e")),
  ].join("\n");
  for (const sel of manifest.uiSelectors ?? []) {
    if (!srcBlob.includes(sel)) {
      errors.push(`ui selector not found in src/e2e: ${sel}`);
    }
  }

  const sumsPath = path.join(repoRoot, "artifacts/packages/SHA256SUMS.txt");
  if (fs.existsSync(sumsPath)) {
    errors.push(
      ...assertPackageHashesMatch(
        manifest.packageHashes,
        fs.readFileSync(sumsPath, "utf8"),
      ),
    );
  } else {
    const hashesDoc = fs.readFileSync(
      path.join(repoRoot, "docs/release/PACKAGE-HASHES.md"),
      "utf8",
    );
    for (const [name, hash] of Object.entries(manifest.packageHashes)) {
      if (!hashesDoc.includes(hash) || !hashesDoc.includes(name)) {
        errors.push(`PACKAGE-HASHES.md missing ${name} / ${hash}`);
      }
    }
  }

  const required = [
    "test:docs",
    "demo",
    "testbench:materialize",
    "test:e2e",
    "package",
  ];
  for (const name of required) {
    if (!packageJson.scripts?.[name]) {
      errors.push(`required script missing/empty: ${name}`);
    }
  }

  try {
    const version = readVersionViaDocumentedNpm();
    if (version !== packageJson.version) {
      errors.push(
        `npm pkg get version mismatch: ${version} vs ${packageJson.version}`,
      );
    }
  } catch (err) {
    errors.push(`documented command integration failed: ${err}`);
  }

  // Fixture integration: materialize script and fake CLI exist as documented.
  const fakeCli = path.join(repoRoot, "fixtures/cursor-cli/fake-agent.cmd");
  const executor = path.join(repoRoot, "fixtures/testbench/executor-app");
  if (!fs.existsSync(fakeCli))
    errors.push("missing fixtures/cursor-cli/fake-agent.cmd");
  if (!fs.existsSync(executor))
    errors.push("missing fixtures/testbench/executor-app");

  if (errors.length) {
    console.error("docs validation failed:");
    for (const e of errors) console.error(" -", e);
    process.exit(1);
  }

  const report = {
    ok: true,
    version: manifest.version,
    guides: manifest.guides.length,
    documentedCommands: manifest.documentedCommands.length,
    signing: manifest.signing,
    checkedAtUtc: new Date().toISOString(),
  };
  const outDir = path.join(repoRoot, "artifacts/docs");
  fs.mkdirSync(outDir, { recursive: true });
  fs.writeFileSync(
    path.join(outDir, "docs-validation.json"),
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report, null, 2));
}

function readVersionViaDocumentedNpm() {
  const opts = {
    cwd: repoRoot,
    encoding: "utf8",
    env: process.env,
    windowsHide: true,
  };
  if (process.platform === "win32") {
    return execFileSync(
      "cmd.exe",
      ["/d", "/s", "/c", "npm pkg get version"],
      opts,
    )
      .trim()
      .replace(/"/g, "");
  }
  return execFileSync("npm", ["pkg", "get", "version"], opts)
    .trim()
    .replace(/"/g, "");
}

main();
