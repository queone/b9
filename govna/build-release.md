# Build and Release

## Build and Test Rules

- use one canonical local build command and keep this document current
- run formatting, static checks, tests, and packaging through that command or documented sequence
- do not trigger release work during routine implementation

This repo uses a self-contained `build.sh` for all build, release-prep, and release work. No external govna tools are required; everything runs directly from `build.sh`.

### Build Presentation

- Reuse the canonical build color policy and palette across supported CODE stacks.
- Color phase headings, command previews, status values, failures, prep output, and release output by semantic role.
- Emit plain output when stdout is not a terminal.
- Emit plain output when `NO_COLOR` is set.
- Emit plain output when `TERM=dumb`.
- Require a 256-color-capable terminal before emitting ANSI sequences.
- Preserve plain-text content and output streams when color is disabled.
- Keep self-contained build scripts compatible with Bash 3.2.

## Minimum Validation

- formatting passes
- static checks pass
- automated tests pass
- changed docs match actual behavior

## Rust Compilation Reuse

- Keep separate Clippy, test, and release-build validation unless a measured change preserves their coverage.
- Reopen optimization work when build duration becomes materially costly.
- Reopen optimization work when stable Cargo or Clippy behavior offers measurable artifact reuse.
- Reopen compiler-cache evaluation only with Director authorization.
- Record the toolchain version, exact commands, isolated target-directory conditions, repeated timings, and unchanged validation coverage in any renewed investigation.
- Compare release-prep changes in disposable repository copies with isolated Cargo and installation roots.
- Require lower median warm duration in every representative Rust consumer.
- Require at least 25 percent lower aggregate warm duration before adopting a release-prep optimization.

Note: Cargo may compile overlapping dependency graphs separately because Clippy, tests, and release builds use different analysis, code-generation, and profile artifacts. Direct measurements did not demonstrate a reliable wall-clock improvement from command reordering alone.

### Rust Release Prep

- Run `GOVNA_PREP_VALIDATION_TOKEN='<token>' ./build.sh prep vX.Y.Z "message"` during Package.
- Use the token printed by the successful final full build reviewed during Ratify.
- Run `./build.sh refresh-validation-token -b <scratch-baseline> -t '<token>'` after exact baseline-only audit-adoption completion.
- Use the refreshed token as Package evidence.
- Omit the token to require a fallback pre-change full build.
- Require prep to recompute HEAD and the Git-visible-state fingerprint before writes.
- Run one post-change full build after prep writes.
- Reuse one prep-owned isolated Cargo target across fallback validation, Cargo.lock refresh, and post-change validation.
- Keep prep responsible for shared-target cleanup.
- Use `-v, --verbose` to stream complete prep phase output.
- Replay captured phase output on failure in default mode.
- Reject `--no-build, -B` outside Rust dry-run prep.

## Canonical Build Commands

```bash
./build.sh
```

To scope the run to selected commands:

```bash
./build.sh <target> [<target> ...]
```

Use space-separated target names. Supported CODE stacks may retain package-wide shared-code validation while limiting target-specific checks, tests, artifacts, and installation to the selected targets.

Run `./build.sh` without targets for repository-wide validation. Follow the applicable stack guidance above for release-prep evidence, pre-change validation, and build-state reuse. Release-prep validation uses the package-wide form.

## Independent Utility Versions

- Treat the repository/package version as the version input and release metadata governed by the existing release mechanism.
- Require one normalized record for each installable utility with its canonical target name, declaration location, declared version, and `--version` invocation.
- Accept only `^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$` as a strict stable SemVer declaration.
- Require `--version` to exit 0, print exactly `<utility-id> <MAJOR.MINOR.PATCH>` or `<utility-id> v<MAJOR.MINOR.PATCH>` plus its newline to stdout, and write nothing to stderr.
- Validate every declaration before compilation.
- Validate each compiled utility result before installing that utility.
- Validate every compiled utility result before release-metadata writes.
- Reject missing, empty, malformed, duplicate, orphaned, and mis-mapped records with a non-zero error that names the utility and recovery action.
- Preserve all independent utility declarations and outputs during repository release prep.

## Pre-Release Checklist (`Package`, `package`, `pack`, or `prep`)

Do not start this checklist unless the director explicitly requests standalone
`Package`, `package`, `pack`, or `prep` in the active Ratified AC context.
Do not treat `./build.sh prep ...` or ordinary build-preparation language as a
workflow request.

The operator flow is two steps:

1. **Run the stack-defined `./build.sh prep vX.Y.Z "message"` invocation.** Stages version bumps, inserts the CHANGELOG row, deletes completed AC files, sweeps matching AC-pointer IE lines from `plan.md`, runs stack-defined validation, and prints the canonical release command. The agent determines the version (semver classification from the AC's scope) and drafts the release message (≤ 80 characters) before invoking prep. Flags: `--dry-run`/`-n` prints intended writes without touching the working tree; `--no-build`/`-B` follows the applicable stack policy.

   Before running prep, satisfy this repository's declared version-target contract and keep repository/package and independently versioned utility declarations aligned as required by its Project Practices.
2. **Run the printed release command (`./build.sh vX.Y.Z "message"`).** Shows `git status --short`, lists every git step it will execute, and prompts for interactive confirmation. On approval it orchestrates `git add → commit → tag → push tag → push branch`.

Present only the release command after prep; do not add trailing commentary about wrapper routing or prompts. The director already knows.

### Appendix: what prep does

`./build.sh prep` runs nine phases internally so the operator flow above stays short. Each phase has a clear failure mode:

1. **Validate inputs.** Semver pattern (`vX.Y.Z`), message non-empty and ≤ 80 characters.
2. **Validate git state.** Inside a git work tree, target tag does not exist yet, HEAD is not at the latest tag with a clean working tree.
3. **Run pre-change validation.** Follow the applicable stack policy for current build evidence, fallback validation, and failure handling before writes.
4. **Detect and validate version targets.** Follow this repository's Project Practices and stack build implementation. Reject missing, malformed, duplicate, or unsafe targets before any write.
5. **Detect CHANGELOG targets + fail-fast idempotency guard.** Root `CHANGELOG.md`. If it already contains a row for the target version, prep exits with a fatal error before any writes.
6. **Parse AC refs.** `AC[0-9]+` scan on the release message; composites like `AC<m>+AC<n>` yield multiple refs.
7. **Apply writes.** Version bumps (per-file idempotent no-op when the file already has the target value); CHANGELOG row insertion under `| Unreleased | |`; AC file deletions (AC files are deleted whole; there are no separate companion files); AC-pointer IE-line sweep from `plan.md` (lines matching `→ govna/ac<N>-` for each released AC). Skipped when `--dry-run`/`-n`. Idempotent re-runs leave already-swept lines alone.
8. **Run post-change validation.** Follow the applicable stack policy for build-state reuse, output, failure handling, and cleanup.
9. **Print release command.** Labeled block: `release command:` followed by the indented command `./build.sh vX.Y.Z "message"`.

CHANGELOG row shape (enforced by prep's insertion code and by convention):

- File shape: `# Changelog` heading, then a 2-column markdown table (`| Version | Summary |` with a `|---------|---------|` separator); first data row is `| Unreleased | |`, followed by one row per release (e.g., `| <version> | <AC-ref>: <one-line summary> |`).
- Summaries are single-line, ≤ 500 characters; lead with the AC reference if any.
- Versions are unprefixed (`0.29.0`, not `v0.29.0`).
- Do not backfill historical tags or invent alternative shapes (Keep-a-Changelog, sectioned `## vX.Y.Z`, etc.).

## Project Practices
