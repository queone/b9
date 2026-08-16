# AC23 Yahoo Public Snapshot Import

## Summary

Add a temporary local-file import path for Yahoo league data that a user has manually captured from public Yahoo pages. The feature must update b9's normalized fantasy state without b9 requesting, authenticating to, crawling, or otherwise automating access to Yahoo. The importer must preserve source provenance, import time, completeness, and stale-state behavior so official Yahoo API synchronization can replace it cleanly when access is granted.

## In Scope

### Files to create

- `src/yahoo_snapshot_import.rs` — parse, validate, normalize, and atomically import settled local snapshot formats.
- `tests/yahoo_snapshot_import.rs` — verify valid, malformed, incomplete, and conflicting snapshot behavior.
- `tests/fixtures/yahoo-public/` — contain sanitized, manually captured snapshot fixtures for every settled input format.

### Files to modify

- `src/cli.rs` — expose the settled local snapshot import command and its short and long flags.
- `src/main.rs` — dispatch the import command without Yahoo network access.
- `src/lib.rs` — export the snapshot import module.
- `src/config.rs` — store only user-approved local import defaults if Audit establishes a need.
- `src/domain.rs` — represent snapshot provenance, completeness, source data, and validation outcomes.
- `src/store/fantasy.rs` — write imported league, team, roster, matchup, standings, and ownership state through existing ownership boundaries.
- `src/store/freshness.rs` — distinguish imported freshness from official-provider freshness.
- `src/store/snapshots.rs` — preserve complete imported snapshots and their replacement semantics.
- `src/store/sync_runs.rs` — record bounded local import runs without representing them as provider synchronization.
- `src/store/schema.sql` — add only durable provenance and completeness state required by the importer.
- `src/terminal.rs` — render import results, validation failures, freshness, and recovery guidance.
- `tests/b9_cli.rs` — verify help, short and long flags, dispatch, no-network behavior, and recovery guidance.
- `tests/config.rs` — verify any settled import configuration behavior.
- `tests/domain.rs` — verify provenance and completeness representations.
- `tests/store_fantasy.rs` — verify atomic replacement, reconciliation, and source precedence.
- `tests/store_state.rs` — verify freshness, snapshots, and run records.
- `docs/api-yahoo.md` — document the official-API boundary and the local snapshot input contract.
- `docs/skout-cli-operations.md` — document the temporary import command and its operational limits.
- `docs/skout-providers-storage.md` — document imported-state provenance, precedence, and replacement behavior.
- `arch.md` — document the temporary local-import path and its separation from Yahoo acquisition.

### Schema changes

- Add a monotonic schema migration only for imported-state provenance, completeness, and replacement metadata established during Audit.

## Out Of Scope

- Exclude all automated Yahoo page retrieval, crawling, browser automation, credential reuse, cookies, scheduled requests, and API workarounds.
- Exclude parsing data that is not manually supplied as a local input file.
- Exclude replacing official Yahoo API synchronization after access is granted.
- Exclude new fantasy analysis, advice, transaction, roster, or player-pool command behavior beyond making imported state available to existing commands.
- Exclude unsupported fields that public Yahoo pages do not expose or that the settled local formats cannot represent safely.

## Migration findings

- Determine whether existing fantasy tables can retain imported-state provenance without migration during Audit.
- Determine the accepted local formats, required completeness classes, and source-precedence rules during Audit.
- Determine how an official Yahoo API snapshot replaces an imported snapshot during Audit.

## Acceptance Tests

**AT1** [Automated] [Pre-release gate] — Verify each settled sanitized local fixture imports league, team, roster, matchup, standings, and ownership data into normalized b9 state.

**AT2** [Automated] [Pre-release gate] — Verify malformed, unsafe, unsupported, and incomplete inputs fail with operation context and leave prior complete state unchanged.

**AT3** [Automated] [Pre-release gate] — Verify imports perform no Yahoo network request, authentication action, browser automation, cookie access, or credential access.

**AT4** [Automated] [Pre-release gate] — Verify imported provenance, import time, completeness, freshness, stale fallback, and official-provider replacement behavior are explicit and deterministic.

**AT5** [Manual] [Pre-release gate] — Import a manually captured snapshot of the public league and confirm b9's existing league-aware command output reflects the captured state.

**AT6** [Manual] [Post-release verification] — Confirm an official Yahoo API synchronization replaces temporary imported state after Yahoo grants b9 API access.

## Status

`DEFERRED` — parked pending either official Yahoo Fantasy API access, explicit Yahoo permission for automated public-page collection, or a renewed Director decision to use the manual local-import bridge.
