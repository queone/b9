# b9 Architecture

## Purpose

Port `skout` from Go to Rust over multiple releases.

## System Summary

The repository contains a metadata-driven Rust CLI, a reusable domain library, an embedded read-only glossary, an isolated SQLite persistence core, a bounded disk cache, an injectable synchronous HTTP boundary, an ESPN JSON adapter, typed odds persistence, and the build and governance infrastructure for subsequent porting work. Runtime orchestration and the remaining provider integrations remain to be established by later acceptance-criteria cycles.

## Current Platform

- Rust

## Major Components

- `src/main.rs`: Rust executable entry point and independently versioned utility declaration
- `src/providers/espn.rs`: injected ESPN scoreboard and moneyline acquisition with typed decoding and structured partial failures
- `src/providers/mod.rs`: provider boundary exports and contextual acquisition errors
- `src/cache.rs`: bounded b9-owned provider payload caching, atomic replacement, and explicit pruning
- `src/cli.rs`: root command metadata, parsing, dispatch, streams, and exit behavior
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

Provider adapters construct owned acquisition records without performing orchestration or persistence. The ESPN adapter submits provider-owned URLs and limits through `HttpClient`, decodes bounded JSON into typed game lines, aborts on scoreboard failure, and reports per-game odds degradation structurally. Injected executors keep network I/O deterministic without moving endpoints, parsing, or failure policy into shared transport. The typed odds store validates complete replacements before capturing its injected clock, replaces only affected moneyline rows atomically, and preserves unrelated markets. Later integration maps ESPN teams to MLB game identifiers and owns freshness, stale fallback, snapshots, warnings, and display.

The persistence core owns one connection to `$HOME/.config/b9/b9.db`, applies ordered migrations atomically, and exposes immediate transactions without exposing its connection. An injected thread-safe clock makes freshness, lifecycle, cache, and odds writes deterministic. The disk cache stores bounded opaque payload bytes under hashed logical keys and replaces entries atomically. Typed persistence and transport APIs return contextual failures instead of silently interpreting operational errors as missing state. Analysis, view-model, display, advisory, and CLI layers consume domain records without placing provider, storage, serialization, or terminal mechanics in the domain module.

The executable passes its utility version into one CLI metadata model, whose shared descriptors drive parsing and b9 root help. A terminal presentation boundary enables the contracted 256-color roles only for supported terminal stdout and otherwise renders the byte-equivalent plain layout. The canonical `i` command dispatches into the library, where `docs/glossary.md` is embedded at compile time and parsed without filesystem or network access. Plain-text glossary rendering remains separate from lookup behavior so a later terminal adapter can add interactive selection and presentation.

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

## Conventions

- update this document when architecture or major workflow changes materially
- keep implementation detail in code and stable architecture here
