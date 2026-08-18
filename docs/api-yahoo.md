# Yahoo Public Fantasy Requests

## Status

b9 acquires read-only Yahoo fantasy data without OAuth, cookies, browser state, or account credentials. Yahoo does not document these public endpoints as a supported API, so every request is bounded and every failed or incompatible refresh retains the last complete snapshot.

## Hosts and ownership

- Use `https://pub-api.fantasysports.yahoo.com/fantasy/v3` only for the redzone league feed.
- Use `https://pub-api-ro.fantasysports.yahoo.com/fantasy/v2` for league-scoped settings, standings, rosters, players, scoreboards, weekly team statistics, and public ranks.
- Treat `ro` as an observed read-only frontend convention rather than a Yahoo compatibility promise.
- Keep account-scoped discovery unavailable; require the operator to provide a league id or key and select a primary team.
- Verify any proposed path independently before adding it to the production allowlist.

## Production request paths

- Fetch `/redzone/mlb?league_id={league_id}&format=json` for current league, team, roster, matchup, and player data.
- Fetch `/league/{league_key}/settings?format=json` for categories, roster positions, season, and current week.
- Fetch `/league/{league_key}/standings?format=json` for team records, rank, waiver priority, budget, and move counts.
- Fetch `/league/{league_key}/teams/roster/players;out=ranks,percent_owned?format=json` for complete league rosters.
- Fetch `/league/{league_key}/players;status=A;start={offset};count=25;out=ranks,percent_owned?format=json` until the first empty page for active free agents.
- Fetch `/league/{league_key}/scoreboard[;week={week}]?format=json` for current or historical matchup scoreboards.
- Fetch `/team/{team_key}/roster;week={week}/players/stats;type=week;week={week}?format=json` for weekly roster statistics.
- Fetch `/league/mlb.l.public/players;player_ids={ids};out=ranks;ranks=season?format=json_f` in bounded batches for public season ranks.

A full Yahoo league key such as `469.l.170874` is preserved. A bare id or legacy `public.170874` value is normalized to `mlb.l.170874`, the public alias accepted by the league-scoped host. Historical `public_pull` storage origins remain readable but are never newly written.

## Transport and security

- Send only HTTPS GET requests through the shared validating transport.
- Send an `Accept` header and never send `Authorization`, `Cookie`, OAuth parameters, refresh tokens, or browser headers.
- Apply a ten-second request timeout and an eight-MiB response limit.
- Bound free-agent pagination to 20 pages of 25 players.
- Reject invalid league and team keys before request construction.
- Reject non-success responses without retaining response bodies in diagnostics.
- Avoid retrying access denial, evading blocking, or enumerating league ids.
- Keep advisory-provider credentials isolated in their existing environment and keyring boundary.

## Setup and synchronization

Run `b9 sync -l <league-id-or-key> -T <team-key-or-name>` for deterministic non-interactive setup. Interactive sync prompts for a missing league id and displays the fetched team list when the primary team is unresolved.

Primary-team matching uses this precedence:

1. exact team key;
2. case-insensitive exact team name;
3. unique case-insensitive team-name substring.

Ambiguous, missing, or stale selections fail without guessing. Synchronization fetches settings, standings, complete rosters, and all free-agent pages before validating the selected team and atomically replacing the prior complete fantasy snapshot. A failed or incomplete acquisition leaves the prior snapshot intact and records a visible provider failure.

## Command behavior

- Use public Yahoo data for foreground, startup, and scheduled `sync`.
- Use public Yahoo scoreboards and weekly team statistics for `m` and `rt --weekly`, including explicit weeks and ISO dates.
- Require MLBAM identity reconciliation before daily matchup overlays; report unresolved players instead of silently applying an empty overlay.
- Populate waiver candidates from the complete public free-agent snapshot.
- Keep `st` local-only.
- Remove `login`, `pp`, `pull-public`, authenticated `fetch`, and Yahoo `-o/--oauth` flags.
- Retain `logout` for one released cleanup window only; it can delete the exact retired `b9` / `yahoo-oauth-token` keychain entry but cannot read, refresh, or use it.

## Response contracts

Yahoo collections may use numeric object keys, arrays, or singleton shapes. Parsers normalize those variants into provider-neutral league, team, player, slot, matchup, category, and weekly-stat records. Empty roster placeholders are skipped; malformed identities, incomplete required collections, invalid roster ownership, and an empty completed free-agent acquisition reject the refresh.

The redzone feed supplies current-week category building blocks. Counting categories sum active roster players; rate categories are recomputed from their underlying totals. Innings use baseball thirds rather than decimal tenths. Bench and injured slots remain visible in roster views but do not contribute to active matchup totals.

## Availability risk

These endpoints are publicly reachable but unofficial and potentially unstable. A future denial, path change, payload change, rate limit, or Yahoo policy change can prevent refresh. b9 mitigates that risk with exact allowlisted paths, bounded requests, fixture-backed parsers, atomic replacement, durable weekly snapshots, and stale fallback. It does not treat authentication as a recovery path.

## Fixture provenance

Scrubbed synthetic fixtures under `tests/fixtures/yahoo/` and `tests/fixtures/yahoo-public/` cover settings, standings, rosters, free agents, scoreboards, weekly statistics, ranks, redzone variants, empty collections, and malformed payloads. They contain no credentials or retained sensitive live response bodies.
