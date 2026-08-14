# b9 Architecture

## Purpose

Port `skout` from Go to Rust over multiple releases.

## System Summary

The repository currently contains an initial Rust executable and the build and governance infrastructure for subsequent porting work. The component boundaries, runtime flow, storage model, and external integrations remain to be established by later acceptance-criteria cycles as behavior is ported.

## Current Platform

- Rust

## Major Components

- `src/main.rs`: initial Rust executable entry point
- `build.sh`: canonical build, validation, and release workflow
- `tests/build_cli.sh`: regression coverage for the build CLI
- `govna/`: governance and delivery-cycle documentation

## Core Files

- `AGENTS.md`: base governance contract
- `plan.md`: prioritized roadmap and approved direction
- `build.sh`: Bash 3.2-compatible build, validation, and release orchestrator for the Rust toolchain
- `govna/development-cycle.md`: workflow from roadmap through release
- `govna/ac-template.md`: acceptance-criteria template for new work
- `govna/build-release.md`: build, test, and release rules

## Data And Control Flow

No stable application control flow is documented yet. Later porting ACs will record each verified path as it is introduced.

## AC Lifecycle Control Flow

The governed change path is `Draft → Audit → Refine → Implement → Ratify → Package`. Draft creates the AC; Audit, Refine, Implement, and Ratify are the four AC phases; Package is post-Ratify release preparation and is not a fifth phase.

## Architecture Notes

- record stable system decisions here
- prefer durable structure and interfaces over transient implementation detail

## Conventions

- update this document when architecture or major workflow changes materially
- keep implementation detail in code and stable architecture here
