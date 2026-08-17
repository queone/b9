# AC29 Skout Parity Remaining Coverage

Skeleton AC, captured per explicit Director decision. Not yet audited or refined — see Status.

## Summary

Extends AC28's live skout-vs-b9 comparison to the commands and flag variants that pass deliberately excluded: `t`, `tt`, and `m` (all three can trigger a live Yahoo/MLB refresh that touches the OS Keychain and prompted a real macOS authorization dialog during AC28's pass — the Director asked to handle those separately, supervised, rather than have an agent run them unattended), plus every flag variant not exercised in AC28's default-invocation-only pass: `m -w <N>`, `m -D <date>`, `m -a`, `h -s <col>`, `h -p <pos>`, `h -w`, `p -s <col>`, `p -p <pos>`, `p -w`, `rt -w`/`--weekly[=N|DATE]`. Code and/or doc change, scope settles during this AC's own Audit once the comparison actually runs.

## In Scope

Skeleton — full scope settles during Audit, run with the Director present/supervising for any step that may trigger Keychain access.

- Run the same PTY-color-preserving comparison approach AC28 used (see that AC's Summary and this session's record for the harness) against `t`, `tt`, `m` (default) and every flag variant listed above, for both b9 and skout, against the same real league.
- Classify every observed difference using AC28's same taxonomy: confirmed bug (verify against live DB where applicable, per AC28's Part A/B pattern), confirmed feature gap, confirmed intentional/already-decided, or data-staleness-only (not a defect).
- Produce concrete Parts (mirroring AC28's structure) for whatever is found, or close with no findings if genuine parity holds.

## Out Of Scope

- Anything already covered and dispositioned by AC28.
- Re-litigating AC28's Part G (coloring) or Part F (Statcast) sizing decisions — those stay in AC28.
- Any Keychain/credential-prompt automation or workaround — if `t`/`tt`/`m` require Director-supervised manual runs to avoid unattended Keychain prompts, that stays a manual, supervised step for this AC too, not something to script around.

## Migration findings

- Blocked on the Director's availability to supervise the auth-touching runs; not blocked on any other AC.
- Confirm whether `t`/`tt`/`m`'s Keychain access can be granted a standing "always allow" exception in this environment before the comparison run, to avoid repeated prompts — Director decision, not an agent one.

## Acceptance Tests

Skeleton — defined during this AC's own Audit, after the comparison pass runs.

**AT1** [Manual] [Pre-release gate] — Placeholder: full acceptance-test set defined during Audit, once the live comparison for `t`/`tt`/`m` and the listed flag variants has actually run.

## Status

`DEFERRED` — skeleton only, captured per explicit Director decision (2026-08-16). Blocked on Director-supervised availability for the auth-touching comparison run; Audit begins after that, on explicit Director request.
