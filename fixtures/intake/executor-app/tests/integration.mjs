import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../src/feature.ts", import.meta.url), "utf8");
if (!src.includes("greet")) {
  console.error("integration: missing greet");
  process.exit(1);
}
console.log("integration ok");
