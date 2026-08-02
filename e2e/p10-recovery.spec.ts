import { test, expect } from "@playwright/test";

test("recovery resume/cancel offer gates new work", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
    localStorage.setItem("tiamat.p10.forceRecovery", "1");
  });
  await page.goto("/");
  await expect(page.getByTestId("recovery-offer")).toBeVisible();
  await expect(page.getByTestId("recovery-reason")).toContainText(
    "Resume or Cancel",
  );
  await expect(page.getByTestId("recovery-resume")).toBeEnabled();
  await expect(page.getByTestId("start-implementation")).toBeDisabled();

  await page.getByTestId("recovery-cancel").click();
  await expect(page.getByTestId("recovery-offer")).toHaveCount(0);
  await expect(page.getByTestId("recovery-offer-probe")).toHaveText("cancelled");
});

test("recovery resume clears offer and records recovery.resumed", async ({
  page,
}) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
    localStorage.setItem("tiamat.p10.forceRecovery", "1");
  });
  await page.goto("/");
  await expect(page.getByTestId("recovery-offer")).toBeVisible();
  await page.getByTestId("recovery-resume").click();
  await expect(page.getByTestId("recovery-offer")).toHaveCount(0);
  await expect(page.getByTestId("recovery-offer-probe")).toHaveText("resumed");
  await page.getByTestId("log-type").fill("recovery.");
  const events = page.locator('[data-testid="log-event"]');
  await expect(events.first()).toBeVisible();
  const types = await events.evaluateAll((nodes) =>
    nodes.map((n) => n.getAttribute("data-type")),
  );
  expect(types.some((t) => t?.includes("recovery.resumed"))).toBeTruthy();
});

test("fixture secrets never reach UI or export", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
    localStorage.removeItem("tiamat.p10.forceRecovery");
  });
  await page.goto("/");
  await expect(page.getByTestId("tiamat-shell")).toBeVisible();

  const leaked = await page.evaluate(async () => {
    const { redactText, exportRunReport, ensureDemoRun } = await import(
      "/src/lib/tauri/commands.ts"
    );
    const redacted = await redactText(
      "token=AKIAIOSFODNN7EXAMPLE secret=fixture-secret-value",
    );
    const demo = await ensureDemoRun();
    const exported = await exportRunReport(demo.run.runId);
    const body = JSON.stringify({ redacted, exported, demo });
    return {
      body,
      hasAws: body.includes("AKIAIOSFODNN7EXAMPLE"),
      hasFixture: body.includes("fixture-secret-value"),
      redactedText: redacted.text,
    };
  });

  expect(leaked.hasAws).toBeFalsy();
  expect(leaked.hasFixture).toBeFalsy();
  expect(leaked.redactedText).not.toContain("AKIAIOSFODNN7EXAMPLE");
  expect(leaked.redactedText).not.toContain("fixture-secret-value");

  const ui = await page.locator("body").innerText();
  expect(ui).not.toContain("AKIAIOSFODNN7EXAMPLE");
  expect(ui).not.toContain("fixture-secret-value");
});
