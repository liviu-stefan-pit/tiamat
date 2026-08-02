import { test, expect } from "@playwright/test";

/**
 * Performance reference (documented CI target):
 * Windows 11 VM, 4 vCPU, 8 GB RAM, fixed WebView2/Tauri versions.
 * Protocol: 3 warm-up runs + 10 measured runs; monotonic in-app timestamps.
 * See fixtures/perf/README.md.
 */

test.describe("P09 logger performance fixtures", () => {
  test("100k persisted events keep DOM bounded and reconstruct from store", async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await page.addInitScript(() => {
      localStorage.removeItem("tiamat.p01.browser-store.v1");
    });
    await page.goto("/");
    await expect(page.getByTestId("activity-log")).toBeVisible();

    const seed = await page.evaluate(async () => {
      const { seedPerfEvents, ensureDemoRun, replayEvents } = await import(
        "/src/lib/tauri/commands.ts"
      );
      const demo = await ensureDemoRun();
      const result = await seedPerfEvents(demo.run.runId, 100_000);
      const replayed = await replayEvents(demo.run.runId, 0);
      await (
        window as unknown as { __tiamatRefreshEvents?: () => Promise<void> }
      ).__tiamatRefreshEvents?.();
      return {
        seeded: result.seeded,
        totalEvents: result.totalEvents,
        replayed: replayed.length,
      };
    });

    expect(seed.seeded).toBe(100_000);
    expect(seed.totalEvents).toBeGreaterThanOrEqual(100_000);
    expect(seed.replayed).toBe(seed.totalEvents);

    await expect(page.getByTestId("log-count")).toContainText(
      String(seed.totalEvents),
      { timeout: 60_000 },
    );
    const rendered = await page.locator('[data-testid="log-event"]').count();
    expect(rendered).toBeLessThan(200);
    await expect(page.getByTestId("activity-log")).toHaveAttribute(
      "data-virtualized",
      "true",
    );

    // Reconstruct: replay again and confirm same ordered sequences.
    const reconstructed = await page.evaluate(async () => {
      const { ensureDemoRun, replayEvents } = await import(
        "/src/lib/tauri/commands.ts"
      );
      const demo = await ensureDemoRun();
      const events = await replayEvents(demo.run.runId, 0);
      const sequences = events.map((event) => event.sequence);
      const sorted = [...sequences].sort((a, b) => a - b);
      return {
        count: events.length,
        ordered: sequences.every((value, index) => value === sorted[index]),
      };
    });
    expect(reconstructed.count).toBe(seed.totalEvents);
    expect(reconstructed.ordered).toBe(true);
  });

  test("1000 events/sec burst meets p95 visibility and input latency budgets", async ({
    page,
  }) => {
    test.setTimeout(120_000);
    await page.addInitScript(() => {
      localStorage.removeItem("tiamat.p01.browser-store.v1");
    });
    await page.goto("/");
    await expect(page.getByTestId("log-search")).toBeVisible();

    const metrics = await page.evaluate(async () => {
      const { ensureDemoRun, emitEventBurst } = await import(
        "/src/lib/tauri/commands.ts"
      );
      const demo = await ensureDemoRun();

      const warmups = 3;
      const measured = 10;
      const visibilitySamples: number[] = [];
      const inputSamples: number[] = [];

      const frame = () =>
        new Promise<void>((resolve) => {
          requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
        });

      async function oneRun(collect: boolean) {
        const t0 = performance.now();
        await emitEventBurst(demo.run.runId, 1000);
        await frame();
        const visibleAt = performance.now();
        if (collect) visibilitySamples.push(visibleAt - t0);

        const input = document.querySelector(
          '[data-testid="log-search"]',
        ) as HTMLInputElement | null;
        const i0 = performance.now();
        if (input) {
          input.focus();
          input.value = `burst-${collect ? "m" : "w"}`;
          input.dispatchEvent(new Event("input", { bubbles: true }));
        }
        await frame();
        const i1 = performance.now();
        if (collect) inputSamples.push(i1 - i0);
      }

      for (let i = 0; i < warmups; i += 1) {
        await oneRun(false);
      }
      visibilitySamples.length = 0;
      inputSamples.length = 0;
      for (let i = 0; i < measured; i += 1) {
        await oneRun(true);
      }

      const percentile = (values: number[], p: number) => {
        const sorted = [...values].sort((a, b) => a - b);
        if (sorted.length === 0) return Number.POSITIVE_INFINITY;
        const idx = Math.min(
          sorted.length - 1,
          Math.ceil((p / 100) * sorted.length) - 1,
        );
        return sorted[idx]!;
      };

      return {
        p95VisibilityMs: percentile(visibilitySamples, 95),
        p95InputMs: percentile(inputSamples, 95),
        visibilitySamples,
        inputSamples,
        rendered: document.querySelectorAll('[data-testid="log-event"]').length,
      };
    });

    expect(metrics.rendered).toBeLessThan(200);
    expect(metrics.p95VisibilityMs).toBeLessThan(250);
    expect(metrics.p95InputMs).toBeLessThan(100);
  });
});
