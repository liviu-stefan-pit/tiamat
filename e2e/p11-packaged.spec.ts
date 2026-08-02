import { test, expect } from "@playwright/test";

test.describe("P11 TestBench / packaging acceptance (fake-only)", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.removeItem("tiamat.p01.browser-store.v1");
      localStorage.removeItem("tiamat.p11.app-settings");
    });
  });

  test("configured CLI path saves and probes without paid calls", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByTestId("open-settings").click();
    await expect(page.getByTestId("settings-panel")).toBeVisible();
    await page
      .getByTestId("cursor-cli-path-input")
      .fill("fixtures/cursor-cli/fake-agent.mjs");
    await page.getByTestId("cursor-cli-path-save").click();
    await expect(page.getByTestId("cursor-cli-path-saved")).toContainText(
      "fake-agent",
    );
    await expect(page.getByTestId("cursor-capability-status")).toContainText(
      "available",
    );
  });

  test("TestBench journey: intake → trust → start (fake)", async ({ page }) => {
    await page.goto("/");
    await page
      .getByTestId("intake-path-input")
      .fill("C:\\fixture\\executor-app");
    await page.getByTestId("intake-analyze").click();
    await expect(page.getByTestId("preflight-card")).toBeVisible();
    await page.getByTestId("trust-untrusted").check();
    await page.getByTestId("trust-execution").check();
    await expect(page.getByTestId("start-implementation")).toBeEnabled();
    await page.getByTestId("start-implementation").click();
    await expect(page.getByTestId("graph-canvas")).toBeVisible();
    await expect(page.getByTestId("activity-log")).toBeVisible();
  });

  test("unicode fixture path is accepted and secrets stay redacted", async ({
    page,
  }) => {
    await page.goto("/");
    await page
      .getByTestId("intake-path-input")
      .fill("C:\\fixture\\testbench\\unicode-项目");
    await page.getByTestId("intake-analyze").click();
    await expect(page.getByTestId("preflight-card")).toBeVisible();
    const body = await page.locator("body").innerText();
    expect(body).not.toContain("fixture-secret-value");
    expect(body).not.toContain("AKIAIOSFODNN7EXAMPLE");
  });

  test("global abort shortcut control remains available", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("open-settings").click();
    await expect(page.getByTestId("abort-settings")).toBeVisible();
    await expect(page.getByTestId("abort-shortcut-input")).toHaveValue(
      /Ctrl\+Shift\+F12/i,
    );
  });

  test("packaging bridge exposes cleanup proof and testbench materialize", async ({
    page,
  }) => {
    await page.goto("/");
    const result = await page.evaluate(async () => {
      const mod = await import("/src/lib/tauri/commands.ts");
      const bench = await mod.materializeTestbench("C:\\fixture\\testbench-out");
      const cleanup = await mod.provePackagedCleanup(
        "11111111-1111-4111-8111-111111111111",
        "C:\\artifacts\\cleanup",
      );
      const uninstall = await mod.planUninstallRetention();
      return {
        cases: bench.cases,
        zero: cleanup.zeroOwnedProcesses,
        retainFlag: uninstall.retainUnpromotedWorkspaces !== undefined,
      };
    });
    expect(result.cases).toContain("executor-app");
    expect(result.zero).toBe(true);
    expect(result.retainFlag).toBe(true);
  });
});
