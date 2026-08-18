# b9 Architecture

## MLB utility workflow

The CLI routes `t`, `tt`, and `sp` into `mlb_commands`, which owns foreground freshness, bounded provider composition, complete snapshot selection, and recovery guidance. MLB, ESPN, and OddsShark adapters own request and decoding details; CLI and rendering modules import no transport payloads or SQLite APIs.

The store uses schema version two for MLB identities, role-distinct 40-man roster rows, season statistics, and league-scoped free agents. Standings, team directories, rendered-command inputs, and future odds use versioned command snapshots where no normalized table exists. Roster replacement and stale fallback are team-scoped so one failed club does not discard other refreshed clubs.

`mlb_display` consumes provider-neutral roster, standings, totals, and one-row-per-game slate records while preserving b9's terminal information hierarchy. The store projects durable season statistics and optional local Yahoo rank, eligibility, ownership, and current-team context into those records; the command layer performs no Yahoo request. Shared terminal roles provide ANSI-safe active, injured, off-active, available, and current-roster styling with plain output. The probable-pitcher workflow uses MLB schedules, ESPN only for the current host-local day, and OddsShark only for future days; optional odds never own command success.

## Purpose

Maintain a local-first Rust fantasy-baseball utility.

## System Summary

The repository contains a metadata-driven Rust CLI, reusable domain records, an embedded glossary, private configuration, isolated SQLite persistence, bounded caching and HTTP, typed Yahoo, ESPN, and MLB adapters, foreground fantasy synchronization, operational utilities, and daily or weekly matchup surfaces. Rejected scraping providers and deeper analysis remain outside the current port.

## Current Platform

- Rust

## Major Components

- `src/main.rs`: Rust executable entry point and independently versioned utility declaration
- `src/providers/espn.rs`: injected ESPN scoreboard and moneyline acquisition with typed decoding and structured partial failures
- `src/providers/mlb.rs`: injected MLB metadata, live-game, statistics, game-log, and quality-start acquisition with bounded batching and short-lived raw-payload caching
- `src/providers/yahoo_public.rs`: bounded unauthenticated Yahoo league, roster, free-agent, scoreboard, weekly-stat, rank, and redzone acquisition
- `src/providers/yahoo_fantasy.rs`: shared typed Yahoo payload models and normalization
- `src/providers/mod.rs`: provider boundary exports and contextual acquisition errors
- `src/cache.rs`: bounded b9-owned provider payload caching, atomic replacement, and explicit pruning
- `src/cli.rs`: root command metadata, parsing, dispatch, streams, and exit behavior
- `src/config.rs`: private atomic selected-league and primary-team preferences
- `src/sync.rs`: public-only setup, status, foreground synchronization, and persistent cross-process synchronization locking
- `src/operations.rs`: confirmed database reset
- `src/matchup.rs`: public selected-period Yahoo acquisition, daily MLB-stat overlays, snapshot fallback, and terminal rendering
- `src/evaluation.rs`: deterministic durable-season ranking used by roster and waiver ordering
- `src/glossary.rs`: embedded glossary parsing, lookup, suggestions, and plain-text rendering
- `src/store.rs`: isolated SQLite ownership, schema migration, inspection, and transaction boundary
- `src/store/schema.sql`: embedded b9 schema-version-two table definitions
- `src/store/freshness.rs`: typed item and row freshness policies and lifecycle state
- `src/store/odds.rs`: validated atomic moneyline replacement and typed game-scoped reads
- `src/store/snapshots.rs`: validated durable command snapshots and stale metadata
- `src/store/seasons.rs`: typed source-season completeness manifests
- `src/store/sync_runs.rs`: typed synchronization-run lifecycle and deterministic counts
- `src/terminal.rs`: deterministic terminal-color policy, b9 256-color roles, ANSI-safe visible widths, and plain fallback
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

Provider adapters construct owned acquisition records without performing orchestration or persistence. The public Yahoo adapter owns exact allowlisted paths, bounded requests and pagination, and numeric-key or array-or-object normalization into provider-neutral league, team, roster, matchup, and weekly-stat records. Foreground synchronization stages settings, standings, complete rosters, free agents, and primary-team validation before one atomic fantasy-snapshot replacement. Transaction history and roster mutations remain unimplemented. The matchup application owns lazy public Yahoo refreshes, ISO-date-to-week resolution, required MLBAM identity reconciliation, daily MLB-stat overlays, versioned durable fallback, optional MLB schedule and ESPN moneyline enrichment, warnings, and rendering.

