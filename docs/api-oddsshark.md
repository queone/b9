# OddsShark API

## Endpoint

- Request `https://www.oddsshark.com/api/scores/mlb?date=<YYYY-MM-DD>` through skout's injected validating transport.
- Send `Referer: https://www.oddsshark.com/mlb/scores`.
- Bound each request to ten seconds and four MiB.

## Decoding

- Decode the observed top-level array, `scores`, or `games` collection.
- Retain event identity, game date, home and away team names, and nonzero American moneylines.
- Accept observed snake-case and camel-case field variants.
- Order retained games by date, teams, and event identity.

## Use and degradation

Use OddsShark only for the two future days in `skout sp`. Normalize both moneylines into vig-free implied probabilities. Match by date and clubs, preferring provider identity or start time when available. Treat malformed, missing, or unavailable future odds as optional slate context and preserve the last complete 12-hour future-odds snapshot.

OddsShark is an unofficial unauthenticated endpoint. Verify one representative future slate before release when games exist; otherwise record dated evidence that the live check is not applicable.
