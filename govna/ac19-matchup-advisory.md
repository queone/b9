# AC19 Matchup Advisory

## Summary

Complete the matchup workflow in Rust. Extend `m` with its daily, weekly, period-selection, category-gap, projection, and contextual recommendation behaviors while retaining deterministic output and durable fallback.

## In Scope

### Files to create

- `src/advisory.rs` — compute matchup category gaps, projections, and contextual recommendations.
- `tests/advisory.rs` — verify advisory calculations and evidence grounding.

### Files to modify

- `src/cli.rs` — expose every settled `m` mode and flag.
- `src/main.rs` — dispatch complete matchup modes.
- `src/lib.rs` — export advisory behavior.
- `src/domain.rs` — add matchup projection and recommendation types.
- `src/matchup.rs` — orchestrate daily, weekly, and selected-period matchup views.
- `src/providers/yahoo_fantasy.rs` — acquire matchup inputs not delivered by the baseline workflow.
- `src/store/fantasy.rs` — persist normalized matchup inputs and complete snapshots.
- `src/store/schema.sql` — add only state required for complete matchup behavior.
- `src/sync.rs` — synchronize required matchup inputs with freshness and fallback semantics.
- `src/terminal.rs` — render complete matchup and advice surfaces.
- `tests/b9_cli.rs` — verify matchup help, dispatch, and recovery guidance.
- `tests/matchup.rs` — verify every settled matchup mode.
- `tests/providers_yahoo_fantasy.rs` — verify additional matchup acquisition and normalization.
- `tests/store_fantasy.rs` — verify matchup persistence and reconciliation.
- `tests/sync.rs` — verify refresh and stale-fallback behavior.
- `docs/api-yahoo.md` — document additional matchup inputs.
- `docs/skout-cli-operations.md` — record resolved matchup command parity.
- `docs/skout-analysis-display-advisory.md` — record resolved analytical and advisory behavior.
- `arch.md` — describe matchup analysis and advisory flow.

### Schema changes

- Add a monotonic schema migration only when Audit confirms additional normalized matchup state is required.

## Out Of Scope

- Defer general player-pool and roster commands to the [roster and player-pool pointer](../plan.md).
- Defer add/drop, transaction, and trade evaluation to the [transactions and roster evaluation pointer](../plan.md).
- Defer background operation and provider completion to the [operations and provider completion pointer](../plan.md).
- Defer non-blocking cosmetic parity to the [parity closure pointer](../plan.md).

## Migration findings

- Determine whether existing matchup snapshots require migration during Audit.

## Acceptance Tests

**AT1** [Automated] [Pre-release gate] — Verify every settled `m` mode and flag selects the correct matchup period and dataset.

**AT2** [Automated] [Pre-release gate] — Verify category gaps, projections, and recommendations are deterministic and grounded in displayed inputs.

**AT3** [Automated] [Pre-release gate] — Verify complete snapshots, freshness gates, and stale fallback never combine incompatible matchup periods.

**AT4** [Manual] [Post-release verification] — Compare representative live matchup and advice output with skout after Yahoo Fantasy API access is available.

## Status

`PENDING` — awaiting user authorization to begin Audit.
