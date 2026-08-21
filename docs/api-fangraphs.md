# FanGraphs Data

`skout sync` reads public leaderboard and projection JSON plus the closer-depth-chart HTML page. Complete season snapshots are validated before atomic replacement; unresolved identities are counted and skipped. FanGraphs failures retain the last complete FanGraphs snapshot.

FanGraphs player ids (`playerid`) are not guaranteed numeric — most projection rows carry an alphanumeric id (e.g. `sa3020134`) and are stored as opaque strings. A projection row's own `xMLBAMID` resolves it to an MLBAM identity when present; a leaderboard-built id crosswalk is the fallback for a row missing it.

## Availability risk

The leaderboard endpoint began returning HTTP 403 independent of request headers — an identifying `Accept` header is sent, but a plain request and one with a full browser `User-Agent` both still receive 403 from this same environment, consistent with provider-side bot detection rather than a missing header. skout does not attempt to defeat that detection (no browser automation, IP rotation, or session-cookie harvesting); a 403 here is treated the same as any other unofficial-endpoint availability failure — the prior FanGraphs snapshot is retained and the sync step reports failed.
