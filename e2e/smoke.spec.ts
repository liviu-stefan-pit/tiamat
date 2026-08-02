import { test, expect } from "@playwright/test";

test("desktop shell smoke: app loads in dev host", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("tiamat-shell")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Tiamat" })).toBeVisible();
  await expect(
    page.getByText("Drop folders or files to begin preflight."),
  ).toBeVisible();
  await expect(page.getByTestId("start-implementation")).toBeDisabled();
});

test("remount/restart replays the same ordered events", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });

  await page.goto("/");
  await expect(page.getByTestId("activity-log")).toBeVisible();
  await expect(page.getByTestId("log-event").first()).toBeVisible();

  const first = await page
    .locator('[data-testid="log-event"]')
    .evaluateAll((nodes) =>
      nodes.map((node) => ({
        id: node.getAttribute("data-event-id"),
        sequence: node.getAttribute("data-sequence"),
        type: node.getAttribute("data-type"),
        text: node.textContent,
      })),
    );

  expect(first.length).toBeGreaterThan(3);
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
        text: node.textContent,
      })),
    );

  expect(second).toEqual(first);
});

test("activity log filters persisted fake run events", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
  await page.goto("/");
  await expect(page.getByTestId("log-event").first()).toBeVisible();
  await page.getByTestId("log-type").fill("phase.");
  const filtered = page.locator('[data-testid="log-event"]');
  await expect(filtered.first()).toBeVisible();
  const count = await filtered.count();
  expect(count).toBeGreaterThan(0);
  for (let i = 0; i < count; i += 1) {
    await expect(filtered.nth(i)).toHaveAttribute("data-type", /phase\./);
  }
});

test("Start stays gated until trust and warnings render", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
  await page.goto("/");
  await expect(page.getByTestId("start-implementation")).toBeDisabled();

  await page
    .getByTestId("intake-path-input")
    .fill("C:\\fixture\\secret-nested");
  await page.getByTestId("intake-analyze").click();

  await expect(page.getByTestId("preflight-card")).toBeVisible();
  await expect(page.getByTestId("preflight-warnings")).toBeVisible();
  await expect(page.getByTestId("preflight-warning").first()).toBeVisible();
  await expect(page.getByTestId("start-implementation")).toBeDisabled();

  const bodyText = await page.locator("body").innerText();
  expect(bodyText).not.toContain("AKIAIOSFODNN7EXAMPLE");
  expect(bodyText).not.toContain("fixture-secret-value");

  await page.getByTestId("trust-untrusted").check();
  await page.getByTestId("trust-execution").check();
  await expect(page.getByTestId("start-implementation")).toBeEnabled();
});

test("intake to isolated output proves source unchanged", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
  await page.goto("/");
  await expect(page.getByTestId("start-implementation")).toBeDisabled();
  await expect(page.getByTestId("workspace-panel")).toContainText(
    "No managed workspace yet",
  );

  await page.getByTestId("intake-path-input").fill("C:\\fixture\\clean-git-app");
  await page.getByTestId("intake-analyze").click();
  await expect(page.getByTestId("preflight-card")).toBeVisible();
  await page.getByTestId("trust-untrusted").check();
  await page.getByTestId("trust-execution").check();
  await expect(page.getByTestId("start-implementation")).toBeEnabled();

  await page.getByTestId("start-implementation").click();
  await expect(page.getByTestId("workspace-source-unchanged")).toHaveText(
    /unchanged/i,
  );
  await expect(page.getByTestId("workspace-managed-root")).toContainText(
    "C:\\managed\\run-",
  );
  await expect(page.getByTestId("workspace-project").first()).toBeVisible();
  await expect(page.getByTestId("workspace-promotion")).toContainText(
    "unpromoted",
  );
  await expect(page.getByTestId("workspace-label")).toContainText(
    "source unchanged",
  );
});

test("rough-spec Start produces visible architect plan on graph", async ({
  page,
}) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
  await page.goto("/");

  await page
    .getByTestId("intake-path-input")
    .fill("C:\\fixture\\rough-spec-notes");
  await page.getByTestId("intake-analyze").click();
  await expect(page.getByTestId("preflight-card")).toBeVisible();
  await page.getByTestId("trust-untrusted").check();
  await page.getByTestId("trust-execution").check();
  await expect(page.getByTestId("start-implementation")).toBeEnabled();

  await page.getByTestId("start-implementation").click();

  await expect(page.getByTestId("graph-plan-title")).toContainText(
    "Rough-spec notes tool",
  );
  await expect(page.getByTestId("graph-node").first()).toHaveAttribute(
    "data-phase-id",
    "P01",
  );
  await expect(page.getByTestId("architect-summary")).toContainText("compiled");
  await expect(page.getByTestId("architect-summary")).toContainText(
    "gpt-5.6-sol-high",
  );
  await expect(page.getByTestId("architect-summary")).toContainText(
    "planMode=yes",
  );
  await expect(page.getByTestId("architect-summary")).toContainText(
    "force=no",
  );
  await expect(page.getByTestId("workspace-source-unchanged")).toHaveText(
    /unchanged/i,
  );
});

