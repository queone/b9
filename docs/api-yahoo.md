# Yahoo Authentication And Raw Requests

## Status

Yahoo Fantasy Sports requires OAuth 2.0 bearer authorization. Treat deterministic transport and credential-store doubles as the pre-release contract. Keep live credentials, live authorization, and live requests outside automated validation.

## Endpoints

- Use `https://api.login.yahoo.com/oauth2/request_auth` for authorization.
- Use `https://api.login.yahoo.com/oauth2/get_token` for authorization-code and refresh exchanges.
- Use `https://localhost:8080/callback` as the registered callback without starting a local callback server.
- Use `https://fantasysports.yahooapis.com/fantasy/v2` as the fantasy API root.
- Read the public client identifier from `YAHOO_CLIENT_ID` at the production environment boundary.

## Roster And Player Pool

- Fetch every roster through the league roster endpoint with ranks and ownership percentages.
- Fetch free agents and waiver-eligible unrostered players through paginated league player requests with `status=A` and explicit offsets.
- Stop free-agent pagination only after an empty page and reject a complete fetch that yields no players.
- Replace the durable fantasy snapshot only after league, roster, and free-agent records are complete.
- Preserve the most recent complete normalized snapshot when a later Yahoo acquisition fails.
- Preserve weekly matchup snapshots by league and week so current, numeric-week, and ISO-date totals can fall back safely.

## Authorization

- Generate independent 32-byte operating-system-random state and PKCE verifier values for every attempt.
- Encode state and verifier with unpadded URL-safe Base64.
- Derive the PKCE challenge with SHA-256.
- Request `response_type=code`, `scope=fspt-r`, `code_challenge_method=S256`, and `access_type=offline`.
- Require the complete callback URL supplied by the browser.
- Match the callback scheme, host, effective port, and path exactly.
- Match exactly one returned state value against the pending authorization.
- Accept exactly one nonblank code after rejecting provider-declared errors.
- Consume the non-cloneable pending authorization during completion.
- Reject bare codes because they cannot prove returned state.

## Tokens And Credentials

- Exchange tokens through the injected `HttpClient` with form encoding, a ten-second timeout, a 64-KiB response limit, and no adapter retry.
- Accept only successful JSON containing a nonblank access token, a case-insensitive bearer token type, and positive non-overflowing `expires_in` seconds.
- Calculate expiry from one injected-clock capture.
- Refresh ten seconds before recorded expiry to retain the predecessor OAuth library's safety margin.
- Preserve the prior refresh token when Yahoo omits a rotated value.
- Coalesce concurrent refresh demand into one exchange and one credential-store update.
- Store serialized tokens only in the operating-system credential store under service `b9` and account `yahoo-oauth-token`.
- Distinguish an absent credential from malformed data and credential-store failure.
- Reject unsupported secure-store platforms without plaintext or repository-local fallback.
- Treat initial credential persistence failure as failed authorization.
- Return usable refreshed data with a typed issue when refresh persistence alone fails.
- Keep logout-style credential deletion idempotent.
- Keep the b9 credential independent from Skout's credential namespace.

## Authenticated Raw Requests

- Accept only absolute-path references beneath the Yahoo fantasy API root.
- Reject authority changes, fragments, backslashes, empty paths, and literal or percent-encoded traversal.
- Preserve caller query parameters while replacing every supplied `format` value with exactly one `format=json`.
- Send one bearer header with a ten-second timeout and an eight-MiB response limit.
- Retry only HTTP 429 for no more than five total attempts.
- Honor numeric `Retry-After` values up to 30 seconds.
- Use waits of one, two, four, and eight seconds when `Retry-After` is absent or invalid.
- Return HTTP 401 and 403 as distinct typed terminal-access failures, each with its own recovery guidance, without response bodies.
- Return successful raw bytes without interpreting Yahoo's fantasy JSON shape.

## Security

- Redact access tokens, refresh tokens, bearer headers, client identifiers, authorization codes, state, PKCE material, and token response bodies from debug, display, and errors.
- Keep token and credential-store types inside the Yahoo adapter.
- Keep Yahoo retries outside the shared no-retry transport.
- Keep live credential and keychain mutation outside pre-release tests.

## Fantasy Data Contract

- Acquire authenticated user leagues and team identity.
- Acquire league settings, scoring categories, roster positions, standings, and complete league rosters for foreground synchronization.
- Acquire weekly scoreboards and both matchup rosters lazily for `b9 m`, from either OAuth or the public redzone feed — see "Matchup data from the public feed" below.
- Traverse Yahoo numeric-key collections and accept observed array-or-object variants.
- Remove emoji presentation runes from fantasy team names before persistence or display while preserving textual Unicode.
- Reject incomplete stable snapshots before normalized replacement.
- Cache weekly command payloads for 60 seconds and retain the last complete snapshot after refresh failure.
- Store league, team, player, free-agent, and roster ownership in the existing schema-version-two tables.
- Keep weekly scoreboards and weekly player statistics in versioned command snapshots.
- Keep `login`, `logout`, `st`, `sync`, and the baseline `m` surface outside the provider adapter.

