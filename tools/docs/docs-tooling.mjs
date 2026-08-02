/**
 * Documentation config parsers and validators for Tiamat P12.
 */

const SHA256_RE = /^[a-f0-9]{64}$/;

/**
 * @param {unknown} raw
 * @returns {{ ok: boolean, value?: object, errors: string[] }}
 */
export function parseDocsManifest(raw) {
  const errors = [];
  if (!raw || typeof raw !== "object") {
    return { ok: false, errors: ["manifest must be an object"] };
  }
  const m = /** @type {Record<string, unknown>} */ (raw);
  if (m.schemaVersion !== 1) errors.push("schemaVersion must be 1");
  if (typeof m.product !== "string" || !m.product)
    errors.push("product required");
  if (typeof m.version !== "string" || !m.version)
    errors.push("version required");
  if (!Array.isArray(m.guides) || m.guides.length === 0) {
    errors.push("guides must be a nonempty array");
  }
  if (
    !Array.isArray(m.documentedCommands) ||
    m.documentedCommands.length === 0
  ) {
    errors.push("documentedCommands must be a nonempty array");
  }
  if (!m.packageHashes || typeof m.packageHashes !== "object") {
    errors.push("packageHashes required");
  } else {
    for (const [name, hash] of Object.entries(
      /** @type {Record<string, unknown>} */ (m.packageHashes),
    )) {
      if (typeof hash !== "string" || !SHA256_RE.test(hash)) {
        errors.push(`invalid sha256 for ${name}`);
      }
    }
  }
  if (typeof m.signing !== "string" || !m.signing)
    errors.push("signing required");
  if (typeof m.emergencyStopShortcut !== "string") {
    errors.push("emergencyStopShortcut required");
  }
  if (errors.length) return { ok: false, errors };
  return { ok: true, value: m, errors: [] };
}

/** @param {string} markdown */
export function extractMarkdownLinks(markdown) {
  const links = [];
  const re = /\[([^\]]*)\]\(([^)]+)\)/g;
  let match;
  while ((match = re.exec(markdown)) !== null) {
    links.push(match[2].trim());
  }
  return links;
}

/** @param {string} markdown */
export function extractFencedCommands(markdown) {
  const commands = [];
  const fence = /```(?:powershell|bash|shell|text)?\n([\s\S]*?)```/gi;
  let match;
  while ((match = fence.exec(markdown)) !== null) {
    const body = match[1] ?? "";
    for (const line of body.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#") || trimmed.startsWith("//"))
        continue;
      if (
        trimmed.startsWith("npm ") ||
        trimmed.startsWith("cargo ") ||
        trimmed.startsWith("git ") ||
        trimmed.startsWith("powershell ")
      ) {
        commands.push(trimmed.replace(/`$/g, ""));
      }
    }
  }
  return commands;
}

/**
 * @param {{ version?: string, documentedCommands: Array<{ id: string, kind: string, script?: string }> }} manifest
 * @param {{ version?: string, scripts?: Record<string, string> }} packageJson
 */
export function validateDocumentedNpmScripts(manifest, packageJson) {
  const errors = [];
  const scripts = packageJson.scripts ?? {};
  if (packageJson.version && packageJson.version !== manifest.version) {
    errors.push(
      `version mismatch: manifest ${manifest.version} vs package.json ${packageJson.version}`,
    );
  }
  for (const cmd of manifest.documentedCommands) {
    if (cmd.kind !== "npm-script") continue;
    const scriptName = cmd.script;
    if (!scriptName) {
      errors.push(`documented command ${cmd.id} missing script field`);
      continue;
    }
    if (!(scriptName in scripts)) {
      errors.push(
        `documented npm script missing in package.json: ${scriptName}`,
      );
    }
  }
  return errors;
}

/**
 * @param {string} fromFile
 * @param {string[]} links
 * @param {(p: string) => boolean} exists
 * @param {(fromFile: string, href: string) => string} resolve
 */
export function validateRelativeLinks(fromFile, links, exists, resolve) {
  const errors = [];
  for (const href of links) {
    if (
      href.startsWith("http://") ||
      href.startsWith("https://") ||
      href.startsWith("mailto:") ||
      href.startsWith("#")
    ) {
      continue;
    }
    const cleaned = href.split("#")[0] ?? href;
    if (!cleaned) continue;
    const target = resolve(fromFile, cleaned);
    if (!exists(target)) {
      errors.push(`broken link in ${fromFile}: ${href} -> ${target}`);
    }
  }
  return errors;
}

/** @param {string} text @param {string[]} forbidden */
export function findForbiddenSecrets(text, forbidden) {
  const hits = [];
  for (const secret of forbidden) {
    if (text.includes(secret)) hits.push(secret);
  }
  return hits;
}

/**
 * @param {Record<string, string>} manifestHashes
 * @param {string} sumsFileContents
 */
export function assertPackageHashesMatch(manifestHashes, sumsFileContents) {
  const errors = [];
  const map = new Map();
  for (const line of sumsFileContents.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const parts = trimmed.split(/\s+/);
    if (parts.length < 2) continue;
    map.set(parts[1], parts[0].toLowerCase());
  }
  for (const [name, hash] of Object.entries(manifestHashes)) {
    const actual = map.get(name);
    if (!actual) {
      errors.push(`SHA256SUMS missing entry for ${name}`);
      continue;
    }
    if (actual !== hash.toLowerCase()) {
      errors.push(`hash mismatch for ${name}`);
    }
  }
  return errors;
}

/** @param {string} command */
export function normalizeNpmRunCommand(command) {
  const m = command.match(/^npm\s+(?:run\s+)?([^\s]+)/);
  if (!m) return null;
  return m[1];
}
