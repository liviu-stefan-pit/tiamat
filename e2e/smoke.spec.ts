import { test, expect } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
});

test("three-pane shell loads with gated Run", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("tiamat-shell")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Tiamat" })).toBeVisible();
  await expect(page.getByTestId("intake-panel")).toBeVisible();
  await expect(page.getByTestId("output-panel")).toBeVisible();
  await expect(page.getByTestId("activity-log")).toBeVisible();
  await expect(page.getByTestId("start-run")).toBeDisabled();
});

test("remount/reload replays ordered log events", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("log-event").first()).toBeVisible();

  const first = await page
    .locator('[data-testid="log-event"]')
    .evaluateAll((nodes) =>
      nodes.map((node) => ({
        id: node.getAttribute("data-event-id"),
        sequence: node.getAttribute("data-sequence"),
        type: node.getAttribute("data-type"),
      })),
    );

  expect(first.length).toBeGreaterThan(0);
  const sequences = first.map((event) => Number(event.sequence));
  expect(sequences).toEqual([...sequences].sort((a, b) => a - b));

  await page.reload();
  await expect(page.getByTestId("log-event").first()).toBeVisible();

  const second = await page
    .locator('[data-testid="log-event"]')
    .evaluateAll((nodes) =>
      nodes.map((node) => ({
        id: node.getAttribute("data-event-id"),
        sequence: node.getAttribute("data-sequence"),
        type: node.getAttribute("data-type"),
      })),
    );

  expect(second).toEqual(first);
});

test("trust + output unlock Run without leaking secrets", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("start-run")).toBeDisabled();

  await page.getByTestId("intake-path-input").fill("C:\\fixture\\secret-nested");
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByTestId("preflight-summary")).toBeVisible();
  await expect(page.getByTestId("start-run")).toBeDisabled();

  const bodyText = await page.locator("body").innerText();
  expect(bodyText).not.toContain("AKIAIOSFODNN7EXAMPLE");
  expect(bodyText).not.toContain("fixture-secret-value");

  await page.getByTestId("trust-ack").locator("input").check();
  await page.getByTestId("output-path-input").fill("C:\\fixture\\output");
  await page.getByRole("button", { name: "Set" }).click();
  await expect(page.getByTestId("start-run")).toBeEnabled();
});