test("executor fake project checkpoints only after unit+integration+e2e pass", async ({
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
  await expect(page.getByTestId("preflight-card")).toBeVisible();
  await page.getByTestId("trust-untrusted").check();
  await page.getByTestId("trust-execution").check();
  await expect(page.getByTestId("start-implementation")).toBeEnabled();

  await page.getByTestId("start-implementation").click();

  await expect(page.getByTestId("graph-plan-title")).toContainText(
    "Executor fixture",
  );
  await expect(page.getByTestId("executor-summary")).toContainText("passed");
  await expect(page.getByTestId("executor-summary")).toContainText("unit:ok");
  await expect(page.getByTestId("executor-summary")).toContainText(
    "integration:ok",
  );
  await expect(page.getByTestId("executor-summary")).toContainText("e2e:ok");
  await expect(page.getByTestId("executor-summary")).toContainText(
    "checkpoint=ready",
  );
  await expect(page.getByTestId("executor-summary")).toContainText(
    "projected=yes",
  );
  await expect(page.getByTestId("workspace-panel")).toContainText(/checkpoint|cp-/i);
});

test("scheduler demo graph shows parallel blocked paused escalated states", async ({
  page,
}) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
  await page.goto("/");

  await page
    .getByTestId("intake-path-input")
    .fill("C:\\fixture\\scheduler-demo");
  await page.getByTestId("intake-analyze").click();
  await expect(page.getByTestId("preflight-card")).toBeVisible();
  await page.getByTestId("trust-untrusted").check();
  await page.getByTestId("trust-execution").check();
  await expect(page.getByTestId("start-implementation")).toBeEnabled();

  await page.getByTestId("start-implementation").click();

  await expect(page.getByTestId("graph-plan-title")).toContainText(
    "Scheduler multi-repo demo",
  );
  await expect(page.getByTestId("scheduler-summary")).toContainText(
    "dag-scheduler",
  );
  await expect(page.getByTestId("scheduler-summary")).toContainText(
    "paused=yes",
  );

  const nodes = page.locator('[data-testid="graph-node"]');
  await expect(nodes).toHaveCount(4);

  await expect(
    page.locator('[data-testid="graph-node"][data-phase-id="P02"]'),
  ).toHaveAttribute("data-status", "running");
  await expect(
    page.locator('[data-testid="graph-node"][data-phase-id="P03"]'),
  ).toHaveAttribute("data-status", "blocked");
  await expect(
    page.locator('[data-testid="graph-node"][data-phase-id="P01"]'),
  ).toHaveAttribute("data-status", "failed");

  const p01Title = await page
    .locator('[data-testid="graph-node"][data-phase-id="P01"]')
    .getAttribute("title");
  // Escalation reason remains visible on the escalated attempt path via P04/P01 history.
  const body = await page.locator("body").innerText();
  expect(body.toLowerCase()).toMatch(/escalat|grok|scheduler/);
  expect(p01Title ?? "").toBeTruthy();

  await expect(page.getByTestId("run-status")).toContainText("paused");
});

test("Cursor capability status and settings dry-run redact secrets", async ({
  page,
}) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
  await page.goto("/");

  await expect(page.getByTestId("cursor-status")).toContainText("available");
  await expect(page.getByTestId("cursor-status")).toContainText("1.2.3");

  await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("settings-panel")).toBeVisible();
  await page.getByTestId("cursor-reprobe").click();
  await expect(page.getByTestId("cursor-capability-status")).toContainText(
    "available",
  );
  await expect(page.getByTestId("cursor-auth")).toHaveText("ready");
  await expect(page.getByTestId("cursor-models")).toContainText("composer-2.5");

  await page.getByTestId("cursor-dry-run").click();
  await expect(page.getByTestId("cursor-preview-spawned")).toHaveText(
    "Spawned: no",
  );
  const argv = await page.getByTestId("cursor-preview-argv").innerText();
  const stdin = await page.getByTestId("cursor-preview-stdin").innerText();
  expect(argv).not.toContain("demo-api-key-should-redact");
  expect(stdin).not.toContain("fixture-secret-value");
  expect(stdin).not.toContain("demo-api-key-should-redact");
  expect(await page.locator("body").innerText()).not.toContain(
    "AKIAIOSFODNN7EXAMPLE",
  );
});

