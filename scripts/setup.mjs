#!/usr/bin/env node
/**
 * Cross-platform contributor bootstrap: verify toolchain, install deps, run CI suite.
 */
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const shell = process.platform === "win32";

function run(cmd, args, opts = {}) {
  console.log(`> ${cmd} ${args.join(" ")}`);
  const result = spawnSync(cmd, args, {
    cwd: repoRoot,
    stdio: "inherit",
    shell,
    env: process.env,
    ...opts,
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function requireCommand(name) {
  const probe = spawnSync(name, ["--version"], { shell, encoding: "utf8" });
  if (probe.error || probe.status !== 0) {
    console.error(`Required command '${name}' is not available on PATH.`);
    process.exit(1);
  }
}

console.log("Tiamat setup — verifying toolchain and installing dependencies");

requireCommand("node");
requireCommand("npm");
requireCommand("cargo");

run("rustup", ["component", "add", "rustfmt", "clippy"]);

const nodeVersion = spawnSync("node", ["-p", "process.versions.node"], {
  encoding: "utf8",
  shell,
}).stdout.trim();
console.log(`Node.js ${nodeVersion}`);
run("rustc", ["--version"]);
run("cargo", ["--version"]);

run("npm", ["ci"]);
run("npx", ["playwright", "install", "chromium"]);

console.log("Running workspace verification");
run("npm", ["run", "ci"]);

console.log("Setup complete.");
