# Rust Port

## Goal

b9 is the Rust successor to skout. The port preserves supported user workflows while improving isolation, determinism, recovery behavior, testability, and terminal presentation where Rust permits a better design.

Parity is an evidence-backed outcome, not a promise that every predecessor implementation detail will be copied. Observable executable behavior takes priority over documentation; intentional differences must be explicit and tested.

## Current State

b9 has a complete retained command shell, private configuration, isolated SQLite state, versioned snapshots, typed freshness, bounded disk caching, validating HTTP transport, and provider boundaries for Yahoo, MLB, ESPN, OddsShark, and five advisory backends.

Available workflows include authentication, league selection, foreground and explicitly managed background synchronization, daily and weekly matchup views, roster and player-pool inspection, roster totals, glossary lookup, MLB team rosters and totals, probable-pitcher odds context, raw Yahoo fetch, bounded log inspection, safe local reset, and advisory provider/model configuration. Player and MLB displays use skout-compatible column ordering and semantic color roles with deterministic plain fallback. The MLB workflow can use a guarded one-time import of compatible local skout state when authenticated Yahoo data is unavailable.

Yahoo's current app-access delay prevents live verification of some fantasy refresh paths. It does not block fixture-backed implementation or non-Yahoo MLB workflows.

## Hands-On Command Parity Tracker

Use this table as the working source of truth for ad hoc skout-to-b9 command comparisons. Update a row after manually comparing complete output and behavior, including data, formatting, colors, alignment, flags, errors, and side effects. Treat `99%` as functionally complete with minor parity defects still possible; reserve `100%` for a fresh, complete manual comparison with no known difference.

| b9 command | Workflow | Parity | State | Current focus or known gap |
|---|---|---:|---|---|
| root help / version | Command discovery and version output | — | Not assessed | Compare root help, command help, aliases, flags, streams, and version forms. |
| `login` | Yahoo authentication | — | Not assessed | Compare browser flow, prompts, credential persistence, errors, and recovery. |
| `logout` | Yahoo credential removal | — | Not assessed | Compare output, missing-credential behavior, and keychain effects. |
| `st` | Status and league selection | — | Not assessed | Compare dashboard fields, colors, league selection, freshness, and daemon state. |
| `sync` | Complete foreground synchronization | 99% | Functionally complete | Continue watching live provider completeness, retained stale data, progress output, and runtime. |
| `pp` | Public Yahoo league pull | — | Not assessed | Compare public data coverage, league resolution, output, and persistence effects. |
| `start` | Start background synchronization | — | Not assessed | Compare already-running, stale-state, process, log, and schedule behavior. |
| `stop` | Stop background synchronization | — | Not assessed | Compare stopped, stale-process, timeout, cleanup, and output behavior. |
| `restart` | Restart background synchronization | — | Not assessed | Compare composed stop/start behavior and failure recovery. |
| `log` | Read or follow daemon logs | — | Not assessed | Compare default tail, line count, follow, truncation, path, and missing-log behavior. |
| `reset` | Remove local b9 state | — | Not assessed | Compare confirmation, cancellation, daemon interaction, deletion scope, and output. |
| `fetch` | Raw authenticated Yahoo request | — | Not assessed | Compare path handling, JSON formatting, raw bytes, attribution, and errors. |
| `lm` | Advisory provider and model configuration | — | Not assessed | Compare provider selection, model discovery, credentials, cancellation, and errors. |
| `m` | Daily or weekly fantasy matchup | — | Not assessed | Compare every table, game state, matchup total, advisory, color, flag, and fallback path. |
| `t` | MLB 40-man roster display | — | Not assessed | Compare roster membership, role/status classification, columns, sorting, and colors. |
| `tt` | MLB standings and team totals | — | Not assessed | Compare standings, totals, Yahoo-player counts, sorting, formatting, and freshness. |
| `sp` | Probable-pitcher slate and odds | — | Not assessed | Compare dates, starters, ownership, odds sources, degradation, sorting, and colors. |
| `r` | Fantasy roster display | 99% | Functionally complete | Continue logging minor live-data, status, color, position, and alignment discrepancies. |
| `rt` | Fantasy roster totals | 99% | Functionally complete | Continue logging minor live-data, aggregation, color, and alignment discrepancies. |
| `h` | Hitter pool and hitter detail | — | Not assessed | Compare default list, sorting, position and waiver filters, detail view, data, and colors. |
| `p` | Pitcher pool and pitcher detail | — | Not assessed | Compare default list, sorting, position and waiver filters, detail view, data, and colors. |
| `i` | Glossary lookup | — | Not assessed | Compare full glossary, exact lookup, ambiguity, suggestions, selection, and formatting. |
| `help` | Command help dispatch | — | Not assessed | Compare command routing, command-specific text, unknown commands, streams, and exits. |

## Replacement Boundary

The retained command surface is implemented and deterministic closure is tracked in the four parity documents below. Full replacement readiness remains `NOT READY` while live Yahoo access is unavailable and while required terminal, keychain, model, or advisory-protocol checks remain pending.

Automated Savant, FanGraphs, and FantasyPros HTML acquisition is rejected under the recorded provider-policy decisions. Bounded on-demand RotoWire lineup acquisition is allowed for roster status parity. Yahoo transaction-history acquisition, Statcast-dependent PQS, the undefined PQT formula, StartHoldScore, and their dependent columns remain explicit gaps. The temporary local Yahoo snapshot-import idea remains deferred.

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

## Independent Review — 2026-08-16

Findings from an ad hoc repository review against the stated port direction and the pending AC19–AC23 roadmap. Ordered most to least significant.

- **The Ratify gate did not catch defects that reached Package.** AC18's Package attempt surfaced four real defects only after Ratify had already been treated as complete: a stale exact-schema test (`tests/store.rs`) missing the new `yahoo_free_agents` table; an explicit `CREATE INDEX` added in `schema.sql`/`store.rs` that violates the repo's existing no-explicit-index convention and was unused by every query against the table; a new `tests/store_fantasy.rs` test that violated a pre-existing (AC16-era) roster-ownership invariant; and a genuine runtime bug in `src/store/fantasy.rs`'s `fantasy_players()` — it bound two positional parameters against a query whose SQL reuses a single `?1` placeholder, so the query would fail with `InvalidParameterCount` on every real call (i.e., every live `h`/`p` invocation once free agents exist). None of these were cosmetic; the last one was a hard functional break in already-"Ratified" code. Recommend treating a full `./build.sh` pass as a hard Ratify precondition, not just a Package precondition, so defects like this surface one phase earlier.
- **Resolved operational review findings.** `src/providers/advisory.rs` owns the live five-provider completion, validation, and OpenAI discovery boundary; `src/model_config.rs` owns interactive routing; and `src/advisory_credentials.rs` owns environment-before-keyring secrets. Automated Savant, FanGraphs, and FantasyPros HTML acquisition is rejected under official policy evidence recorded in `skout-providers-storage.md`; bounded on-demand RotoWire lineup acquisition is allowed for roster status parity.

Positive notes, for balance: dependency versions in `Cargo.toml` are pinned exactly throughout; all currently shipped commands are consistently wired between `cli.rs` registration and dispatch; no `TODO`/`FIXME`/`unimplemented!`/`panic!` markers exist in `src/`; and the discovery documentation (`skout-parity.md`, `skout-cli-operations.md`, `skout-providers-storage.md`, `skout-analysis-display-advisory.md`) is unusually thorough, with a machine-checkable coverage manifest and an explicit replacement-readiness matrix that already tracks most gaps as open items rather than silent omissions.
