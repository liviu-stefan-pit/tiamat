import { test, expect } from "@playwright/test";

test("desktop shell smoke: app loads in dev host", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("tiamat-shell")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Tiamat" })).toBeVisible();
  await expect(page.getByText("Drop folders or files to begin preflight.")).toBeVisible();
});
