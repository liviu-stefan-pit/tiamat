# Model and cost policy

## Preferred models (runtime IDs from `agent --list-models`)

| Role | Preferred ID |
|---|---|
| Initial architect only | `gpt-5.6-sol-high` |
| Tiny / mechanical implementation | `composer-2.5` |
| Small bounded implementation | `cursor-grok-4.5-low` |
| Normal implementation / debugging | `cursor-grok-4.5-medium` |
| Complex work, escalation, final reviews | `cursor-grok-4.5-high` |

No implementation or review phase may use SOL. If SOL is unavailable for architecture, Tiamat uses available Grok High and records degraded mode, or fails preflight if no allowed high tier exists.

Unavailable preferred IDs trigger a deterministic Composer/Grok fallback recorded in events — never an unrelated guessed model. Fast variants are not selected by default.

## Attempt budget and escalation

Default attempt watchdog: **10 minutes** (warning at 8 minutes). After timeout: graceful stop → 15 s grace → terminate Job Object → resume same chat with next allowed model.

Default escalation: Composer → Grok Low → Grok Medium → Grok High (max four attempts). At Grok High, one same-tier resume is allowed if useful progress exists; further timeout fails the phase. Deterministic policy/auth/build failures do not consume blind model escalations.

## Spending consent

Before the first real run for each Cursor executable/account capability hash, Tiamat requires a disposable lowest-cost contract canary with explicit spending consent. Deterministic demos and CI never call paid models.

```powershell
npm run test:canary
```

Requires local spending-consent environment variables; never part of CI.

## Visibility

Model routing, substitutions, attempt costs/usage (when emitted), and escalations appear in the activity log and run reports.
