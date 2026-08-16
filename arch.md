# b9 Architecture

## MLB utility workflow

The CLI routes `t`, `tt`, and `sp` into `mlb_commands`, which owns foreground freshness, bounded provider composition, complete snapshot selection, and recovery guidance. MLB, ESPN, and OddsShark adapters own request and decoding details; CLI and rendering modules import no transport payloads or SQLite APIs. A guarded compatibility adapter opens `~/.config/skout/skout.db` read-only only when b9 lacks Yahoo-linked state, transactionally maps compatible rows into b9's schema-version-one store, preserves distinct Yahoo and MLB seed identities, and records completion in `sync_log` so the source is not reopened.

The store reuses schema version one for MLB identities, role-distinct 40-man roster rows, and season statistics. Standings, team directories, rendered-command inputs, and future odds use versioned command snapshots where no normalized table exists. Roster replacement and stale fallback are team-scoped so one failed club does not discard other refreshed clubs.

`mlb_display` consumes provider-neutral roster, standings, totals, and one-row-per-game slate records while preserving skout's terminal information hierarchy. The store projects durable season statistics and optional local Yahoo rank, eligibility, ownership, and current-team context into those records; the command layer performs no Yahoo request. Shared terminal roles provide ANSI-safe active, injured, off-active, available, and current-roster styling with plain output. The probable-pitcher workflow uses MLB schedules, ESPN only for the current host-local day, and OddsShark only for future days; optional odds never own command success.

## Purpose

Port `skout` from Go to Rust over multiple releases.

## System Summary

The repository contains a metadata-driven Rust CLI, reusable domain records, an embedded glossary, private configuration, isolated SQLite persistence, bounded caching and HTTP, typed Yahoo, ESPN, and MLB adapters, foreground fantasy synchronization, and a baseline weekly matchup surface. Remaining commands, background operation, deeper matchup modes, scrapers, and advisory analysis remain later parity work.

## Current Platform

- Rust

## Major Components

- `src/main.rs`: Rust executable entry point and independently versioned utility declaration
- `src/providers/espn.rs`: injected ESPN scoreboard and moneyline acquisition with typed decoding and structured partial failures
- `src/providers/mlb.rs`: injected MLB metadata, live-game, statistics, game-log, and quality-start acquisition with bounded batching and short-lived raw-payload caching
- `src/providers/yahoo.rs`: injected Yahoo OAuth, secure token refresh, and authenticated bounded raw acquisition
- `src/providers/yahoo_fantasy.rs`: typed Yahoo league, team, roster, scoreboard, and weekly-stat acquisition
- `src/providers/mod.rs`: provider boundary exports and contextual acquisition errors
- `src/cache.rs`: bounded b9-owned provider payload caching, atomic replacement, and explicit pruning
- `src/cli.rs`: root command metadata, parsing, dispatch, streams, and exit behavior
- `src/config.rs`: private atomic selected-league and authenticated-team preferences
- `src/sync.rs`: login, logout, status, and foreground stable-data synchronization application services
- `src/matchup.rs`: lazy weekly acquisition, snapshot fallback, baseline view models, and terminal rendering
- `src/glossary.rs`: embedded glossary parsing, lookup, suggestions, and plain-text rendering
- `src/store.rs`: isolated SQLite ownership, schema migration, inspection, and transaction boundary
- `src/store/schema.sql`: embedded b9 schema-version-one table definitions
- `src/store/freshness.rs`: typed item and row freshness policies and lifecycle state
- `src/store/odds.rs`: validated atomic moneyline replacement and typed game-scoped reads
- `src/store/snapshots.rs`: validated durable command snapshots and stale metadata
- `src/store/seasons.rs`: typed source-season completeness manifests
- `src/store/sync_runs.rs`: typed synchronization-run lifecycle and deterministic counts
- `src/terminal.rs`: deterministic terminal-color policy and CLI presentation styles
- `src/transport.rs`: validating synchronous HTTP client, injectable executor, and blocking Rustls implementation
- `src/lib.rs`: reusable Rust library root
- `src/domain.rs`: provider-neutral fantasy-baseball domain records and invariants
- `build.sh`: canonical build, validation, and release workflow
- `tests/build_cli.sh`: regression coverage for the build CLI
- `tests/domain.rs`: public domain-contract regression coverage
- `tests/store.rs`: public persistence-contract regression coverage
- `govna/`: governance and delivery-cycle documentation

