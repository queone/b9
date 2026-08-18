# b9

## MLB utilities

Run `b9 t` to show skout-style 40-man roster tables or `b9 t [team]` to select a club by abbreviation, city, or nickname. The roster rows include season statistics and locally cached Yahoo rank, eligibility, and owner context when available. Run `b9 tt` for division-grouped MLB standings with inline team totals. Run `b9 sp` for three compact host-local slate days with paired probable pitchers, matchup odds bars, and optional local roster context.

Use `-f` or `--force` with any MLB utility command to bypass its freshness gate. These commands use unauthenticated MLB, ESPN, and OddsShark data and never require or refresh Yahoo authorization. When b9 has no Yahoo-linked local state, the first MLB utility command imports compatible identity, ownership, statistics, freshness, and empty selections once from the read-only legacy database at `~/.config/skout/skout.db`. Complete cached snapshots remain available with a stale warning when a provider refresh fails. OddsShark is an unofficial future-game source and may degrade without failing the MLB slate.

`b9` is the Rust successor to the Go `skout` binary. Retained commands have fixture-backed deterministic behavior and explicit Rust improvements around state isolation, foreground synchronization, transport bounds, and snapshot recovery. Replacement readiness remains conditional on the live Yahoo and terminal gates recorded in the parity documents.

## Why

Develop the successor to `skout` in Rust over multiple releases while keeping parity and replacement-readiness claims tied to verified behavior.

## Current CLI

Run `b9` or `b9 --help` to see the implemented command surface in b9's compact Usage format. Supported 256-color terminals receive the b9 title and section hierarchy; redirected output, `NO_COLOR`, `TERM=dumb`, and terminals without advertised 256-color support receive the same layout as plain text. Run `b9 --version` to print the independently versioned binary contract.

Use `b9 i [TERM]` to browse the full glossary or look up one term. The glossary is compiled into the binary and works offline without the repository checkout. Ambiguous non-interactive lookups report matching keys and ask for an exact key.

Use `b9 sync -l <league-id-or-key> -T <team-key-or-name>` for deterministic Yahoo setup and foreground refresh. In an interactive terminal, `sync` prompts for a missing league or primary team and saves both selections. Use `st` for local status; `m`, `r`, `rt`, `h`, and `p` for fantasy decisions; `t`, `tt`, and `sp` for MLB context; and `reset` for explicit local-state removal. Yahoo fantasy reads use unauthenticated public endpoints and never consult the Keychain. Background synchronization and Yahoo authentication commands are retired. Tables preserve skout's column order, fixed-width hierarchy, semantic 256-color roles, and plain redirected fallback wherever the available Rust data model supports the corresponding cells.

`b9 sync` refreshes Yahoo league settings, standings, complete rosters, and free agents alongside MLB and odds data. Yahoo resources are staged and validated before one atomic fantasy-snapshot replacement. Each completed foreground step appears immediately, followed by one aggregate result. A provider failure retains that provider's last complete data and does not block unrelated sources; retry later after checking network access.

Yahoo's public fantasy endpoints are unofficial and may change, deny access, or return incompatible payloads without notice. b9 sends no Yahoo cookies or authorization headers, performs no league enumeration or access-denial evasion, and retains the last complete snapshot when refresh fails.

## Governance

This repo is governed by an explicit session-entry contract for AI coding agents — see [`govna/operator-contract-rationale.md`](govna/operator-contract-rationale.md) for the design reasoning and [`AGENTS.md`](AGENTS.md) for the operational rules.

## AC Workflow

Here, "AC" names both the acceptance-criteria document—the change blueprint—and the governed change it tracks from Draft through Package.

Use the standalone action vocabulary `Draft → Audit → Refine → Implement → Ratify → Package` for an active AC. Draft creates the AC; Audit, Refine, Implement, and Ratify are the four AC phases; Package is post-Ratify release preparation. Accept lowercase forms for the phase actions and `package`, `pack`, or `prep` for Package. Ordinary coding phrases such as `build`, `prepare the build`, and `package the binary` do not advance the workflow.
