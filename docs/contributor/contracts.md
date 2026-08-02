# Contracts

Versioned JSON Schema + mirrored Rust/TypeScript types. Current `schemaVersion`: **1**.

## Schemas

| Schema | Path |
|---|---|
| Intake manifest | `schemas/intake-manifest.schema.json` |
| Project plan | `schemas/project-plan.schema.json` |
| Event envelope | `schemas/event-envelope.schema.json` |
| Phase result | `schemas/phase-result.schema.json` |

Valid/invalid fixtures: `fixtures/contracts/v1/`.

## Rust crate

`crates/tiamat-contracts` exports domain structs and `validate_json` helpers. Integration tests: `npm run test:contracts`.

## TypeScript

`src/domain/contracts.ts` mirrors the same shapes for the UI bridge.

## Compatibility rules

- Migrations for SQLite are append-only.
- Never silently reinterpret old records; bump schema version and migrate.
- Phase-result payloads are immutable once accepted by the orchestrator.

## Validate locally

```powershell
npm run test:contracts
cargo test -p tiamat-contracts
```
