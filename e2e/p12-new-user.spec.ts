import { test, expect } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Packaged new-user TestBench E2E (P12): follows docs/user/first-run.md
 * using the deterministic fake Cursor CLI — no paid models.
 */
const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

test.describe("P12 new-user TestBench journey (docs-guided, fake-only)", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.removeItem("tiamat.p01.browser-store.v1");
      localStorage.removeItem("tiamat.p11.app-settings");
      localStorage.removeItem("tiamat.p07.abort");
    });
  });

  test("documentation index and first-run guide exist for the journey", () => {
    const index = path.join(repoRoot, "docs/README.md");
    const firstRun = path.join(repoRoot, "docs/user/first-run.md");
    const manifest = path.join(repoRoot, "docs/config/docs-manifest.json");
    expect(fs.existsSync(index)).toBe(true);
    expect(fs.existsSync(firstRun)).toBe(true);
    expect(fs.existsSync(manifest)).toBe(true);
    const body = fs.readFileSync(firstRun, "utf8");
    expect(body).toContain("npm run testbench:materialize");
    expect(body).toContain("executor-app");
    expect(body).toContain("Ctrl+Shift+F12");
    expect(body).not.toContain("fixture-secret-value");
  });

  test("new user: Settings fake CLI → intake executor-app → trust → Start", async ({
    page,
  }) => {
    await page.goto("/");

    // Step 1 from first-run.md — configure Cursor CLI (fake)
    await page.getByTestId("open-settings").click();
    await expect(page.getByTestId("settings-panel")).toBeVisible();
    await page
      .getByTestId("cursor-cli-path-input")
      .fill("fixtures/cursor-cli/fake-agent.mjs");
    await page.getByTestId("cursor-cli-path-save").click();
    await expect(page.getByTestId("cursor-capability-status")).toContainText(
      /available/i,
    );
    await expect(page.getByTestId("abort-shortcut-input")).toHaveValue(
      /Ctrl\+Shift\+F12/i,
    );

    // Close settings by continuing to intake (shell keeps panels accessible)
    await page
      .getByTestId("intake-path-input")
      .fill("C:\\fixture\\testbench\\executor-app");
    await page.getByTestId("intake-analyze").click();
    await expect(page.getByTestId("preflight-card")).toBeVisible();

    // Trust acknowledgments from intake-and-trust.md
    await page.getByTestId("trust-untrusted").check();
    await page.getByTestId("trust-execution").check();
    await expect(page.getByTestId("start-implementation")).toBeEnabled();

    // Start implementation
    await page.getByTestId("start-implementation").click();
    await expect(page.getByTestId("graph-canvas")).toBeVisible();
    await expect(page.getByTestId("activity-log")).toBeVisible();
    await expect(page.getByTestId("workspace-panel")).toBeVisible();
    await expect(page.getByTestId("run-controls")).toBeVisible();
    await expect(page.getByTestId("emergency-stop-hint")).toContainText(
      /Ctrl\+Shift\+F12|Emergency/i,
    );

    // Isolation / promotion surfaces from docs
    await expect(page.getByTestId("workspace-promotion")).toBeVisible();
    await expect(page.getByTestId("workspace-export")).toBeVisible();
    await expect(page.getByTestId("workspace-promote")).toBeVisible();
    await page.getByTestId("workspace-export").click();
    await expect(page.getByTestId("workspace-promotion")).toContainText(/exported/i);
    await page.getByTestId("workspace-promote").click();
    await expect(page.getByTestId("workspace-promotion")).toContainText(/promoted/i);

    // Secrets from docs forbidden list never appear
    const body = await page.locator("body").innerText();
    expect(body).not.toContain("fixture-secret-value");
    expect(body).not.toContain("AKIAIOSFODNN7EXAMPLE");
  });

  test("new user can pause / cancel / emergency controls as documented", async ({
    page,
  }) => {
    await page.goto("/");
    await page
      .getByTestId("intake-path-input")
      .fill("C:\\fixture\\testbench\\executor-app");
    await page.getByTestId("intake-analyze").click();
    await page.getByTestId("trust-untrusted").check();
    await page.getByTestId("trust-execution").check();
    await page.getByTestId("start-implementation").click();

    await expect(page.getByTestId("cancel-run")).toBeVisible();
    await expect(page.getByTestId("emergency-abort")).toBeVisible();
    await page.getByTestId("emergency-abort").click();
    await expect(page.getByTestId("abort-ack")).toBeVisible();
  });
});
