# AC26 Yahoo Public Feed As Primary Fetcher

Skeleton AC, captured per explicit Director decision. Not yet audited or refined — see Status.

## Summary

Marry the two Yahoo acquisition paths AC23 establishes as permanent, independent commands: fold the `pub-api.fantasysports.yahoo.com` public feed (via `pp`) in as the *primary default* acquisition path for Yahoo fantasy data that doesn't require authentication — league, teams, rosters, matchups, and player stats — while `b9 login`/`b9 sync` (OAuth) remains for data confirmed to genuinely require an authenticated session (free agents/waiver pool, transactions, roster moves, and anything else AC23 or this AC's own Audit confirms needs login). `pub-api` is already permanent, load-bearing infrastructure as of AC23, not something this AC needs to argue for — it is an unofficial, undocumented Yahoo endpoint with no support contract and may change or disappear without notice, a risk the Director has explicitly accepted, not contingent on further discussion. What this AC decides is the *mechanics* of the merge (command surface, precedence, fixture/resilience strategy), deferred until AC23 has run in production to inform that design. Code and doc change.

## In Scope

Skeleton — full file/module list settles during this AC's own Audit/Refine, after AC23 has shipped and run in production.

- Fold `src/providers/yahoo_public.rs`'s client (from AC23) into the primary fetch path used by `sync`/`b9 st`, in addition to its standalone use via `pp`.
- Settle the command surface during Refine: does `pp`/`pull-public` stay as-is alongside a `sync` that now calls the same client internally, get merged into `sync` outright, or something else?
- Enumerate precisely which data classes still require OAuth (free agents, transactions, roster moves at minimum — confirmed absent from the public feed during AC23) and leave those on the existing `login`/`sync` path unchanged.
- Update `docs/api-yahoo.md`, `docs/skout-cli-operations.md`, `arch.md` to describe the public feed as the primary default for the data it covers.
- AC23 already treats `pp` and `sync` as independent, permanent, coexisting commands with no precedence between them; this AC is what establishes precedence (public feed first for unauthenticated-coverable data).

## Out Of Scope

- Anything the public feed doesn't expose (confirmed during AC23: no free agents, no transactions, no REST-mirror resources at the equivalent paths) — stays on OAuth.
- Removing `b9 login`/`b9 sync` entirely — OAuth remains required for auth-scoped data and as the mechanism that resolves Yahoo-redacted fields (e.g. manager nicknames).
- Deciding this AC's full scope before AC23 has shipped and run for real — see Migration findings.

## Migration findings

- Blocked on AC23 shipping and running in production first; real usage is the evidence this decision is built on, and AC23's Implement pass may surface endpoint behavior (rate limits, response drift, additional fields) that changes this AC's design.
- Determine whether the "official vs public" provenance distinction AC23 builds (`SyncOrigin::PublicPull`, latest-complete-run lookup) collapses once the public feed is primary, or whether it's still needed to track staleness/fallback.
- Determine fixture/test strategy appropriate once the public feed carries default-path load rather than only `pp`'s standalone use — AC23's fixtures were sized for that narrower use.

## Acceptance Tests

Skeleton — defined during this AC's own Audit.

**AT1** [Manual] [Pre-release gate] — Placeholder: full acceptance-test set defined during Audit, once AC23's production behavior is known.

## Status

`DEFERRED` — skeleton only, captured per explicit Director decision (2026-08-16). Blocked on AC23 shipping and running in production; Audit begins after that, on explicit Director request.
