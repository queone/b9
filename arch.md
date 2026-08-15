# b9 Architecture

## Purpose

Port `skout` from Go to Rust over multiple releases.

## System Summary

The repository contains a metadata-driven Rust CLI, a reusable domain library, an embedded read-only glossary, and the build and governance infrastructure for subsequent porting work. Runtime orchestration, storage, and external integrations remain to be established by later acceptance-criteria cycles.

## Current Platform

- Rust

## Major Components

- `src/main.rs`: Rust executable entry point and independently versioned utility declaration
- `src/cli.rs`: root command metadata, parsing, dispatch, streams, and exit behavior
- `src/glossary.rs`: embedded glossary parsing, lookup, suggestions, and plain-text rendering
- `src/terminal.rs`: deterministic terminal-color policy and CLI presentation styles
- `src/lib.rs`: reusable Rust library root
- `src/domain.rs`: provider-neutral fantasy-baseball domain records and invariants
- `build.sh`: canonical build, validation, and release workflow
- `tests/build_cli.sh`: regression coverage for the build CLI
- `tests/domain.rs`: public domain-contract regression coverage
- `govna/`: governance and delivery-cycle documentation

## Core Files

- `AGENTS.md`: base governance contract
- `plan.md`: prioritized roadmap and approved direction
- `build.sh`: Bash 3.2-compatible build, validation, and release orchestrator for the Rust toolchain
- `govna/development-cycle.md`: workflow from roadmap through release
- `govna/ac-template.md`: acceptance-criteria template for new work
- `govna/build-release.md`: build, test, and release rules

## Data And Control Flow

Provider and persistence adapters will construct owned domain records through later slices. Analysis, view-model, display, advisory, and CLI layers will consume those records without placing provider, storage, serialization, or terminal mechanics in the domain module.

The executable passes its utility version into one CLI metadata model, whose shared descriptors drive parsing and Skout-style root help. A terminal presentation boundary enables the contracted 256-color roles only for supported terminal stdout and otherwise renders the byte-equivalent plain layout. Glossary commands dispatch into the library, where `docs/glossary.md` is embedded at compile time and parsed without filesystem or network access. Plain-text glossary rendering remains separate from lookup behavior so a later terminal adapter can add Skout-compatible selection and presentation.

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

## Conventions

- update this document when architecture or major workflow changes materially
- keep implementation detail in code and stable architecture here
