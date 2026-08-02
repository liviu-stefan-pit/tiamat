#!/usr/bin/env node
/**
 * Cross-platform packaging: run `tauri build`, stage bundles under artifacts/packages/,
 * and write SHA-256 sums + package-manifest.json.
 */
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  createReadStream,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const outDir = join(repoRoot, "artifacts", "packages");
mkdirSync(outDir, { recursive: true });

console.log("Building Tiamat packages…");
const env = {
  ...process.env,
  PATH: `${join(process.env.HOME || process.env.USERPROFILE || "", ".cargo", "bin")}${process.platform === "win32" ? ";" : ":"}${process.env.PATH || ""}`,
};
const build = spawnSync("npm", ["run", "tauri:build"], {
  cwd: repoRoot,
  env,
  stdio: "inherit",
  shell: process.platform === "win32",
});
if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

function findBundleRoot() {
  const candidates = [];
  if (process.env.CARGO_TARGET_DIR) {
    candidates.push(join(process.env.CARGO_TARGET_DIR, "release", "bundle"));
  }
  candidates.push(
    join(repoRoot, "src-tauri", "target", "release", "bundle"),
    join(repoRoot, "target", "release", "bundle"),
  );
  for (const c of candidates) {
    if (c && existsSync(c)) return c;
  }
  return null;
}

function walkFiles(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) walkFiles(full, out);
    else out.push(full);
  }
  return out;
}

function sha256File(path) {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", reject);
    stream.on("end", () => resolve(hash.digest("hex")));
  });
}

const bundleRoot = findBundleRoot();
if (!bundleRoot) {
  console.error("bundle output missing after tauri build");
  process.exit(1);
}
console.log(`Using bundle root: ${bundleRoot}`);

const packageJson = JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf8"));
const interesting = (name) =>
  /\.(msi|exe|AppImage|deb|rpm|dmg)$/i.test(name) ||
  name.endsWith(".app.tar.gz") ||
  name.includes("setup");

const staged = [];
const sumsPath = join(outDir, "SHA256SUMS.txt");
const sumLines = [];

for (const file of walkFiles(bundleRoot)) {
  const name = file.split(/[/\\]/).pop();
  if (!interesting(name)) continue;
  const dest = join(outDir, name);
  copyFileSync(file, dest);
  const hash = await sha256File(dest);
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".")) : "";
  let kind = "bundle";
  if (ext.toLowerCase() === ".msi") kind = "msi";
  else if (/\.exe$/i.test(name) && /setup/i.test(name)) kind = "nsis";
  else if (/\.AppImage$/i.test(name)) kind = "appimage";
  else if (/\.deb$/i.test(name)) kind = "deb";
  else if (/\.rpm$/i.test(name)) kind = "rpm";
  staged.push({
    path: dest,
    name,
    kind,
    sha256: hash,
    byteSize: statSync(dest).size,
    relativeFromBundle: relative(bundleRoot, file),
  });
  sumLines.push(`${hash}  ${name}`);
}

if (staged.length === 0) {
  console.error(`No package artifacts found under ${bundleRoot}`);
  process.exit(1);
}

writeFileSync(sumsPath, `${sumLines.join("\n")}\n`, "utf8");
const manifest = {
  version: packageJson.version,
  productName: "Tiamat",
  createdAtUtc: new Date().toISOString(),
  signing: "unsigned-dev",
  bundleRoot,
  platform: process.platform,
  artifacts: staged,
};
writeFileSync(
  join(outDir, "package-manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
  "utf8",
);
console.log(`Staged ${staged.length} package(s) to ${outDir}`);
console.log(JSON.stringify(manifest, null, 2));