The roster and player-pool commands read normalized Yahoo teams, ownership, free agents, and MLB season statistics from the isolated store. Fantasy roster totals join statistics through MLBAM identity, preserving the predecessor's shared-identity aggregation for split two-way players. MLB totals refreshes supplement the bulk pitching feed with per-starter quality starts, and zero-valued bulk omissions cannot erase a previously acquired nonzero total. MLB innings retain source display notation for aggregate parity while rate calculations use true thirds internally. Synchronization fetches free agents as a bounded paginated complete set before atomic replacement. Player cards use the MLB game-log adapter as a foreground refresh path and retain a versioned per-player snapshot so a labeled compatible fallback remains available during provider failure. Primary player, roster, matchup, team, totals, and slate tables use fixed source-compatible column geometry where the Rust model owns the corresponding data; deferred analytical and rich-status cells remain documented gaps. Headers use blue 33, secondary values gray 245, available players green 34, and inactive or injured rows use the established gray and dark-yellow tiers before falling back to identical plain text.

Weekly roster totals use Yahoo matchup category values in the stored league order and retain weekly snapshots for stale fallback. Waiver filtering uses the durable active-26-man membership plus the predecessor-compatible 60th-percentile usage floors, keeping active-roster interpretation in the store and selection policy in the command layer.

The persistence core owns one connection to `$HOME/.config/b9/b9.db`, applies ordered migrations atomically, and exposes immediate transactions without exposing its connection. An injected thread-safe clock makes freshness, lifecycle, cache, and odds writes deterministic. The disk cache stores bounded opaque payload bytes under hashed logical keys and replaces entries atomically. Typed persistence and transport APIs return contextual failures instead of silently interpreting operational errors as missing state. Analysis, view-model, display, and CLI layers consume domain records without placing provider, storage, serialization, or terminal mechanics in the domain module.

The executable passes its literal independent utility version into one CLI metadata model, whose shared descriptors drive parsing and b9 root help. Package preserves that independent declaration while updating Cargo package and lockfile versions, and canonical validation checks each version against its own contract. The CLI dispatches status, synchronization, operations, and matchup work into application modules rather than provider or storage internals. Foreground synchronization holds a persistent cross-process file lock at the predecessor-compatible path. Reset deletes only the local database and preserves configuration, cache, historical log data, and predecessor files. A terminal presentation boundary enables contracted color roles only for supported terminal stdout and otherwise renders deterministic plain output.

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
- Keep provider endpoints, retries, parsing, and error interpretation outside shared transport.
- Keep provider acquisition separate from normalized persistence and command orchestration.
- Keep partial provider degradation typed until an integration layer selects warnings or fallback.
- Keep odds freshness, team mapping, stale fallback, and snapshots outside the ESPN adapter.
- Keep MLB display-time status ranking, timezone formatting, doubleheader selection, team mapping, reconciliation, and normalized writes outside the MLB adapter.
- Keep Yahoo fantasy parsing shared by the public Yahoo adapter and command snapshots.
- Keep stable normalized synchronization separate from lazy weekly matchup acquisition.
- Keep weekly scoreboards and roster statistics in versioned snapshots rather than new normalized tables.
- Keep synchronization foreground-only and explicitly invoked.
- Keep foreground synchronization behind one persistent cross-process execution lock.
- Keep rejected automated provider acquisition unreachable from commands, synchronization, transport, and adapters.

### Status dashboard boundary

- Keep status rendering local-first and read-only with respect to Yahoo provider traffic.
- Store dashboard lifecycle, provider freshness, bounded failure, and circuit fields in the versioned b9 database.
- Keep status local-only and Yahoo synchronization public-only.
- Classify public Yahoo denial as provider unavailability with retry-later guidance and no authentication recovery path.

### Public Yahoo boundary

- Fetch only exact allowlisted paths from Yahoo's two public fantasy hosts with no cookies, authorization header, or browser.
- Require an explicit league and primary-team selection when account-scoped discovery is unavailable.
- Replace the prior complete Yahoo fantasy snapshot only after all required public resources and the selected team validate.
- Preserve historical `public_pull` origin decoding while writing no new run with that origin.
- Treat the public endpoints as unofficial and retain complete stale data across denial or incompatible responses.
- Merge public league, team, roster, and player fields without erasing retained authenticated-only values.
- Apply authenticated scoring metadata, standings metadata, precise roster metadata, free agents, and team identity when those requests succeed.
- Keep transaction-history and roster-move acquisition unimplemented until a later approved provider contract.
- Track standalone pulls with `SyncOrigin::PublicPull` and keep public and authenticated matchup snapshots distinct for freshness and stale-fallback selection.
- Render Yahoo-redacted fields (e.g. manager nicknames) as an explicit placeholder; resolve them only through authenticated supplementation, never by inference.
- Never evade Yahoo's anti-automation measures; only ever request the operator's own configured league.

## Conventions

- update this document when architecture or major workflow changes materially
- keep implementation detail in code and stable architecture here
