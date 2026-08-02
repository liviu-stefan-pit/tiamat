# P09 performance fixtures

## Reference environment

Documented CI / acceptance reference for Tiamat P09 logger/graph performance:

| Item | Value |
|------|-------|
| OS | Windows 11 |
| CPU | 4 vCPU |
| RAM | 8 GB |
| Host | Fixed WebView2 + Tauri 2.x versions used by this repo |
| Timestamps | Monotonic in-app (`performance.now()` / UTC event stamps) |
| Protocol | 3 warm-up runs, then 10 measured runs |

## Fixtures

| Fixture | Command / API | Purpose |
|---------|---------------|---------|
| 100,000 persisted events | `seed_perf_events(runId, 100000)` | Reconstruct/replay + virtualization bound |
| 1,000 events/second burst | `emit_event_burst(runId, 1000)` | Event-to-visible and input latency |

Browser host (Vite/Playwright) uses the in-memory `browser-store` persistence layer for the same command names. Native Tauri uses SQLite WAL bulk insert.

## Gates

- p95 event-to-visible latency &lt; 250 ms
- p95 input latency &lt; 100 ms
- Rendered log DOM remains bounded by virtualization (`data-virtualized="true"`, child count ≪ event count)

## Running

```bash
npm run test:e2e -- e2e/p09-perf.spec.ts
cargo test --test event_volume
```
