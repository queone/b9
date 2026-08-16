# Rust Port

## Goal

b9 is the Rust successor to skout. The port preserves supported user workflows while improving isolation, determinism, recovery behavior, testability, and terminal presentation where Rust permits a better design.

Parity is an evidence-backed outcome, not a promise that every predecessor implementation detail will be copied. Observable executable behavior takes priority over documentation; intentional differences must be explicit and tested.

## Current State

b9 has a working Rust CLI, private configuration, isolated SQLite state, versioned snapshots, typed freshness, bounded disk caching, validating HTTP transport, and provider boundaries for Yahoo, MLB, ESPN, and OddsShark.

Available workflows include authentication, league selection, foreground synchronization, a baseline matchup view, glossary lookup, MLB team rosters and totals, and probable-pitcher odds context. The MLB workflow can use a guarded one-time import of compatible local skout state when authenticated Yahoo data is unavailable.

Yahoo's current app-access delay prevents live verification of some fantasy refresh paths. It does not block fixture-backed implementation or non-Yahoo MLB workflows.

## Remaining Port Work

The active delivery order lives in [`../plan.md`](../plan.md). Each idea pointer links to its scoped draft contract.

The remaining work is organized as product-sized verticals:

- Roster and player-pool workflows.
- Complete matchup modes and contextual advice.
- Transaction and roster evaluation workflows.
- Operational commands and retained provider integrations.
- Final behavioral, display, documentation, and architecture closure.

The temporary local Yahoo snapshot-import idea is deferred until a future decision.

## Design Boundaries

- Keep provider acquisition, persistence, orchestration, domain logic, rendering, and CLI parsing separate.
- Keep external data typed at the provider boundary and preserve unknown compatibility values until an owning adapter validates them.
- Keep complete snapshots atomic and retain the last usable state after failed refreshes.
- Keep credentials, tokens, URLs, and raw provider payloads out of durable diagnostic surfaces.
- Keep terminal detection outside deterministic view rendering.
- Keep b9 state owned by b9; treat predecessor state only as guarded, read-only compatibility input.
- Keep official Yahoo API synchronization separate from any temporary local-data bridge.

## Reference Map

- [`skout-parity.md`](skout-parity.md) — source baseline, evidence policy, capability taxonomy, and conflict ledger.
- [`skout-cli-operations.md`](skout-cli-operations.md) — command and operational behavior inventory.
- [`skout-providers-storage.md`](skout-providers-storage.md) — provider, cache, persistence, freshness, and synchronization inventory.
- [`skout-analysis-display-advisory.md`](skout-analysis-display-advisory.md) — analysis, display, advisory, and replacement-readiness inventory.
- [`../arch.md`](../arch.md) — delivered Rust architecture and stable implementation decisions.
- [`api-yahoo.md`](api-yahoo.md), [`api-mlbam.md`](api-mlbam.md), [`api-espn.md`](api-espn.md), and [`api-oddsshark.md`](api-oddsshark.md) — provider contracts used by b9.
