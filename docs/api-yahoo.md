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
- Return HTTP 401 and 403 as typed terminal-access failures without response bodies.
- Return successful raw bytes without interpreting Yahoo's fantasy JSON shape.

## Security

- Redact access tokens, refresh tokens, bearer headers, client identifiers, authorization codes, state, PKCE material, and token response bodies from debug, display, and errors.
- Keep token and credential-store types inside the Yahoo adapter.
- Keep Yahoo retries outside the shared no-retry transport.
- Keep live credential and keychain mutation outside pre-release tests.

## Fantasy Data Contract

- Acquire authenticated user leagues and team identity.
- Acquire league settings, scoring categories, roster positions, standings, and complete league rosters for foreground synchronization.
- Acquire weekly scoreboards and both matchup rosters lazily for `b9 m`.
- Traverse Yahoo numeric-key collections and accept observed array-or-object variants.
- Reject incomplete stable snapshots before normalized replacement.
- Cache weekly command payloads for 60 seconds and retain the last complete snapshot after refresh failure.
- Store league, team, player, and roster ownership in the existing schema-version-one tables.
- Keep weekly scoreboards and weekly player statistics in versioned command snapshots.
- Keep `login`, `logout`, `st`, `sync`, and the baseline `m` surface outside the provider adapter.

## Remaining Yahoo Work

- Defer free agents, transactions, waiver analysis, persisted circuits, background synchronization, and secondary commands.
- Defer weekly, daily, and advisory matchup modes beyond `m -w <week>`.

## Fixture Provenance

The scrubbed synthetic fixtures under `tests/fixtures/yahoo/` cover token, league, settings, standings, roster, matchup, weekly-stat, singleton, empty, and malformed shapes. They contain no credentials or personal data.