## Delivered Integration And Gaps

- Fetch and atomically replace complete free-agent snapshots for roster and waiver evaluation.
- Resolve current, explicit-week, and ISO-day matchup views with durable stale fallback.
- Share foreground, startup, and scheduled synchronization through one application service and execution lock.
- Keep daily player-stat enrichment in the MLB adapter because Yahoo supplies the matchup scoreboard and weekly roster baseline.
- Defer Yahoo transaction-history acquisition and reconciliation until a later approved provider contract.
- Keep live OAuth and fantasy-format verification pending while Yahoo application access is unavailable.

## Fixture Provenance

The scrubbed synthetic fixtures under `tests/fixtures/yahoo/` cover token, league, settings, standings, roster, matchup, weekly-stat, singleton, empty, and malformed shapes. They contain no credentials or personal data.

## Local status and authorization boundary

`b9 st` does not read the Yahoo credential, refresh OAuth, or make a Yahoo request. It reports local synchronization state and directs the operator to explicit `b9 login` or `b9 sync` when live access is required. On those explicit paths, HTTP 401 is classified as an expired session and HTTP 403 as external Yahoo authorization denial, each with distinct recovery guidance; secure-store denial, missing, and malformed outcomes are classified separately. Cached data is retained when a refresh fails.

## Public redzone feed (unauthenticated)

`b9 pp` (long alias `pull-public`) fetches league, team, roster, matchup, and player data from Yahoo's public redzone feed without OAuth, the credential store, or any `b9 login` state. It is a permanent, independent command — not a temporary bridge — that coexists with `b9 login`/`b9 sync`.

### Endpoint

```
GET https://pub-api.fantasysports.yahoo.com/fantasy/v3/redzone/mlb?league_id={league_id}&format=json
```

- `league_id` is the bare numeric Yahoo league id (e.g. `170874`) — not the full `{game_key}.l.{league_id}` key `sync` uses elsewhere in this document. b9 converts between the two; see "League key vs. league id" below.
- No cookies, no auth header, no query parameter beyond `league_id` and `format=json` are required. Confirmed with a bare, cookie-free `curl` against a real league.
- Confirmed deliberately public, not an oversight: sibling account-scoped paths on the same host — `pub-api.fantasysports.yahoo.com/fantasy/v3/user/subscriptions` and `pub-api-ro.fantasysports.yahoo.com/fantasy/v2/users;use_login=1/profile` — return HTTP 401 without login, while the redzone path returns 200 with real data.
- Confirmed **not** a general mirror of the official REST API: the equivalent authenticated-shape paths (`.../league/{key}`, `.../league/{key}/standings`, `.../league/{key}/transactions`, `.../league/{key}/players;status=FA`) all return 404 on this host. Do not assume another resource is public without separately verifying it the same way.
- One response returns the entire current week for every team in the league in a single call — no pagination observed.

### Response shape (fields b9 reads)

Top-level: `service.leagues.{league_id}`, `service.players` (a lookup table by Yahoo player id), `service.injuryStatuses`.

`service.leagues.{league_id}`:
- `name`, `scoringType` (`"head"` confirmed → `HeadToHead`; other values pass through as `ScoringType::Other`), `weekInfo.start` (`YYYY-MM-DD`, its year becomes the season)
- `teams.{team_id}`: `id`, `name`, `rank`, `wins`, `losses`, `ties`, `managers.{id}.nickName` (server-redacted to the literal string `"--hidden--"` for every team observed — never a real name), `players[]`

Each roster entry in `teams.{team_id}.players[]`:
- `id`, `position` (roster slot), `eligiblePositionSlots`, `positionType`, `status` (injury code, e.g. `IL`)
- **Empty roster slots are not omitted** — they come back as a placeholder shape: `id: null`, `positionType: false` (a JSON boolean, not a string, unlike every other row), and `invalid: true`. b9's deserializer treats `id`/`positionType` as flexible types and skips any entry with `invalid: true` or a null `id` rather than fabricating a player.

`service.players.{id}`: `name`, `team` (MLB abbreviation) — used to fill in the roster entry's player identity.

Not present in the feed and not written by `pp`: the free-agent/waiver pool, transaction history, and scoring-category abbreviations/names (only numeric stat ids with no label — b9 classifies them itself; see "Matchup data from the public feed" below). Per-player Yahoo stat totals (`teams.{team_id}.players[].stats`), `league.stats` (scoring-category metadata), `league.matchupGroups` (pairings), `league.weekInfo.week`, and `league.positions` (roster-slot labels, one entry per slot) are present and, as of `b9 m`'s public-feed support, parsed and used.

### League key vs. league id

