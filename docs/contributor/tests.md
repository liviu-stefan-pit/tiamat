# Tests

## Taxonomy

| Layer | How to run | Notes |
|---|---|---|
| Rust unit + integration | `npm run test:rust` / `cargo test --workspace` | Includes Job Object, recovery, packaging, workspace isolation |
| Frontend unit | `npm run test:frontend` | Vitest + Testing Library |
| Contract fixtures | `npm run test:contracts` | Schema round-trip / rejection |
| Docs tooling | `npm run test:docs` | Manifest parsers, link/command checks |
| E2E (dev host) | `npm run test:e2e` | Playwright + Vite; fake-only |
| Packaged clean-profile smoke | `npm run test:packaged` | Isolated APPDATA |
| Cleanup proof | `npm run test:cleanup-proof` | Zero owned processes |
| Deterministic demo | `npm run demo` | Full fake story |
| Real Cursor canary | `npm run test:canary` | Spending-consented, version-gated; **not CI** |

Combined:

```powershell
npm test
npm run test:all
npm run ci
```

## Policy

- Deterministic suites and CI use the fake Cursor CLI only. No paid calls.
- Live canary requires explicit spending consent env vars and is a release gate, not CI.
- E2E uses isolated temp dirs / deterministic fakes.
- Fixture secrets must never appear in DB, artifacts, exports, or UI.

## TestBench

See `fixtures/testbench/README.md` and `npm run testbench:materialize`.

## Docs / new-user E2E

`e2e/p12-new-user.spec.ts` follows the documented TestBench journey from [../user/first-run.md](../user/first-run.md).
