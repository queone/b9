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

## Independent Review — 2026-08-16

Findings from an ad hoc repository review against the stated port direction and the pending AC19–AC23 roadmap. Ordered most to least significant.

- **The Ratify gate did not catch defects that reached Package.** AC18's Package attempt surfaced four real defects only after Ratify had already been treated as complete: a stale exact-schema test (`tests/store.rs`) missing the new `yahoo_free_agents` table; an explicit `CREATE INDEX` added in `schema.sql`/`store.rs` that violates the repo's existing no-explicit-index convention and was unused by every query against the table; a new `tests/store_fantasy.rs` test that violated a pre-existing (AC16-era) roster-ownership invariant; and a genuine runtime bug in `src/store/fantasy.rs`'s `fantasy_players()` — it bound two positional parameters against a query whose SQL reuses a single `?1` placeholder, so the query would fail with `InvalidParameterCount` on every real call (i.e., every live `h`/`p` invocation once free agents exist). None of these were cosmetic; the last one was a hard functional break in already-"Ratified" code. Recommend treating a full `./build.sh` pass as a hard Ratify precondition, not just a Package precondition, so defects like this surface one phase earlier.
- **The LLM advisory adapter has no clearly owned implementation slot.** `docs/skout-analysis-display-advisory.md` tracks `ADV-LLM` (skout's OpenAI-backed trade/lineup/waiver advisory behind `--advise/-a`, ~16 production + 9 test files in the Go source) as `Required` for replacement readiness, currently `NOT STARTED`/`NOT READY`, with `SLICE-ADV-ADAPTERS` covering the live adapter, model routing, and key/keychain handling. Across AC19 (`src/advisory.rs` — deterministic category gaps, projections, and recommendations), AC20, AC21 (`src/model_config.rs` — interactive/persisted model *configuration*, not the completion-call path itself), and AC22, no file is explicitly named as the owner of the actual outbound OpenAI request/response adapter. This is architecturally a fifth external provider with a materially different transport, cost, and credential profile than Yahoo/MLB/ESPN/OddsShark, and it should get an explicit owning file named during AC19 or AC21 Audit rather than being implied by adjacent scope.
- **The remaining scraper providers don't yet carry the same policy scrutiny AC23 gave Yahoo.** AC23 deliberately avoids any automated Yahoo page retrieval — it's a manual, local-file-only import path with an explicit no-network acceptance test (AT3). That's a sound design for working around the Yahoo API access delay, not a scraping workaround, despite the "circumvent" framing it's sometimes described with. By contrast, Savant, Fangraphs, FantasyPros, and RotoWire (`docs/skout-providers-storage.md` Provider Contracts) are all unauthenticated HTML/CSV scraping of pages not designed for programmatic access, and AC21 currently defers their inclusion to "Audit proves [no] observable or required replacement behavior" without naming the same ToS/fragility lens applied to Yahoo. Worth carrying that same scrutiny into AC21 Audit explicitly rather than treating it as a pure technical-parity question.

Positive notes, for balance: dependency versions in `Cargo.toml` are pinned exactly throughout; all 13 currently shipped commands (`login`, `logout`, `st`, `sync`, `m`, `t`, `tt`, `sp`, `r`, `rt`, `h`, `p`, `whatis`) are consistently wired between `cli.rs` registration and dispatch; no `TODO`/`FIXME`/`unimplemented!`/`panic!` markers exist in `src/`; and the discovery documentation (`skout-parity.md`, `skout-cli-operations.md`, `skout-providers-storage.md`, `skout-analysis-display-advisory.md`) is unusually thorough, with a machine-checkable coverage manifest and an explicit replacement-readiness matrix that already tracks most of the gaps above as open items rather than silent omissions.
