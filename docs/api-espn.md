# ESPN MLB Scoreboard And Odds

## Status

ESPN's MLB scoreboard and core-odds endpoints are public, unauthenticated, unofficial, and unsupported. Treat captured fixtures as the pre-release contract and live responses as post-release observations.

## Endpoints

- Use `https://site.api.espn.com/apis/site/v2/sports/baseball/mlb/scoreboard?dates=YYYYMMDD` for slate discovery.
- Use `https://sports.core.api.espn.com/v2/sports/baseball/leagues/mlb/events/{event-id}/competitions/{competition-id}/odds` for one game's odds.
- Request the caller-supplied UTC calendar day and the following UTC calendar day.
- Deduplicate scoreboard events by their first nonempty event identifier.
- Preserve first-seen event order.

## Scoreboard Shape

Read only these observed fields:

- Read `events[].id` as the event identifier.
- Read `events[].competitions[0].id` as the competition identifier.
- Read `competitors[].homeAway` to distinguish home and away.
- Read `competitors[].team.displayName` as the team name used by later game mapping.
- Skip incomplete events instead of inventing identifiers or team names.

## Odds Shape

Read only these observed fields:

- Read `items[0]` as ESPN's established top provider.
- Read `items[0].provider.name` as the sportsbook label.
- Read `items[0].homeTeamOdds.moneyLine` as signed American home odds.
- Read `items[0].awayTeamOdds.moneyLine` as signed American away odds.
- Mark a game quoted when either moneyline is nonzero.
- Retain a valid scoreboard game as unquoted when `items` is empty or both prices are zero.

Ignore spread, total, and pitcher-prop fields. ESPN supplies only the moneyline context retained by the current parity target.

## Transport

Route every request through b9's injected `HttpClient`. Use GET, a ten-second total timeout, a four-MiB response limit, no retries, and no disk cache. Accept HTTPS endpoint configuration in production and loopback HTTP only in tests.

Abort the acquisition when either scoreboard request fails transport, HTTP status, size, or JSON decoding. Retain all valid scoreboard games when an individual odds request fails and return each degraded event as a bounded structured issue. Keep response bodies and query values out of user-facing diagnostics.

## Persistence

Keep acquisition separate from persistence. The typed odds store replaces only `moneyline` rows for explicitly affected positive MLB game identifiers in one immediate transaction. It writes either two rows for one quoted game or no rows for one unquoted game, preserves unrelated markets and games, and captures one injected store time per replacement.

Keep these later integration policies outside the adapter and typed store:

- Map ESPN team-name pairs to MLB game identifiers.
- Apply the 30-minute odds freshness decision.
- Select stale persisted lines after refresh failure.
- Render warnings and moneyline-derived probability context.
- Update command snapshots.

## Fixture Provenance

The scrubbed fixtures under `tests/fixtures/espn/` reproduce the observed field subsets documented above. Test metadata records the two endpoint families and the 2026-08-15 scrub date. Fixtures contain no credentials or personal data and require no external network access.

Live payload changes do not silently expand this contract. Capture and review a new scrubbed fixture before changing typed decoding or normalization behavior.
