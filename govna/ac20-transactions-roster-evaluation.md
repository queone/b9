# AC20 Transactions And Roster Evaluation

## Summary

Complete transaction-oriented fantasy analysis in Rust. Deliver add/drop discovery, waiver and transaction context, roster quality evaluation, trade evaluation, and the player-ranking engines needed to support those decisions.

## In Scope

### Files to create

- `src/evaluation.rs` — rank players and evaluate rosters, drops, additions, and trades.
- `src/transaction_commands.rs` — orchestrate settled transaction and evaluation command surfaces.
- `src/transaction_display.rs` — render rankings, candidates, transactions, and evaluations.
- `tests/evaluation.rs` — verify scoring, ranking, and recommendation behavior.
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
- `tests/b9_cli.rs` — verify command contracts and failures.
- `tests/providers_yahoo_fantasy.rs` — verify transaction and waiver response handling.
- `tests/store_fantasy.rs` — verify transaction persistence and reconciliation.
- `tests/sync.rs` — verify freshness and provider-failure behavior.
- `docs/api-yahoo.md` — document waiver and transaction contracts.
- `docs/skout-cli-operations.md` — record resolved command parity.
- `docs/skout-analysis-display-advisory.md` — record resolved ranking and evaluation behavior.
- `arch.md` — describe transaction ingestion and evaluation boundaries.

### Schema changes

- Add a monotonic schema migration for transaction history and any durable evaluation inputs identified during Audit.

## Out Of Scope

- Defer base roster and player-pool browsing to the [roster and player-pool pointer](../plan.md).
- Defer matchup-specific advice to the [matchup advisory pointer](../plan.md).
- Defer daemon operation and unrelated provider enrichment to the [operations and provider completion pointer](../plan.md).
- Defer final cosmetic and documentation closure to the [parity closure pointer](../plan.md).

## Migration findings

- Determine legacy ranking-model and transaction-history migration requirements during Audit.

## Acceptance Tests

**AT1** [Automated] [Pre-release gate] — Verify settled transaction and evaluation commands operate from normalized fixture data.

**AT2** [Automated] [Pre-release gate] — Verify rankings, add/drop candidates, roster evaluations, and trade evaluations are deterministic and explain their inputs.

**AT3** [Automated] [Pre-release gate] — Verify transaction snapshots reconcile atomically and retain the last complete usable state after failure.

**AT4** [Manual] [Post-release verification] — Compare representative live transaction and evaluation workflows with skout after Yahoo Fantasy API access is available.

## Status

`PENDING` — awaiting user authorization to begin Audit.
