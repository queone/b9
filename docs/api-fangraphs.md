# FanGraphs Data

`skout sync` reads public leaderboard and projection JSON plus the closer-depth-chart HTML page. Complete season snapshots are validated before atomic replacement; unresolved identities are counted and skipped. FanGraphs failures retain the last complete FanGraphs snapshot.

FanGraphs player ids (`playerid`) are not guaranteed numeric — most projection rows carry an alphanumeric id (e.g. `sa3020134`) and are stored as opaque strings. A projection row's own `xMLBAMID` resolves it to an MLBAM identity when present; a leaderboard-built id crosswalk is the fallback for a row missing it.