b9's configuration stores the full Yahoo league key (`{game_key}.l.{league_id}`, e.g. `469.l.170874`) in `current_league` — the same value `sync`/`st` use. `pp` needs only the trailing numeric `league_id` for the request, but writes its snapshot under a b9 storage key that existing commands can find:

- If `current_league` already resolves to the same numeric league, `pp` reuses that real key verbatim, so `sync`- and `pp`-written data land in the same place.
- Otherwise `pp` synthesizes `public.{league_id}` and sets `current_league` to it (clearing `current_team_key`, since it named a team in whatever league was previously selected).

`pp`'s league id resolves in order: an explicit `-l/--league` override (bare number or full key); `current_league`, if already set by a prior `login`/`sync`; a previously saved `pp`-only selection (`config.pull_public_league_id`); only then an interactive prompt, whose answer is saved for next time. Non-interactive with nothing resolvable fails with actionable guidance rather than hanging.

### Provenance and precedence

`sync_runs.origin` gained a fourth value, `public_pull` (`SyncOrigin::PublicPull`), alongside the existing `manual`/`automatic`/`startup`. `sync` and `pp` write the same durable fantasy tables; whichever completes successfully most recently is what's live — a plain overwrite, no field-level merge. `Store::current_data_origin` reports the `origin` of the latest **complete** run, never merely the latest run regardless of status, so a failed attempt (either source) never misreports what's actually in the tables. Redacted fields (e.g. manager nicknames) render as an explicit `--hidden--` placeholder and resolve automatically the next time an official `sync` succeeds — `pp` never attempts to infer or backfill them.

### Matchup data from the public feed

`b9 m`'s default weekly view (no `-D`/`-w`/`-a`) can source its scoreboard and both rosters from the public feed instead of OAuth, resolving "my team" from a positional `[team]` argument (`b9 m <team>`, name/manager substring match against stored teams) persisted to `config.current_team_key` on first successful resolution, matching `pp -l`'s persist-once pattern — a second `b9 m` needs no argument. `b9 m -D <date>`, `-w <week>`, and `-a` (advisory) always stay OAuth-only: the public feed's players aren't MLBAM-identity-reconciled, so a daily overlay over them would silently no-op rather than fail loudly.

Every per-player `stats` value is a **weekly, not season**, total (confirmed live/incremental within the week, not a fixed final number) — a `pp`-computed matchup score reflects the state as of the moment `pp` (or `b9 m`'s own lazy public fetch) ran. `league.stats` classifies each id (`isScoring`, `isNegative`, `group`); the ids actually observed:

- Counting (sum directly across a team's active roster — bench/IL excluded): GP(1), R(7), HR(12), RBI(13), SB(16), AB(6), H-batting(8), W(28), SV(32), K(42), OUT(33), ER(37), BB-pitching(39), H-pitching(34, `group: "pitching"` — not batting H, confirmed against real data).
- Rate (computed from summed counting stats, never summed/averaged directly): AVG(3) = ΣH(8) ÷ ΣAB(6); ERA(26) = 9 × ΣER(37) ÷ (ΣOUT(33) ÷ 3); WHIP(27) = (ΣBB(39) + ΣH(34)) ÷ (ΣOUT(33) ÷ 3). Displayed IP(50) is reformatted from ΣOUT back into Yahoo's `.1`/`.2` fractional notation — id 50 itself is never summed directly, it's not true decimal.
- Excluded: id 60 (`H/AB`), confirmed `isScoring: false` — a display-only combined stat.

`league.positions` is a **repeated-per-slot** flat array (e.g. three `"OF"` entries means three OF slots), not pre-counted the way OAuth's own settings response is — `pp` tallies it into the same `PositionWrite { position, count }` shape OAuth `sync` already writes. `pp` also now writes `current_week` from `weekInfo.week`.

`command_snapshots` rows for `b9 m`'s scoreboard (`match_scoreboard`) and roster (`match_roster`) datasets carry `source="public_pull"` alongside existing `source="yahoo"` OAuth rows for the same `(dataset, scope)` — no migration, `source` is already part of the primary key. `b9 m`'s default view reads whichever row is freshest and not stale, regardless of source: an OAuth-authenticated operator can still see public-feed data if it's fresher than their last `sync`. Because of that, the daily-overlay decision above is made per actual fetch (which source served the roster data this time), not just once from whether OAuth is currently available — a `public_pull`-sourced roster cache hit suppresses the overlay even for an authenticated caller. `b9 m -D`/`-w`/`-a` never read or write the `public_pull` source at all, so they're unaffected by any of this.

### Risk posture

Yahoo's Terms of Service likely prohibit automated access to this endpoint even though the data is publicly viewable without login — this is a Director-accepted risk, not an oversight. `pp` does not evade Yahoo's anti-automation measures if they engage: it sends a normal, identifiable request (no spoofed user agent, no header/fingerprint evasion) and does not retry past a single blocked response. It only ever requests the operator's own configured league — never an arbitrary or enumerated league id.
