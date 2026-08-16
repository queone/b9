# AC21 Operations And Provider Completion

## Summary

Complete the Rust operational surface and remaining provider integrations required by observable skout behavior. Deliver foreground utilities, managed background operation, logging and recovery commands, model configuration, and the remaining evidence-backed enrichment sources.

## In Scope

### Files to create

- `src/operations.rs` — implement fetch, reset, logging, and lifecycle orchestration.
- `src/daemon.rs` — implement bounded background synchronization and signal handling.
- `src/model_config.rs` — implement interactive and persisted model configuration.
- `tests/operations.rs` — verify operational command behavior and recovery.
- `tests/daemon.rs` — verify lifecycle, exclusivity, shutdown, and failure behavior.
- `tests/model_config.rs` — verify model configuration behavior without live services.

### Files to modify

- `src/cli.rs` — expose `fetch`, `reset`, `log`, `start`, `stop`, `restart`, hidden `_daemon`, and `lm` contracts.
- `src/main.rs` — dispatch operational commands and the daemon entry point.
- `src/lib.rs` — export operations, daemon, and model-configuration modules.
- `src/config.rs` — persist settled operational and model settings safely.
- `src/domain.rs` — add remaining provider and operational types.
- `src/providers/mod.rs` — register each remaining approved provider.
- `src/store.rs` — support reset and operational ownership boundaries.
- `src/store/freshness.rs` — represent remaining provider freshness policies.
- `src/store/snapshots.rs` — preserve complete enrichment snapshots.
- `src/store/sync_runs.rs` — record foreground and background runs.
- `src/store/schema.sql` — add only state required by remaining providers and operations.
- `src/sync.rs` — coordinate remaining providers in foreground and background modes.
- `src/transport.rs` — support provider-specific transport requirements without weakening shared policy.
- `tests/b9_cli.rs` — verify operational help, visibility, dispatch, and errors.
- `tests/config.rs` — verify persisted operational settings.
- `tests/store_state.rs` — verify reset, freshness, snapshot, and run state.
- `tests/sync.rs` — verify complete provider orchestration.
- `tests/transport.rs` — verify remaining transport policies.
- `docs/skout-cli-operations.md` — record resolved operational parity.
- `docs/skout-providers-storage.md` — record resolved provider and storage behavior.
- `docs/skout-analysis-display-advisory.md` — record resolved model-configuration behavior.
- `arch.md` — describe lifecycle, concurrency, provider, and recovery plumbing.

### Schema changes

- Add monotonic migrations only for the remaining provider and operational state selected during Audit.

## Out Of Scope

- Exclude providers or scrapers that Audit proves have no observable or required replacement behavior.
- Defer final cross-command visual parity and documentation closure to the [parity closure pointer](../plan.md).
- Exclude release publication and external service provisioning.

## Migration findings

- Inventory every remaining provider, credential, state-file, process, and model-setting migration during Audit.

## Acceptance Tests

**AT1** [Automated] [Pre-release gate] — Verify every settled operational command has bounded, recoverable, and platform-safe behavior.

**AT2** [Automated] [Pre-release gate] — Verify daemon exclusivity, clean shutdown, foreground equivalence, logging, and failure recovery.

**AT3** [Automated] [Pre-release gate] — Verify each retained provider uses typed acquisition, bounded transport, atomic persistence, freshness gates, and durable fallback.

**AT4** [Automated] [Pre-release gate] — Verify model configuration persists safely and never exposes credentials.

**AT5** [Manual] [Post-release verification] — Exercise retained live providers and background operation in the supported host environment.

## Status

`PENDING` — awaiting user authorization to begin Audit.
