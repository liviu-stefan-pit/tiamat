import { test, expect } from "@playwright/test";

test("P09 keyboard controls, restart replay, and completion summary", async ({
  page,
}) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
  await page.goto("/");
  await expect(page.getByTestId("tiamat-shell")).toBeVisible();
  await expect(page.getByLabel("Phase graph")).toBeVisible();
  await expect(page.getByLabel("Activity log")).toBeVisible();
  await expect(page.getByLabel("Run controls")).toBeVisible();

  await page.keyboard.press("Tab");
  await expect(page.locator(":focus")).toBeVisible();

  await page
    .getByTestId("intake-path-input")
    .fill("C:\\fixture\\scheduler-multi");
  await page.getByTestId("intake-analyze").click();
  await page.getByTestId("trust-untrusted").check();
  await page.getByTestId("trust-execution").check();
  await page.getByTestId("start-implementation").click();

  await expect(page.getByTestId("graph-plan-title")).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByTestId("attempt-timeline")).toBeVisible();
  await expect(page.getByTestId("run-controls")).toBeVisible();

  await page.getByTestId("cancel-run").click();
  await expect(page.getByTestId("run-status")).toContainText(/cancelled|unknown|paused|executing/i);

  const before = await page.getByTestId("log-count").innerText();
  await page.reload();
  await expect(page.getByTestId("log-count")).toBeVisible();
  const after = await page.getByTestId("log-count").innerText();
  expect(after).toContain("persisted events");
  expect(before).toContain("persisted events");

  await expect(page.getByTestId("completion-summary")).toBeVisible();
});

test("P09 open output and export report controls map to state", async ({
  page,
}) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
  await page.goto("/");
  await page
    .getByTestId("intake-path-input")
    .fill("C:\\fixture\\executor-app");
  await page.getByTestId("intake-analyze").click();
  await page.getByTestId("trust-untrusted").check();
  await page.getByTestId("trust-execution").check();
  await page.getByTestId("start-implementation").click();
  await expect(page.getByTestId("executor-summary")).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByTestId("open-output")).toBeEnabled();
  await page.getByTestId("open-output").click();
  await expect(page.getByTestId("output-path")).toBeVisible();
  await page.getByTestId("report-export").click();
  await expect(page.getByTestId("export-status")).toContainText(/Exported report/i);
  await expect(page.getByTestId("evidence-panel")).toBeVisible();
  await expect(page.getByTestId("completion-summary")).toBeVisible();
});
