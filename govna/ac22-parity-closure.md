# AC22 Parity Closure

## Summary

Close the Go-to-Rust port through an exhaustive executable comparison. Correct remaining behavioral and display differences, reconcile architecture and API documentation with the delivered Rust design, remove obsolete internal skout assumptions, and establish evidence that b9 can supplant skout for the supported workflow.

## In Scope

### Files to create

- `tests/parity.rs` — verify cross-command fixtures and intentional-difference contracts.

### Files to modify

- `src/cli.rs` — correct remaining command-contract parity defects.
- `src/main.rs` — correct remaining dispatch and user-facing behavior defects.
- `src/domain.rs` — correct shared representation defects exposed by parity review.
- `src/terminal.rs` — complete settled color, spacing, width, table, and non-terminal behavior.
- `src/glossary.rs` — complete settled glossary selection and rendering behavior.
- `src/matchup.rs` — correct remaining matchup parity defects.
- `src/mlb_commands.rs` — correct remaining MLB command parity defects.
- `src/mlb_display.rs` — correct remaining MLB display parity defects.
- `src/player_commands.rs` — correct remaining player workflow parity defects.
- `src/player_display.rs` — correct remaining player display parity defects.
- `src/advisory.rs` — correct remaining advisory parity defects.
- `src/evaluation.rs` — correct remaining evaluation parity defects.
- `src/transaction_commands.rs` — correct remaining transaction parity defects.
- `src/transaction_display.rs` — correct remaining transaction display parity defects.
- `src/operations.rs` — correct remaining operational parity defects.
- `src/daemon.rs` — correct remaining lifecycle parity defects.
- `src/model_config.rs` — correct remaining configuration parity defects.
- `tests/b9_cli.rs` — cover final command and help contracts.
- `tests/terminal.rs` — cover final rendering contracts.
- `tests/glossary.rs` — cover final glossary behavior.
- `docs/skout-parity.md` — record the final evidence and intentional differences.
- `docs/skout-cli-operations.md` — reconcile every command and operational outcome.
- `docs/skout-providers-storage.md` — reconcile every retained provider and storage path.
- `docs/skout-analysis-display-advisory.md` — reconcile every analytical, display, and advisory outcome.
- `docs/api-espn.md` — align the API contract with final Rust usage.
- `docs/api-mlbam.md` — align the API contract with final Rust usage.
- `docs/api-oddsshark.md` — align the API contract with final Rust usage.
- `docs/api-yahoo.md` — align the API contract with final Rust usage.
- `docs/glossary.md` — align glossary documentation with final behavior.
- `README.md` — describe the complete supported b9 workflow.
- `arch.md` — replace transitional port descriptions with the delivered Rust architecture and plumbing improvements.
- `plan.md` — remove fulfilled pointers and leave only genuine follow-on work.

### Schema changes

- Prohibit new feature schema; permit only corrective migration work required by a parity defect found during Audit.

## Out Of Scope

- Exclude new features without observable skout behavior or an explicitly approved Rust improvement.
- Exclude support for intentionally retired providers and commands.
- Exclude release publication.

## Migration findings

- Identify obsolete skout-named configuration, persisted state, documentation, and compatibility surfaces during Audit.

## Acceptance Tests

**AT1** [Automated] [Pre-release gate] — Verify every retained command, flag, alias, error path, and non-interactive output contract against the final parity matrix.

**AT2** [Automated] [Pre-release gate] — Verify every intentional Rust improvement is documented and regression-tested.

**AT3** [Automated] [Pre-release gate] — Verify internal product references use b9 except where skout names a historical source or explicit compatibility surface.

**AT4** [Automated] [Pre-release gate] — Verify architecture, API, glossary, parity, and user documentation describe the delivered implementation without stale transitional claims.

**AT5** [Manual] [Pre-release gate] — Review representative color, column, width, interactive, and terminal outputs against skout and approve each intentional difference.

**AT6** [Manual] [Post-release verification] — Run the supported live workflow end to end after Yahoo Fantasy API access is available.

## Status

`PENDING` — awaiting user authorization to begin Audit.
