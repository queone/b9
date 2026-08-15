# b9

`b9` is a Rust port of `skout`, which is written in Go. The port is at an early stage and does not yet claim feature parity or readiness to replace `skout`.

## Why

Develop the successor to `skout` in Rust over multiple releases while keeping parity and replacement-readiness claims tied to verified behavior.

## Current CLI

Run `b9` or `b9 --help` to see the implemented command surface in Skout's compact Usage format. Supported 256-color terminals receive the Skout-style title and section hierarchy; redirected output, `NO_COLOR`, `TERM=dumb`, and terminals without advertised 256-color support receive the same layout as plain text. Run `b9 --version` to print the independently versioned binary contract.

Use `b9 whatis [TERM]` to browse the full glossary or look up one term. The visible `b9 i [TERM]` alias preserves Skout compatibility. The glossary is compiled into the binary and works offline without the repository checkout.

This slice intentionally omits Skout's interactive ambiguity selector and colored output. Ambiguous terms report matching keys and ask for an exact key.

## Governance

This repo is governed by an explicit session-entry contract for AI coding agents — see [`govna/operator-contract-rationale.md`](govna/operator-contract-rationale.md) for the design reasoning and [`AGENTS.md`](AGENTS.md) for the operational rules.

## AC Workflow

Here, "AC" names both the acceptance-criteria document—the change blueprint—and the governed change it tracks from Draft through Package.

Use the standalone action vocabulary `Draft → Audit → Refine → Implement → Ratify → Package` for an active AC. Draft creates the AC; Audit, Refine, Implement, and Ratify are the four AC phases; Package is post-Ratify release preparation. Accept lowercase forms for the phase actions and `package`, `pack`, or `prep` for Package. Ordinary coding phrases such as `build`, `prepare the build`, and `package the binary` do not advance the workflow.