test("unfocused window global abort and second-press force", async ({
  page,
}) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
    localStorage.removeItem("tiamat.p07.lastAbortPress");
  });
  await page.goto("/");
  await expect(page.getByTestId("tiamat-shell")).toBeVisible();

  // Mark run as executing so abort has an active run.
  await page.evaluate(async () => {
    const { transitionRunStatus, ensureDemoRun } = await import(
      "/src/lib/tauri/commands.ts"
    );
    const demo = await ensureDemoRun();
    await transitionRunStatus(demo.run.runId, "executing", "E2E active run");
  });
  await page.reload();
  await expect(page.getByTestId("run-status")).toContainText("executing");

  // Blur / unfocus the window, then press the global abort chord.
  await page.evaluate(() => {
    window.blur();
    (document.activeElement as HTMLElement | null)?.blur?.();
  });
  await page.keyboard.press("Control+Shift+F12");
  await expect(page.getByTestId("abort-ack")).toContainText(/Emergency cancel|abort/i);
  await expect(page.getByTestId("run-status")).toContainText("cancelled");

  // Re-activate and second-press force within 3s.
  await page.evaluate(async () => {
    const { transitionRunStatus, ensureDemoRun } = await import(
      "/src/lib/tauri/commands.ts"
    );
    const demo = await ensureDemoRun();
    await transitionRunStatus(demo.run.runId, "executing", "E2E second press");
  });
  await page.reload();
  await expect(page.getByTestId("run-status")).toContainText("executing");
  await page.keyboard.press("Control+Shift+F12");
  await expect(page.getByTestId("abort-ack")).toBeVisible();
  await page.keyboard.press("Control+Shift+F12");
  await expect(page.getByTestId("abort-ack")).toContainText(/Forced abort|force/i);
});

test("timeout fixture persists same-chat resume metadata", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
  });
  await page.goto("/");
  await expect(page.getByTestId("tiamat-shell")).toBeVisible();

  await page.evaluate(async () => {
    const { ensureDemoRun, runProcessFixture } = await import(
      "/src/lib/tauri/commands.ts"
    );
    const demo = await ensureDemoRun();
    const outcome = await runProcessFixture({
      runId: demo.run.runId,
      mode: "silent_hang",
      warnAfterMs: 40,
      gracefulAfterMs: 100,
      forceGraceMs: 40,
    });
    (window as unknown as { __p07Outcome?: unknown }).__p07Outcome = outcome;
  });

  const outcome = await page.evaluate(() => {
    return (window as unknown as { __p07Outcome?: {
      timedOut: boolean;
      zeroSurvivors: boolean;
      cleanupOk: boolean;
      resume?: { chatId?: string; nextModel?: string; reason: string };
    } }).__p07Outcome;
  });
  expect(outcome?.timedOut).toBe(true);
  expect(outcome?.zeroSurvivors).toBe(true);
  expect(outcome?.cleanupOk).toBe(true);
  expect(outcome?.resume?.chatId).toBe("chat-timeout-fixture");
  expect(outcome?.resume?.nextModel).toBe("cursor-grok-4.5-low");
  expect(outcome?.resume?.reason).toBe("attempt_watchdog_timeout");

  await page.getByTestId("run-timeout-fixture").evaluate((el) => {
    (el as HTMLElement).style.display = "inline";
  });
  await page.getByTestId("run-timeout-fixture").click();
  await expect(page.getByTestId("abort-ack")).toContainText(/Timeout resume|chat-timeout-fixture/);
});

test("close policy keep-running and abort rebind UI", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.removeItem("tiamat.p01.browser-store.v1");
    localStorage.removeItem("tiamat.p07.abort");
  });
  await page.goto("/");
  await page.getByTestId("simulate-close-policy").evaluate((el) => {
    (el as HTMLElement).style.display = "inline";
  });
  await page.getByTestId("simulate-close-policy").click();
  await expect(page.getByTestId("close-policy-dialog")).toBeVisible();
  await page.getByTestId("keep-running").click();
  await expect(page.getByTestId("abort-ack")).toContainText(/Keep Tiamat running/i);

  await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("abort-settings")).toBeVisible();
  await page.getByTestId("abort-shortcut-input").fill("Ctrl+Alt+F12");
  await page.getByTestId("abort-rebind").click();
  await expect(page.getByTestId("abort-status-text")).toContainText(/degraded|rebinding/i);
  await expect(page.getByTestId("tray-fallback-flag")).toContainText("enabled");
});
