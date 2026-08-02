import { describe, expect, it } from "vitest";
import {
  assertPackageHashesMatch,
  extractFencedCommands,
  extractMarkdownLinks,
  findForbiddenSecrets,
  normalizeNpmRunCommand,
  parseDocsManifest,
  validateDocumentedNpmScripts,
  validateRelativeLinks,
} from "./docs-tooling.mjs";

describe("docs-tooling parsers", () => {
  it("parses a valid manifest", () => {
    const result = parseDocsManifest({
      schemaVersion: 1,
      product: "Tiamat",
      version: "0.1.0",
      candidate: "P11-RELEASE-CANDIDATE.md",
      docsRoot: "docs",
      guides: [
        { id: "install", path: "docs/user/install.md", audience: "user" },
      ],
      documentedCommands: [
        {
          id: "demo",
          command: "npm run demo",
          kind: "npm-script",
          script: "demo",
        },
      ],
      uiSelectors: ["start-implementation"],
      packageHashes: {
        "Tiamat_0.1.0_x64-setup.exe":
          "216bb9a8da1ca025e19f8d3ef19060a83e335f0427d404d56059d370c74d0ee7",
      },
      signing: "unsigned-dev",
      emergencyStopShortcut: "Ctrl+Shift+F12",
      forbiddenExamples: ["fixture-secret-value"],
    });
    expect(result.ok).toBe(true);
    expect(result.value?.version).toBe("0.1.0");
  });

  it("rejects invalid hashes and empty guides", () => {
    const result = parseDocsManifest({
      schemaVersion: 1,
      product: "Tiamat",
      version: "0.1.0",
      guides: [],
      documentedCommands: [],
      packageHashes: { bad: "not-a-hash" },
      signing: "unsigned-dev",
      emergencyStopShortcut: "Ctrl+Shift+F12",
    });
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("guides"))).toBe(true);
    expect(result.errors.some((e) => e.includes("sha256"))).toBe(true);
  });

  it("extracts markdown links and fenced commands", () => {
    const md = `
See [install](./install.md) and [hashes](../release/PACKAGE-HASHES.md#top).

\`\`\`powershell
npm run demo
cargo test --workspace
# comment
\`\`\`
`;
    expect(extractMarkdownLinks(md)).toEqual([
      "./install.md",
      "../release/PACKAGE-HASHES.md#top",
    ]);
    expect(extractFencedCommands(md)).toEqual([
      "npm run demo",
      "cargo test --workspace",
    ]);
  });

  it("validates documented npm scripts against package.json", () => {
    const manifest = {
      version: "0.1.0",
      documentedCommands: [
        { id: "demo", kind: "npm-script", script: "demo" },
        { id: "missing", kind: "npm-script", script: "does-not-exist" },
        { id: "shell", kind: "shell" },
      ],
    };
    const errors = validateDocumentedNpmScripts(manifest, {
      version: "0.1.0",
      scripts: { demo: "node demo.js" },
    });
    expect(errors).toEqual([
      "documented npm script missing in package.json: does-not-exist",
    ]);
  });

  it("validates relative links with injected fs helpers", () => {
    const exists = (p: string) =>
      p.replace(/\\/g, "/").endsWith("docs/user/install.md");
    const resolve = (_from: string, href: string) =>
      href.startsWith("./") ? `docs/user/${href.slice(2)}` : href;
    const errors = validateRelativeLinks(
      "docs/user/first-run.md",
      ["./install.md", "./missing.md", "https://example.com"],
      exists,
      resolve,
    );
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain("missing.md");
  });

  it("finds forbidden secrets and matches package hashes", () => {
    expect(
      findForbiddenSecrets("hello fixture-secret-value", [
        "fixture-secret-value",
      ]),
    ).toEqual(["fixture-secret-value"]);
    const sums = `216bb9a8da1ca025e19f8d3ef19060a83e335f0427d404d56059d370c74d0ee7  Tiamat_0.1.0_x64-setup.exe
cdbee67986cf95ed9efe6401408db52cc3986bc34205fcb13132a15f6ed4d7b4  Tiamat_0.1.0_x64_en-US.msi
`;
    expect(
      assertPackageHashesMatch(
        {
          "Tiamat_0.1.0_x64-setup.exe":
            "216bb9a8da1ca025e19f8d3ef19060a83e335f0427d404d56059d370c74d0ee7",
          "Tiamat_0.1.0_x64_en-US.msi":
            "cdbee67986cf95ed9efe6401408db52cc3986bc34205fcb13132a15f6ed4d7b4",
        },
        sums,
      ),
    ).toEqual([]);
  });

  it("normalizes npm run command names", () => {
    expect(normalizeNpmRunCommand("npm run demo")).toBe("demo");
    expect(normalizeNpmRunCommand("npm test")).toBe("test");
    expect(normalizeNpmRunCommand("cargo test")).toBeNull();
  });
});
