# AC20 Transactions And Roster Evaluation

## Summary

Complete transaction-oriented fantasy analysis in Rust. Deliver add/drop discovery, waiver and transaction context, roster quality evaluation, trade evaluation, PQS/PQT/StartHoldScore computation, and the player-ranking engines needed to support those decisions.

## In Scope

### Files to create

- `src/evaluation.rs` — rank players and evaluate rosters, drops, additions, and trades.
- `src/signals.rs` — compute PQS, PQT, and StartHoldScore from durable evaluation inputs.
- `src/transaction_commands.rs` — orchestrate settled transaction and evaluation command surfaces.
- `src/transaction_display.rs` — render rankings, candidates, transactions, and evaluations.
- `tests/evaluation.rs` — verify scoring, ranking, and recommendation behavior.
- `tests/signals.rs` — verify PQS, PQT, StartHoldScore, and missing-input behavior.
- `tests/transaction_commands.rs` — verify end-to-end command behavior.

### Files to modify

- `src/cli.rs` — expose every settled transaction and evaluation command and flag.
- `src/main.rs` — dispatch transaction and evaluation workflows.
- `src/lib.rs` — export evaluation and transaction modules.
- `src/domain.rs` — add ranking, transaction, roster-evaluation, and trade types.
- `src/providers/yahoo_fantasy.rs` — acquire waiver, transaction, roster, and ownership inputs.
- `src/store/fantasy.rs` — persist normalized transaction and evaluation inputs.
- `src/store/schema.sql` — add only normalized state required by these workflows.
- `src/sync.rs` — synchronize transaction inputs and complete snapshots.
- `src/terminal.rs` — support shared evaluation rendering.
- `src/player_commands.rs` — attach computed StartHoldScore to the roster read model.
- `src/player_display.rs` — render the settled StartHoldScore roster column.
- `tests/b9_cli.rs` — verify command contracts and failures.
- `tests/providers_yahoo_fantasy.rs` — verify transaction and waiver response handling.
- `tests/store_fantasy.rs` — verify transaction persistence and reconciliation.
- `tests/sync.rs` — verify freshness and provider-failure behavior.
- `tests/player_commands.rs` — verify roster StartHoldScore availability and recovery behavior.
- `tests/player_display.rs` — verify StartHoldScore column rendering.
- `docs/api-yahoo.md` — document waiver and transaction contracts.
- `docs/skout-cli-operations.md` — record resolved command parity.
- `docs/skout-analysis-display-advisory.md` — record resolved ranking and evaluation behavior.
- `arch.md` — describe transaction ingestion and evaluation boundaries.

### Schema changes

- Add a monotonic schema migration for transaction history and any durable evaluation inputs identified during Audit.

## Out Of Scope

- Defer base roster and player-pool browsing to the [roster and player-pool pointer](../plan.md), except for the settled StartHoldScore roster column.
- Defer matchup-specific advice to the [matchup advisory pointer](../plan.md).
- Defer daemon operation and unrelated provider enrichment to the [operations and provider completion pointer](../plan.md).
- Defer final cosmetic and documentation closure to the [parity closure pointer](../plan.md).

## Migration findings

- Determine legacy ranking-model and transaction-history migration requirements during Audit.
- Determine durable PQS, PQT, and StartHoldScore input and output requirements during Audit.

## Acceptance Tests

**AT1** [Automated] [Pre-release gate] — Verify settled transaction and evaluation commands operate from normalized fixture data.

**AT2** [Automated] [Pre-release gate] — Verify rankings, PQS, PQT, StartHoldScore, add/drop candidates, roster evaluations, trade evaluations, and the settled roster column are deterministic and explain their inputs.

**AT3** [Automated] [Pre-release gate] — Verify transaction snapshots reconcile atomically and retain the last complete usable state after failure.

**AT4** [Manual] [Post-release verification] — Compare representative live transaction and evaluation workflows with skout after Yahoo Fantasy API access is available.

## Status

`PENDING` — awaiting user authorization to begin Audit.
