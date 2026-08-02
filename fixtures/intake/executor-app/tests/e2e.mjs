import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
if (!existsSync(join(root, "src", "feature.ts"))) {
  console.error("e2e: missing feature.ts");
  process.exit(1);
}
const src = await import("node:fs").then((fs) =>
  fs.readFileSync(join(root, "src", "feature.ts"), "utf8"),
);
if (!src.includes("export function greet")) {
  console.error("e2e: greet not implemented");
  process.exit(1);
}
console.log("e2e ok");
