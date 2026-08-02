# Model and cost policy

## Preferred models (runtime IDs from `agent --list-models`)

Tiamat only uses **Cursor Composer** and **Cursor Grok**. SOL and other families are never selected.

| Role | Preferred ID |
|---|---|
| Initial architect | `cursor-grok-4.5-high` |
| Tiny / mechanical implementation | `composer-2.5` |
| Small bounded implementation | `cursor-grok-4.5-low` |
| Normal implementation / debugging | `cursor-grok-4.5-medium` |
| Complex work, escalation, final reviews | `cursor-grok-4.5-high` |

Unavailable preferred IDs trigger a deterministic Composer/Grok fallback recorded in events — never an unrelated guessed model. Fast variants are not selected by default.

## Attempt budget and escalation

Default phase budget: **20 minutes** (architect **30 minutes**); env-overridable via `TIAMAT_PHASE_TIMEOUT_MS` / `TIAMAT_ARCHITECT_TIMEOUT_MS`.

Default escalation: Composer → Grok Low → Grok Medium → Grok High (max three attempts). At Grok High, one same-tier resume is allowed if useful progress exists; further timeout fails the phase. Deterministic policy/auth/build failures do not consume blind model escalations.

## Spending consent

Before the first real run for each Cursor executable/account capability hash, Tiamat requires a disposable lowest-cost contract canary with explicit spending consent. Deterministic demos and CI never call paid models.

```bash
npm run test:canary
```

Requires local spending-consent environment variables; never part of CI.

## Visibility

Model routing, substitutions, attempt costs/usage (when emitted), and escalations appear in the activity log and run reports.
