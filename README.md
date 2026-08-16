# b9

## MLB utilities

Run `b9 t` to show skout-style 40-man roster tables or `b9 t [team]` to select a club by abbreviation, city, or nickname. The roster rows include season statistics and locally cached Yahoo rank, eligibility, and owner context when available. Run `b9 tt` for division-grouped MLB standings with inline team totals. Run `b9 sp` for three compact host-local slate days with paired probable pitchers, matchup odds bars, and optional local roster context.

Use `-f` or `--force` with any MLB utility command to bypass its freshness gate. These commands use unauthenticated MLB, ESPN, and OddsShark data and never require or refresh Yahoo authorization. When b9 has no Yahoo-linked local state, the first MLB utility command imports compatible identity, ownership, statistics, freshness, and empty selections once from the read-only legacy database at `~/.config/skout/skout.db`. Complete cached snapshots remain available with a stale warning when a provider refresh fails. OddsShark is an unofficial future-game source and may degrade without failing the MLB slate.

`b9` is a Rust port of `skout`, which is written in Go. The port is at an early stage and does not yet claim feature parity or readiness to replace `skout`.

## Why

Develop the successor to `skout` in Rust over multiple releases while keeping parity and replacement-readiness claims tied to verified behavior.

## Current CLI

Run `b9` or `b9 --help` to see the implemented command surface in b9's compact Usage format. Supported 256-color terminals receive the b9 title and section hierarchy; redirected output, `NO_COLOR`, `TERM=dumb`, and terminals without advertised 256-color support receive the same layout as plain text. Run `b9 --version` to print the independently versioned binary contract.

Use `b9 i [TERM]` to browse the full glossary or look up one term. The glossary is compiled into the binary and works offline without the repository checkout.

The current glossary intentionally omits an interactive ambiguity selector and colored output. Ambiguous terms report matching keys and ask for an exact key.

## Governance

This repo is governed by an explicit session-entry contract for AI coding agents — see [`govna/operator-contract-rationale.md`](govna/operator-contract-rationale.md) for the design reasoning and [`AGENTS.md`](AGENTS.md) for the operational rules.

## AC Workflow

Here, "AC" names both the acceptance-criteria document—the change blueprint—and the governed change it tracks from Draft through Package.

Use the standalone action vocabulary `Draft → Audit → Refine → Implement → Ratify → Package` for an active AC. Draft creates the AC; Audit, Refine, Implement, and Ratify are the four AC phases; Package is post-Ratify release preparation. Accept lowercase forms for the phase actions and `package`, `pack`, or `prep` for Package. Ordinary coding phrases such as `build`, `prepare the build`, and `package the binary` do not advance the workflow.
