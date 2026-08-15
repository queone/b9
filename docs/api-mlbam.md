# MLB StatsAPI Reference

## Status

Treat `https://statsapi.mlb.com/api/v1` as a public, unauthenticated JSON service without a supported stability guarantee. Treat the committed fixtures as the pre-release contract and live responses as post-release observations.

## Implemented Endpoints

| Status | Path | b9 operation |
|---|---|---|
| Implemented | `/seasons/{season}?sportId=1` | Acquire season boundaries |
| Implemented | `/schedule?sportId=1&date={date}&hydrate=linescore,probablePitcher,lineups` | Acquire one hydrated schedule day |
| Implemented | `/game/{game-id}/boxscore` | Acquire batting order, bench, and live player statistics |
| Implemented | `/standings?leagueId=103,104&season={season}` | Acquire AL and NL standings |
| Implemented | `/teams/{team-id}/roster?rosterType=40Man` | Acquire one 40-man roster |
| Implemented | `/people?personIds={comma-separated-batch}` | Acquire people identities in batches of 100 |

Use GET with a 10-second total timeout and an 8 MiB response limit. Construct every request inside the MLB adapter and dispatch it through b9's validating `HttpClient`. Keep response bodies out of status errors and user-facing diagnostics.

## Deferred Endpoints

| Status | Path family | Deferred capability |
|---|---|---|
| Deferred | `/people/{id}/stats` | Single-player hitting, pitching, and quality starts |
| Deferred | `/people/search` | Approximate player-name lookup |
| Deferred | `/sports/1/players` | All-player identity seed |
| Deferred | `/stats` | Bulk season and date-range hitting and pitching |
| Deferred | `/stats?stats=gameLog` | Hitter and pitcher game logs |
| Deferred | `/transactions` | Injury acquisition |

## Reference-Only Endpoints

| Status | Path family | Possible use |
|---|---|---|
| Reference only | `/teams` and `/teams/{id}` | Team metadata discovery |
| Reference only | `/venues` and `/venues/{id}` | Venue metadata |
| Reference only | `/schedule/postseason` | Postseason series |
| Reference only | `/game/{game-id}/feed/live` | Play-by-play and complete live state |
| Reference only | `/game/{game-id}/content` | Editorial and media content |
| Reference only | `/stats/leaders` | League leaderboards |
| Reference only | `/people/{id}` and hydrated variants | Full biography and career statistics |
| Reference only | `/people/changes` | Recently modified people records |
| Reference only | `/schedule` with start/end, team, or season filters | Multi-day and team schedules |
| Reference only | `/transactions` with team, player, or status-change filters | Transaction history beyond injuries |
| Reference only | `/schedule/games` | Alternate schedule route |
| Reference only | `/game/{game-id}/linescore` | Standalone inning state |
| Reference only | `/statTypes` and `/gameTypes` | Provider metadata discovery |

## Response Contracts

Preserve provider ordering for schedules, standings, rosters, batting orders, benches, and lineups. Preserve ratio values and innings pitched as source strings. Keep boxscore players keyed by positive MLB person ID without an iteration-order contract.

Skip independently unusable schedule games, roster members, boxscore players, standings rows, and people records according to the adapter contract. Reject missing required top-level envelopes. Accept explicitly present empty collections except for the season envelope, which must contain a season.

Normalize roster positions and statuses to uppercase, trim jersey numbers, and default missing roster status to `A`. Expand `TWP` members into adjacent hitter and pitcher compatibility records. Treat `P`, `SP`, and `RP` as pitchers.

Deduplicate requested people IDs by first occurrence, partition them into batches of 100, and restore first-request order after decoding. Omit missing upstream people and abort the complete acquisition on any batch failure.

## Schedule Cache

Use namespace `mlb`, logical key `schedule-<YYYY-MM-DD>`, and a 60-second TTL. Treat age 60 seconds as expired. Return `Hit`, `Miss`, `Expired`, or `Corrupt` with the typed schedule result. Refetch missing, expired, corrupt-frame, and corrupt-JSON entries.

Propagate cache read failures. Preserve a successful live schedule when cache persistence fails and return a cache-write issue capped at 256 Unicode scalar values. Keep URLs and credentials out of cache keys.

## Fixture Provenance

The fixtures were derived from the Skout executable contracts, its MLB provider tests, and documented StatsAPI shapes on 2026-08-15. They are minimized compatibility captures rather than claims about current live responses. No credentials or operator data were present; only unrelated response fields were removed.

| Fixture | Exact endpoint or query | Origin |
|---|---|---|
| `season.json` | `/seasons/2026?sportId=1` | Skout season test plus API reference |
| `schedule.json` | `/schedule?sportId=1&date=2026-05-15&hydrate=linescore,probablePitcher,lineups` | Skout schedule models and command fixtures |
| `boxscore.json` | `/game/800001/boxscore` | Skout boxscore models and enrichment behavior |
| `standings.json` | `/standings?leagueId=103,104&season=2026` | Skout standings adapter contract |
| `roster.json` | `/teams/119/roster?rosterType=40Man` | Skout roster tests including two-way compatibility |
| `people.json` | `/people?personIds=699009,699008` | Skout people identity tests and model contract |

## Post-Release Verification

Exercise every implemented endpoint live and compare its shape with the committed fixture. Treat any difference as evidence to review, not permission to weaken fixture-backed validation automatically.
