# Rough-spec notes tool

## Summary
Turn brainstorm notes into a small testable notes list app.

## Assumptions
- Desktop-first MVP
- No cloud sync in v1

## Risks
- Ambiguous scope in brainstorm notes

## Phase: P01 — Notes list vertical slice

Deep design notes for fixtures and list rendering.

- **phaseId**: P01
- **objective**: Render a notes list from fixture data. Integration tests inapplicable for notes-only MVP shell; e2e tests inapplicable until a UI host exists.
- **dependencies**: none
- **projectIds**: notes-app
- **readRoots**: /managed/notes-app
- **writeRoots**: /managed/notes-app
- **modelTier**: composer
- **estimatedMinutes**: 10
- **rollbackCheckpoint**: intake-baseline
- **rollbackStrategy**: restore
- **expectedArtifacts**: src/notes.ts

### Acceptance criteria
- `AC-P01-01` — Notes list unit test passes against fixture data — evidence: unit

### Unit tests
- `UT-P01-01` — command: `npm` `test` — cwd: `.` — timeout: 120 — covers: AC-P01-01

### Integration tests
- (none) — reason: notes-only MVP

### E2E tests
- (none) — reason: no UI host yet

### Manual checks
- (none)

## Final gates
- `FG-01` — Independent architecture review — deps: P01 — evidence: review
