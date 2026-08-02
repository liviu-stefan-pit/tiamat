import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../src/feature.ts", import.meta.url), "utf8");
if (!src.includes("export function greet")) {
  console.error("unit: greet not implemented");
  process.exit(1);
}
console.log("unit ok");