## Core Files

- `AGENTS.md`: base governance contract
- `plan.md`: prioritized roadmap and approved direction
- `build.sh`: Bash 3.2-compatible build, validation, and release orchestrator for the Rust toolchain
- `govna/development-cycle.md`: workflow from roadmap through release
- `govna/ac-template.md`: acceptance-criteria template for new work
- `govna/build-release.md`: build, test, and release rules

## Data And Control Flow

Provider adapters construct owned acquisition records without performing orchestration or persistence. The Yahoo authentication adapter owns PKCE, credentials, refresh, request construction, retries, and terminal access errors. The Yahoo fantasy adapter interprets numeric-key and array-or-object payloads into provider-neutral league, team, roster, matchup, and weekly-stat records. Foreground synchronization validates a complete stable Yahoo snapshot before one normalized replacement, then reconciles unique Yahoo-to-MLB identities without overwriting prior mappings. The matchup application owns 60-second lazy Yahoo refreshes, versioned durable fallback, optional MLB schedule and ESPN moneyline enrichment, view assembly, warnings, and rendering.

The persistence core owns one connection to `$HOME/.config/b9/b9.db`, applies ordered migrations atomically, and exposes immediate transactions without exposing its connection. An injected thread-safe clock makes freshness, lifecycle, cache, and odds writes deterministic. The disk cache stores bounded opaque payload bytes under hashed logical keys and replaces entries atomically. Typed persistence and transport APIs return contextual failures instead of silently interpreting operational errors as missing state. Analysis, view-model, display, advisory, and CLI layers consume domain records without placing provider, storage, serialization, or terminal mechanics in the domain module.

The executable passes its literal utility version into one CLI metadata model, whose shared descriptors drive parsing and b9 root help. Package updates that literal together with Cargo package and lockfile versions, and canonical validation rejects disagreement. The CLI dispatches authentication, status, sync, and matchup work into application modules rather than provider or storage internals. A terminal presentation boundary enables contracted color roles only for supported terminal stdout and otherwise renders deterministic plain output.

## AC Lifecycle Control Flow

The governed change path is `Draft → Audit → Refine → Implement → Ratify → Package`. Draft creates the AC; Audit, Refine, Implement, and Ratify are the four AC phases; Package is post-Ratify release preparation and is not a fifth phase.

## Architecture Notes

- record stable system decisions here
- prefer durable structure and interfaces over transient implementation detail
- Keep `src/domain.rs` independent from provider, persistence, serialization, analysis, and rendering dependencies.
- Preserve unknown external scoring and position values losslessly until an owning adapter validates them.
- Keep domain collections non-null and preserve source order.
- Keep the glossary embedded and read-only at runtime.
- Keep command parsing and help generated from one metadata model.
- Keep terminal detection outside deterministic presentation rendering.
- Keep b9 storage isolated from the predecessor database.
- Keep schema migrations and version updates inside one immediate transaction.
- Keep the owned SQLite connection behind the storage transaction boundary.
- Keep provider source identity explicit instead of inferring it from item names.
- Keep predecessor freshness fallback out of the isolated b9 database.
- Keep successful timestamps and snapshot payloads unchanged across failed refreshes.
- Validate durable JSON before replacing the prior successful snapshot.
- Keep provider TTL values and fallback selection outside the persistence layer.
- Keep cache keys free of URLs, credentials, tokens, and secrets.
- Keep cache pruning explicit and independent from successful cache writes.
- Keep provider adapters behind the validating HTTP client boundary.
- Keep provider authentication, endpoints, retries, parsing, and error interpretation outside shared transport.
- Keep provider acquisition separate from normalized persistence and command orchestration.
- Keep partial provider degradation typed until an integration layer selects warnings or fallback.
- Keep odds freshness, team mapping, stale fallback, and snapshots outside the ESPN adapter.
- Keep MLB display-time status ranking, timezone formatting, doubleheader selection, team mapping, reconciliation, and normalized writes outside the MLB adapter.
- Keep Yahoo fantasy parsing outside the Yahoo authentication adapter.
- Keep stable normalized synchronization separate from lazy weekly matchup acquisition.
- Keep weekly scoreboards and roster statistics in versioned snapshots rather than new normalized tables.
- Keep synchronization foreground-owned until background operation demonstrates product value.

## Conventions

- update this document when architecture or major workflow changes materially
- keep implementation detail in code and stable architecture here
