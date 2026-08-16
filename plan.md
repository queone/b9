# b9 Plan

- Deliver the Yahoo-network-independent MLB utility workflow through `t`, `tt`, and `sp`, including guarded one-time legacy local-context import, typed roster/stat persistence, durable fallback, current ESPN moneylines, future OddsShark moneylines, and shared terminal rendering.

## Product Direction

Port `skout` from Go to Rust across multiple releases. Treat feature parity and readiness to supplant `skout` as verification outcomes rather than schedule assumptions.

## Parity Discovery

Use [`docs/skout-parity.md`](docs/skout-parity.md) as the source catalog and evidence policy. Complete detailed discovery in outside-in dependency order, starting with observable contracts rather than implementation dependencies:

1. CLI and operations
2. Providers and storage
3. Analysis, display, and advisory

Ratify the applicable detailed inventory before implementing its Rust capabilities. Resolve every documented behavior conflict explicitly; do not infer parity from documentation alone.

CLI and operations detailed inventory: [`docs/skout-cli-operations.md`](docs/skout-cli-operations.md). Current executable behavior remains required parity; implementation stays blocked until Ratify.

Providers and storage detailed inventory: [`docs/skout-providers-storage.md`](docs/skout-providers-storage.md). Current executable behavior remains required parity; implementation stays blocked until Ratify.

Analysis, display, and advisory detailed inventory: [`docs/skout-analysis-display-advisory.md`](docs/skout-analysis-display-advisory.md). Detailed parity discovery is complete; current executable behavior remains required parity, and implementation stays blocked until Ratify.

## Implementation Progress

- Implement PS-1 isolated persistence ownership and schema migration.
- Implement PS-2 typed freshness, snapshots, season state, and synchronization runs.
- Implement PS-3 bounded atomic caching and validating injectable HTTP transport.
- Implement the ESPN PS-4 sub-slice with typed JSON acquisition and atomic moneyline persistence.
- Implement the bounded MLB PS-4 metadata and live-game sub-slice with typed acquisition, identity batching, and schedule caching.
- Implement the bounded MLB PS-4 statistics sub-slice with typed bulk identity, date-range caching, game logs, and deterministic quality-start aggregation.
- Implement the Yahoo PS-4 authentication and authenticated raw-request foundation with secure credentials and single-flight refresh.
- Implement the first vertical fantasy workflow with `login`, `logout`, `st`, foreground `sync`, baseline `m`, Yahoo normalization, stable persistence, durable weekly fallback, MLB identity reconciliation, and optional ESPN odds context.
- Defer remaining command integration, deeper matchup modes, background operation, remaining providers, scrapers, analysis, and advisory work to later governed ACs.

## Ideas To Explore

Ideas captured for future reference. A bullet list — each line starts with `- IE<N>: ` (sequential N) for stable references. Two kinds: (a) **pre-rubric IE** — `IE<N>: <one-liner>`, awaiting director discussion and the objective-fit rubric (see `AGENTS.md` Approval Boundaries); (b) **AC-pointer** — `IE<N>: <one-liner> → govna/ac<N>-<slug>.md`, pointing at a drafted AC stub not yet through critique. A pre-rubric entry that clears the rubric converts to an AC-pointer at AC-draft time, keeping its `IE<N>` number. Remove entries when the idea is rejected, retired, or (for AC-pointers) the AC has shipped and its file deleted. Not a historical record.
