# Skout Providers and Storage Inventory

> Historical reference: this inventory describes the fixed skout source baseline below. Do not treat its implementation or readiness claims as current b9 status. Use `skout-parity-checklist.md` for active parity tracking and inspect the skout source when correcting behavior.

Current b9 intentionally diverges from this inventory: all read-only Yahoo workflows use exact bounded paths on the two public Yahoo fantasy hosts, account discovery is replaced by explicit league and primary-team selection, complete fantasy refreshes replace atomically, and OAuth/circuit behavior is retired. The advisory subsystem is also retired.

## Source Baseline

- Source: `<skout-repo>` at clean commit `cf65984024bd10a0a41faa69b8aecd3894052c31`
- Rule: executable paths outrank documentation; cross-workstream evidence does not change manifest ownership
- Exclusions: secrets, token values, database contents, generated files, ignored files, and live interactions

## Dependency Inventory

| Dependency | Purpose and compatibility role | Owner | Rust boundary |
|---|---|---|---|
| `github.com/olekukonko/tablewriter v1.1.4` | Observable tables | Analysis/display | Defer |
| `github.com/queone/governa-color v1.4.1` | Observable color | CLI/operations | Defer |
| `github.com/spf13/cobra v1.10.2` | Command parsing | CLI/operations | Defer |
| `github.com/zalando/go-keyring v0.2.8` | Credential storage | CLI/operations | Retired in b9 |
| `golang.org/x/net v0.57.0` | HTML parsing | Providers/storage | Fixture-backed parser boundary |
| `golang.org/x/oauth2 v0.36.0` | Yahoo OAuth/refresh | Providers/storage | Preserve protocol |
| `golang.org/x/term v0.45.0` | TTY behavior | CLI/operations | Defer |
| `golang.org/x/text v0.40.0` | Identity normalization | Providers/storage | Preserve vectors |
| `gopkg.in/yaml.v3 v3.0.1` | Strategy config | Analysis/advisory | Defer |
| `modernc.org/sqlite v1.50.1` | SQLite, WAL, busy timeout | Providers/storage | Preserve semantics through pinned `rusqlite =0.40.1` with bundled SQLite |

Source: `<skout-repo>/go.mod`. Indirect modules are transitive evidence, not compatibility selections.

## Provider Contracts

| Capability | Authentication/transport | Parsing and normalization | Cache, writes, and degradation | Evidence and verification | Live |
|---|---|---|---|---|---|
| PROV-YAHOO | OAuth2 PKCE, `YAHOO_CLIENT_ID`, bearer requests, four bounded 429 retries; terminal 401/403 and five failed cycles open persisted circuit | Numeric-key JSON and array/object transaction variants into leagues, teams, players, slots, matchups, categories, transactions | 60-second disk cache for scoreboard/week stats; writes Yahoo tables, players, slots, transactions, freshness and snapshots; failures retain prior state | `internal/yahoo`; auth/parser/cache/circuit and 429 retry tests; pagination gap | OAuth, refresh, formats |
| PROV-MLB | Unauthenticated StatsAPI, 10-second client, chunked people IDs | JSON stats, schedules, boxscores, people, standings, rosters, injuries; IP/missing-ratio rules | 60-second disk cache; writes players, stats, schedule, rosters, injuries, identity, freshness and snapshots | `internal/mlb`; client/cache/roster/people/enrichment tests; throttling gap | Endpoint formats |
| PROV-SAVANT | Unauthenticated CSV, 20-second client, multiple leaderboards | Merge expected, batted-ball, sprint, arsenal, and pitcher feeds by MLBAM ID | Writes Statcast rows and freshness; successful feeds survive sibling failure | `internal/savant`; CSV/merge/fallback tests | CSV headers |
| PROV-FANGRAPHS | Unauthenticated JSON/HTML, 20-second client | Leaderboards/projections JSON; Guts and closer HTML; MLBAM crosswalk | Writes player/season fields, Statcast FG columns, projections, closers and freshness; cFIP memory cache 24h | `internal/fangraphs`; client/closer tests; live HTML gap | JSON/HTML shapes |
| PROV-FANTASYPROS | Unauthenticated HTML, 20-second client | Embedded `__NEXT_DATA__`; Yahoo ID then folded name/team; ambiguity fails closed | Writes ECR and freshness; errors preserve prior values | client and ECR identity tests | Script shape |
| PROV-ESPN | Unauthenticated scoreboard plus per-game odds, 10-second client | Caller-supplied UTC day plus next day, dedupe events, top provider, moneylines | Match writes current moneylines to `mlb_odds`; scoreboard failure aborts, per-game odds failure degrades | ESPN tests and `cmd/skout/match.go` | Unofficial APIs |
| PROV-ODDS | Unauthenticated OddsShark date endpoint with Referer, 10-second client | Date slate, team aliases, moneylines | Future `sp_odds` snapshot; failed refresh marks prior snapshot stale; no totals/K props | OddsShark and SP tests | Unofficial API |
| PROV-ROTOWIRE | Unauthenticated HTML, 15-second client | Daily lineup DOM, teams, pitchers, batting order, status aliases | Two-minute disk cache; fetch errors propagate; cache writes best effort | RotoWire HTML/cache tests | DOM shape |

No provider implements general retries or quota accounting. Yahoo alone implements provider-local bounded 429 retries with response tests. Authentication is not applicable outside Yahoo. Rate-limit response tests remain gaps for the other providers.

### Automated Provider Policy

- Reject automated Savant acquisition because the [MLB terms reviewed 2026-08-16](https://www.mlb.com/official-information/terms-of-use) prohibit automated scripts that collect from MLB digital properties.
- Reject automated FanGraphs acquisition because the [FanGraphs guidance reviewed 2026-08-16](https://blogs.fangraphs.com/contact/) says scraping and public API endpoints are not supported.
- Reject FantasyPros HTML scraping; reconsider only its [authorized API reviewed 2026-08-16](https://support.fantasypros.com/hc/en-us/articles/49749297704475-How-do-I-request-access-to-the-FantasyPros-API) after separately approved production access exists.
- Allow bounded, unauthenticated RotoWire daily-lineup acquisition only for on-demand roster status, with a two-minute cache and MLBAM fallback.
- Retain predecessor-named compatibility columns as inert schema-version-two data only; expose no b9 command, adapter, transport, credential, synchronization, or new write path for the rejected providers.
- Supplement bulk MLB pitching totals with bounded per-pitcher quality-start acquisition for starters, and preserve prior nonzero quality starts when a later bulk response omits them.

## Provider Operation Ledger

Extraction: exported production provider functions matching `Fetch*` or `GetRaw`. Constructors are boundaries, not acquisition operations.

| ID | Capability | Operation | Unit | Downstream | Evidence | Assertion/gap | Live |
|---|---|---|---|---|---|---|---|
| POP-001 | PROV-YAHOO | `GetRaw` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/client.go:162` | `<skout-repo>/internal/yahoo/client_test.go:227` assertion set | Live endpoint excluded |
| POP-002 | PROV-YAHOO | `FetchUserLeagues` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/client.go:255` | `<skout-repo>/internal/yahoo/client_test.go:23` assertion set | Live endpoint excluded |
| POP-003 | PROV-YAHOO | `FetchRoster` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/client.go:273` | `<skout-repo>/internal/yahoo/client_test.go:167` assertion set | Live endpoint excluded |
| POP-004 | PROV-YAHOO | `FetchFreeAgents` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/client.go:299` | `<skout-repo>/internal/yahoo/client_test.go:291` assertion set | Live endpoint excluded |
| POP-005 | PROV-YAHOO | `FetchScoreboard` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/client.go:398` | `<skout-repo>/internal/yahoo/client_test.go:483` assertion set | Live endpoint excluded |
| POP-006 | PROV-YAHOO | `FetchScoreboardCached` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/client.go:413` | `<skout-repo>/internal/yahoo/client_test.go:483` assertion set | Live endpoint excluded |
| POP-007 | PROV-YAHOO | `FetchRosterWeekStats` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/client.go:428` | `<skout-repo>/internal/yahoo/client_test.go:558` assertion set | Live endpoint excluded |
| POP-008 | PROV-YAHOO | `FetchRosterWeekStatsCached` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/client.go:438` | `<skout-repo>/internal/yahoo/client_test.go:558` assertion set | Live endpoint excluded |
| POP-009 | PROV-YAHOO | `FetchLeagueSettings` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/sync.go:31` | `<skout-repo>/internal/yahoo/client_test.go:23` assertion set | Live endpoint excluded |
| POP-010 | PROV-YAHOO | `FetchStandings` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/sync.go:40` | `<skout-repo>/internal/yahoo/client_test.go:23` assertion set | Live endpoint excluded |
| POP-011 | PROV-YAHOO | `FetchAllRosters` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/sync.go:57` | `<skout-repo>/internal/yahoo/client_test.go:23` assertion set | Live endpoint excluded |
| POP-012 | PROV-YAHOO | `FetchTeamKey` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/sync.go:66` | `<skout-repo>/internal/yahoo/client_test.go:23` assertion set | Live endpoint excluded |
| POP-013 | PROV-YAHOO | `FetchTransactions` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/yahoo/sync.go:72` | `<skout-repo>/internal/yahoo/client_test.go:23` assertion set | Live endpoint excluded |
| POP-014 | PROV-MLB | `FetchPlayerBirthDates` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/people.go:40` | `<skout-repo>/internal/mlb/people_test.go:16` assertion set | Live endpoint excluded |
| POP-015 | PROV-MLB | `FetchPeopleIdentities` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/people.go:94` | `<skout-repo>/internal/mlb/people_test.go:96` assertion set | Live endpoint excluded |
| POP-016 | PROV-MLB | `FetchHittingStats` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:51` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-017 | PROV-MLB | `FetchPitchingStats` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:72` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-018 | PROV-MLB | `FetchSchedule` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:95` | `<skout-repo>/internal/mlb/client_test.go:296` assertion set | Live endpoint excluded |
| POP-019 | PROV-MLB | `FetchScheduleCached` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:112` | `<skout-repo>/internal/mlb/client_test.go:296` assertion set | Live endpoint excluded |
| POP-020 | PROV-MLB | `FetchBoxscore` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:127` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-021 | PROV-MLB | `FetchBulkHittingStats` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:139` | `<skout-repo>/internal/mlb/client_test.go:131` assertion set | Live endpoint excluded |
| POP-022 | PROV-MLB | `FetchBulkPitchingStats` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:158` | `<skout-repo>/internal/mlb/client_test.go:174` assertion set | Live endpoint excluded |
| POP-023 | PROV-MLB | `FetchBulkHittingStatsByDateRange` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:176` | `<skout-repo>/internal/mlb/client_test.go:131` assertion set | Live endpoint excluded |
| POP-024 | PROV-MLB | `FetchBulkHittingStatsByDateRangeCached` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:193` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-025 | PROV-MLB | `FetchBulkPitchingStatsByDateRange` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:210` | `<skout-repo>/internal/mlb/client_test.go:174` assertion set | Live endpoint excluded |
| POP-026 | PROV-MLB | `FetchBulkPitchingStatsByDateRangeCached` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:227` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-027 | PROV-MLB | `FetchHitterGameLog` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:269` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-028 | PROV-MLB | `FetchPitcherGameLog` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:312` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-029 | PROV-MLB | `FetchQSForPitchersByDateRange` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:337` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-030 | PROV-MLB | `FetchQSForPitchers` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:388` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-031 | PROV-MLB | `FetchPlayerHands` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:422` | `<skout-repo>/internal/mlb/client_test.go:45` assertion set | Live endpoint excluded |
| POP-032 | PROV-MLB | `FetchPlayerID` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:444` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-033 | PROV-MLB | `FetchSeasonDates` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:460` | `<skout-repo>/internal/mlb/client_test.go:16` assertion set | Live endpoint excluded |
| POP-034 | PROV-MLB | `FetchAllPlayers` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:475` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-035 | PROV-MLB | `FetchAllInjuries` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:496` | `<skout-repo>/internal/mlb/client_test.go:234` assertion set | Live endpoint excluded |
| POP-036 | PROV-MLB | `FetchTeamGamesPlayed` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/client.go:538` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-037 | PROV-MLB | `FetchStandings` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/standings.go:22` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-038 | PROV-MLB | `FetchActiveRoster` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/mlb/active_roster.go:50` | `<skout-repo>/internal/mlb/active_roster_test.go:10` assertion set | Live endpoint excluded |
| POP-039 | PROV-SAVANT | `FetchHittingStatcast` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/savant/client.go:50` | `<skout-repo>/internal/savant/client_test.go:282` assertion set | Live endpoint excluded |
| POP-040 | PROV-SAVANT | `FetchPitchingStatcast` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/savant/client.go:57` | `<skout-repo>/internal/savant/client_test.go:238` assertion set | Live endpoint excluded |
| POP-041 | PROV-SAVANT | `FetchHittingStatcastMerged` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/savant/client.go:67` | `<skout-repo>/internal/savant/client_test.go:282` assertion set | Live endpoint excluded |
| POP-042 | PROV-SAVANT | `FetchPitchingStatcastMerged` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/savant/client.go:140` | `<skout-repo>/internal/savant/client_test.go:238` assertion set | Live endpoint excluded |
| POP-043 | PROV-SAVANT | `FetchByMLBID` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/savant/client.go:226` | `<skout-repo>/internal/savant/client_test.go:53` assertion set | Live endpoint excluded |
| POP-044 | PROV-FANGRAPHS | `FetchCloserDepthChart` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/fangraphs/closer_chart.go:59` | `<skout-repo>/internal/fangraphs/closer_chart_test.go:24` assertion set | Live endpoint excluded |
| POP-045 | PROV-FANGRAPHS | `FetchProjectionHitting` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/fangraphs/projections.go:40` | `<skout-repo>/internal/fangraphs/client_test.go:9` assertion set | Live endpoint excluded |
| POP-046 | PROV-FANGRAPHS | `FetchProjectionPitching` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/fangraphs/projections.go:80` | `<skout-repo>/internal/fangraphs/client_test.go:9` assertion set | Live endpoint excluded |
| POP-047 | PROV-FANGRAPHS | `FetchHittingLeaderboard` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/fangraphs/client.go:63` | `<skout-repo>/internal/fangraphs/client_test.go:9` assertion set | Live endpoint excluded |
| POP-048 | PROV-FANGRAPHS | `FetchPitchingLeaderboard` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/fangraphs/client.go:87` | `<skout-repo>/internal/fangraphs/client_test.go:41` assertion set | Live endpoint excluded |
| POP-049 | PROV-FANGRAPHS | `FetchCFIP` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/fangraphs/client.go:114` | `<skout-repo>/internal/fangraphs/client_test.go:70` assertion set | Live endpoint excluded |
| POP-050 | PROV-FANTASYPROS | `FetchECR` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/fantasypros/client.go:26` | `<skout-repo>/internal/fantasypros/client_test.go:9` assertion set | Live endpoint excluded |
| POP-051 | PROV-ESPN | `FetchGameLines` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/espnapi/client.go:68` | `<skout-repo>/internal/espnapi/client_test.go:15` assertion set | Live endpoint excluded |
| POP-052 | PROV-ODDS | `FetchGameLines` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/oddsshark/client.go:60` | `<skout-repo>/internal/oddsshark/client_test.go:10` assertion set | Live endpoint excluded |
| POP-053 | PROV-ROTOWIRE | `FetchDailyLineups` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/rotowire/client.go:84` | `<skout-repo>/internal/rotowire/rotowire_test.go:13` assertion set | Live endpoint excluded |
| POP-054 | PROV-ROTOWIRE | `FetchDailyLineupsCached` | Exported acquisition operation; request paths are PFP rows | CLI orchestration or provider-local composition | `<skout-repo>/internal/rotowire/client.go:102` | `<skout-repo>/internal/rotowire/rotowire_test.go:13` assertion set | Live endpoint excluded |

Every POP row is a pure acquisition boundary, so direct normalized writes and snapshots are `Not applicable`. Its capability identifies the exact downstream writes, snapshots, freshness gates, and failure policy in Provider-to-Persistence Flow; Cross-Workstream Evidence identifies the callers that perform them. Provider-local composition means another POP row consumes the result before the same capability flow applies.

## Fetch Path and Environment Ledger

Extraction: production provider lines containing literal `http://`, `https://`, or `YAHOO_CLIENT_ID`. Dynamic Yahoo and MLB paths route through `client.go:162-252` and `client.go:33-47`.

| ID | Capability | Declaration | Role | Evidence | Verification |
|---|---|---|---|---|---|
| EPT-001 | PROV-ROTOWIRE | lineupURL  = "https://www.rotowire.com/baseball/daily-lineups.php" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/rotowire/client.go:17` | Fixture/mock verification where present; live format check deferred |
| EPT-002 | PROV-FANGRAPHS | var closerChartURL = "https://www.fangraphs.com/roster-resource/closer-depth-chart" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/fangraphs/closer_chart.go:27` | Fixture/mock verification where present; live format check deferred |
| EPT-003 | PROV-ESPN | scoreboardURL = "https://site.api.espn.com/apis/site/v2/sports/baseball/mlb/scoreboard" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/espnapi/client.go:23` | Fixture/mock verification where present; live format check deferred |
| EPT-004 | PROV-ESPN | oddsURLFmt    = "https://sports.core.api.espn.com/v2/sports/baseball/leagues/mlb/events/%s/competitions/%s/odds" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/espnapi/client.go:24` | Fixture/mock verification where present; live format check deferred |
| EPT-005 | PROV-YAHOO | const yahooBaseURL = "https://fantasysports.yahooapis.com/fantasy/v2" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/yahoo/client.go:36` | Fixture/mock verification where present; live format check deferred |
| EPT-006 | PROV-FANGRAPHS | var projectionURL = "https://www.fangraphs.com/api/projections?type=%s&stats=%s&pos=all&season=%d&sortstat=ADP&sortorder=desc&page=1_5000" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/fangraphs/projections.go:10` | Fixture/mock verification where present; live format check deferred |
| EPT-007 | PROV-FANGRAPHS | battingLeaderboardURL  = "https://www.fangraphs.com/api/leaders/major-league/data?pos=all&stats=bat&lg=all&qual=0&season=%d&season1=%d&type=8&month=0&pageItems=2000&ind=0" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/fangraphs/client.go:18` | Fixture/mock verification where present; live format check deferred |
| EPT-008 | PROV-FANGRAPHS | pitchingLeaderboardURL = "https://www.fangraphs.com/api/leaders/major-league/data?pos=all&stats=pit&lg=all&qual=0&season=%d&season1=%d&type=8&month=0&pageItems=2000&ind=0" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/fangraphs/client.go:19` | Fixture/mock verification where present; live format check deferred |
| EPT-009 | PROV-FANGRAPHS | GutsURL                = "https://www.fangraphs.com/tools/guts" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/fangraphs/client.go:20` | Fixture/mock verification where present; live format check deferred |
| EPT-010 | PROV-ODDS | var baseURL = "https://www.oddsshark.com/api/scores/mlb" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/oddsshark/client.go:18` | Fixture/mock verification where present; live format check deferred |
| EPT-011 | PROV-ODDS | req.Header.Set("Referer", "https://www.oddsshark.com/mlb/scores") | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/oddsshark/client.go:67` | Fixture/mock verification where present; live format check deferred |
| EPT-012 | PROV-SAVANT | battingLeaderURL = "https://baseballsavant.mlb.com/leaderboard/expected_statistics?type=batter&year=%d&position=&team=&min=%d&csv=true" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/savant/client.go:17` | Fixture/mock verification where present; live format check deferred |
| EPT-013 | PROV-SAVANT | battingStatcastURL = "https://baseballsavant.mlb.com/leaderboard/statcast?type=batter&year=%d&position=&team=&min=%d&csv=true" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/savant/client.go:19` | Fixture/mock verification where present; live format check deferred |
| EPT-014 | PROV-SAVANT | sprintSpeedURL = "https://baseballsavant.mlb.com/leaderboard/sprint_speed?min_opp=0&year=%d&team=&position=&csv=true" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/savant/client.go:21` | Fixture/mock verification where present; live format check deferred |
| EPT-015 | PROV-SAVANT | stuffPlusURL = "https://baseballsavant.mlb.com/leaderboard/pitcher-quality?type=n&year=%d&min=%d&csv=true" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/savant/client.go:23` | Fixture/mock verification where present; live format check deferred |
| EPT-016 | PROV-SAVANT | pitcherExpectedURL = "https://baseballsavant.mlb.com/leaderboard/expected_statistics?type=pitcher&year=%d&position=&team=&min=%d&csv=true" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/savant/client.go:25` | Fixture/mock verification where present; live format check deferred |
| EPT-017 | PROV-SAVANT | pitcherBattedBallURL = "https://baseballsavant.mlb.com/leaderboard/statcast?type=pitcher&year=%d&position=&team=&min=%d&csv=true" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/savant/client.go:27` | Fixture/mock verification where present; live format check deferred |
| EPT-018 | PROV-SAVANT | pitchArsenalSpeedURL = "https://baseballsavant.mlb.com/leaderboard/pitch-arsenals?year=%d&min=%d&type=avg_speed&hand=&csv=true" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/savant/client.go:29` | Fixture/mock verification where present; live format check deferred |
| EPT-019 | PROV-SAVANT | pitchArsenalSpinURL = "https://baseballsavant.mlb.com/leaderboard/pitch-arsenals?year=%d&min=%d&type=avg_spin&hand=&csv=true" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/savant/client.go:31` | Fixture/mock verification where present; live format check deferred |
| EPT-020 | PROV-YAHOO | authURL  = "https://api.login.yahoo.com/oauth2/request_auth" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/yahoo/auth.go:22` | Fixture/mock verification where present; live format check deferred |
| EPT-021 | PROV-YAHOO | tokenURL = "https://api.login.yahoo.com/oauth2/get_token" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/yahoo/auth.go:23` | Fixture/mock verification where present; live format check deferred |
| EPT-022 | PROV-YAHOO | redirectURL = "https://localhost:8080/callback" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/yahoo/auth.go:28` | Fixture/mock verification where present; live format check deferred |
| EPT-023 | PROV-YAHOO | ClientID:    os.Getenv("YAHOO_CLIENT_ID"), | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/yahoo/auth.go:33` | Fixture/mock verification where present; live format check deferred |
| EPT-024 | PROV-YAHOO | return fmt.Errorf("YAHOO_CLIENT_ID environment variable must be set") | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/yahoo/auth.go:50` | Fixture/mock verification where present; live format check deferred |
| EPT-025 | PROV-FANTASYPROS | var rankingsURL = "https://www.fantasypros.com/mlb/rankings/overall.php" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/fantasypros/client.go:14` | Fixture/mock verification where present; live format check deferred |
| EPT-026 | PROV-MLB | var baseURL = "https://statsapi.mlb.com/api/v1" | Central request constructor or OAuth/environment boundary | `<skout-repo>/internal/mlb/client.go:24` | Fixture/mock verification where present; live format check deferred |

No extraction false positives occur. Redirect URLs are OAuth evidence rather than data fetches.

### Request Construction Sites

Extraction: production calls that construct or execute HTTP requests. Shared helpers represent the single construction site for their dynamic operation paths.

| ID | Capability | Construction site | Operation mapping | Evidence | Verification |
|---|---|---|---|---|---|
| PFP-001 | PROV-ODDS | req, err := http.NewRequest(http.MethodGet, url, nil) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/oddsshark/client.go:62` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-002 | PROV-ODDS | resp, err := httpClient.Do(req) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/oddsshark/client.go:69` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-003 | PROV-ROTOWIRE | resp, err := httpClient.Get(lineupURL) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/rotowire/client.go:85` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-004 | PROV-MLB | resp, err := httpClient.Get(url) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/mlb/client.go:34` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-005 | PROV-FANGRAPHS | req, err := http.NewRequest(http.MethodGet, closerChartURL, nil) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/fangraphs/closer_chart.go:60` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-006 | PROV-FANGRAPHS | resp, err := httpClient.Do(req) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/fangraphs/closer_chart.go:67` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-007 | PROV-FANGRAPHS | resp, err := httpClient.Get(url) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/fangraphs/client.go:153` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-008 | PROV-FANGRAPHS | resp, err := httpClient.Get(GutsURL) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/fangraphs/client.go:187` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-009 | PROV-SAVANT | resp, err := httpClient.Get(url) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/savant/client.go:241` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-010 | PROV-FANTASYPROS | resp, err := httpClient.Get(rankingsURL) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/fantasypros/client.go:27` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-011 | PROV-ESPN | resp, err := httpClient.Get(url) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/espnapi/client.go:189` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-012 | PROV-FANGRAPHS | resp, err := httpClient.Get(url) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/fangraphs/projections.go:42` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-013 | PROV-FANGRAPHS | resp, err := httpClient.Get(url) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/fangraphs/projections.go:82` | Provider-local mock/fixture tests or explicit operation gap |
| PFP-014 | PROV-YAHOO | resp, err := c.http.Get(url) | Enclosing exported operation or shared transport helper | `<skout-repo>/internal/yahoo/client.go:177` | Provider-local mock/fixture tests or explicit operation gap |

Every PFP row maps to the enclosing or calling POP operation in its cited source. Request sites perform no direct storage mutation; their capability flow row and cross-workstream callers provide the normalized-write, snapshot, freshness, fallback, and failure mapping.

## Provider-to-Persistence Flow

| Provider | Identity | Writes | Snapshots | Freshness/fallback |
|---|---|---|---|---|
| Yahoo | Yahoo league/team/player keys, later MLBAM crosswalk | leagues, categories, positions, teams, players, slots, transactions | match scoreboard/roster and roster totals | Item/row state; disk cache; prior snapshots |
| MLB | MLBAM ID, team abbreviation, game PK | players, season stats, schedule, active rosters, injuries | match, player-card, game-note, schedule, standings, supplements | Item/row/season state; disk cache; stale snapshots |
| Savant | MLBAM ID/stat group | `statcast_seasons` | None | Item/player state; partial-feed warnings |
| FanGraphs | MLBAM ID/season/source/group | player/season fields, Statcast FG columns, projections, closers | None | Item/row/season state |
| FantasyPros | Yahoo ID then folded name/team | `players.ecr` | None | Item/row state; ambiguity skips |
| ESPN | Game PK/team side | `mlb_odds` moneylines | None | Store timestamp; missing price degrades |
| OddsShark | Date/game/team side | None | `sp_odds` | Snapshot stale fallback |
| RotoWire | Normalized team/player names | None | Caller game-note context | Two-minute disk cache |

## Cache Contract

| ID | Path/format | TTL/invalidation | Atomicity/permissions | Corruption/failure | Evidence |
|---|---|---|---|---|---|
| CAC-001 | User cache dir, fallback `~/.cache`, `skout/api-cache/<filename>`; JSON timestamp/payload envelope | Caller TTL; expiry at age ≥ TTL; prune siblings older than 24h on put | Process mutex; dir 0700; file 0600; direct non-rename write; cross-process race tolerated | Missing, malformed, zero-time and expired are misses; fetch error returns; post-fetch write/prune errors ignored | `internal/cache/disk.go` and all `disk_test.go` assertions |

## SQLite Schema and Migration

- Open `~/.config/skout/skout.db`; create parent 0700; use WAL and 5000 ms busy timeout.
- Apply version 36 with idempotent table creation.
- Preserve domain tables for missing/stale versions; add `sync_runs.origin` when absent; replace version row.
- Propagate unreadable existing version state.
- Declare no foreign keys or standalone indexes; primary keys and CHECK constraints enforce stored relationships.
- Treat the test named `TestMigrate_DestructiveUpgradeFromV26` as stale naming because its current assertion verifies preservation.
- Record earlier historical transitions as unreconstructable gaps.

| ID | Table | Constraints | Evidence | Assertion/gap |
|---|---|---|---|---|
| SCH-001 | `schema_version` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:9` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-002 | `sync_log` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:13` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-003 | `sync_item_state` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:18` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-004 | `sync_row_state` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:30` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-005 | `command_snapshots` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:45` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-006 | `yahoo_leagues` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:57` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-007 | `yahoo_stat_categories` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:72` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-008 | `yahoo_roster_positions` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:83` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-009 | `yahoo_teams` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:90` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-010 | `players` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:108` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-011 | `statcast_seasons` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:141` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-012 | `yahoo_roster_slots` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:178` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-013 | `mlbam_season_stats` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:186` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-014 | `mlb_game_schedule` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:261` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-015 | `season_sync_status` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:269` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-016 | `sync_runs` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:279` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-017 | `projection_seasons` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:289` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-018 | `yahoo_transactions` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:313` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-019 | `mlb_team_active_rosters` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:328` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |
| SCH-020 | `mlb_odds` | Primary and CHECK constraints in schema text; no declared foreign keys or standalone indexes | `<skout-repo>/internal/store/schema.go:338` | `store_test.go:TestOpen_createsSchema`; table-specific assertions or explicit gaps in SAP ledger |

## Storage Behavioral Groups

| Group | Contract | Evidence |
|---|---|---|
| STO-IDENTITY | Resolve external IDs and normalized name/team/position/jersey tiers; prefer seed/canonical rows; preserve two-way rows; ambiguity fails closed | `identity.go`, tests, canonical identity doc |
| STO-ROSTERS | Replace team/league slot sets transactionally; validate complete league set before delete; reconstruct keys and role views | `player.go`, `team_roster.go`, tests |
| STO-STATS | Upsert player/season/group data with documented transaction boundaries; preserve QS on zero input; manifest complete/partial/version state | player, Statcast, projection, manifest sources/tests |
| STO-ODDS | Delete only supplied game PKs then replace within a transaction; absent games stay absent; zero freshness when empty | `odds.go`, tests, CF-001 |
| STO-SNAPSHOTS | Save complete JSON only; failed refresh marks stale/error without replacing payload; exact dataset/source/scope key | snapshot source/tests and cross-evidence |
| STO-FRESHNESS | Attempts do not advance success; failures preserve prior success; missing/non-complete/stale/version mismatch needs sync; unscoped legacy fallback | store/snapshot/manifest sources |
| STO-RUNS | Track running/complete/failed with JSON counts and manual/automatic/startup origin | `syncrun.go` |
| STO-BASE | Additive migration is not one outer transaction; SQL filters/order/null/default behavior is normative | `store.go`, `schema.go`, tests |

## Public Store API Ledger

Extraction: every exported `*Store` method. The cited SQL/return contract defines filters, ordering, joins, nulls, defaults and failures.

| ID | API | Kind | Contract | Evidence | Assertion/gap |
|---|---|---|---|---|---|
| SAP-001 | `UpsertGameSchedule` | Write/lifecycle | schedule replacement and stale-filtered reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/schedule.go:15` | `<skout-repo>/internal/store/schedule_test.go:8` |
| SAP-002 | `GetTodaySchedule` | Read/query | schedule replacement and stale-filtered reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/schedule.go:39` | `<skout-repo>/internal/store/schedule_test.go:8` |
| SAP-003 | `GetSchedule` | Read/query | schedule replacement and stale-filtered reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/schedule.go:44` | `<skout-repo>/internal/store/schedule_test.go:8` |
| SAP-004 | `GetScheduleAny` | Read/query | schedule replacement and stale-filtered reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/schedule.go:79` | Explicit gap: no API-named assertion |
| SAP-005 | `UpsertProjections` | Write/lifecycle | transactional projection upsert and source blend reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/projection.go:29` | `<skout-repo>/internal/store/projection_test.go:5` |
| SAP-006 | `GetProjectionBlend` | Read/query | transactional projection upsert and source blend reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/projection.go:64` | `<skout-repo>/internal/store/projection_test.go:31` |
| SAP-007 | `ResolvePlayer` | Boundary | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:52` | `<skout-repo>/internal/store/identity_test.go:9` |
| SAP-008 | `UpsertPlayer` | Write/lifecycle | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:58` | `<skout-repo>/internal/store/store_test.go:516` |
| SAP-009 | `GetUnmatchedPlayers` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:256` | `<skout-repo>/internal/store/identity_test.go:130` |
| SAP-010 | `GetUnmatchedCount` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:278` | Explicit gap: no API-named assertion |
| SAP-011 | `GetMLBAMPlayersForReconciliation` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:304` | `<skout-repo>/internal/store/identity_test.go:613` |
| SAP-012 | `DetectMisboundPlayers` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:366` | Explicit gap: no API-named assertion |
| SAP-013 | `DetectJerseyTeamMisbindings` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:466` | `<skout-repo>/internal/store/identity_test.go:954` |
| SAP-014 | `UnreconcilePlayer` | Write/lifecycle | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:517` | `<skout-repo>/internal/store/identity_test.go:738` |
| SAP-015 | `ReconcilePlayer` | Write/lifecycle | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:529` | `<skout-repo>/internal/store/identity_test.go:112` |
| SAP-016 | `GetPlayerIDByMLBAM` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:541` | `<skout-repo>/internal/store/identity_test.go:190` |
| SAP-017 | `ResolveStatPlayerID` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:554` | Explicit gap: no API-named assertion |
| SAP-018 | `CanonicalPlayerID` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:571` | Explicit gap: no API-named assertion |
| SAP-019 | `Upsert40ManIdentity` | Write/lifecycle | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:589` | `<skout-repo>/internal/store/identity_test.go:459` |
| SAP-020 | `MLBAMIDsPresent` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:623` | Explicit gap: no API-named assertion |
| SAP-021 | `ResolveClosingCandidate` | Boundary | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:662` | `<skout-repo>/internal/store/identity_test.go:772` |
| SAP-022 | `GetCurrentCloserDesignations` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:721` | `<skout-repo>/internal/store/identity_test.go:825` |
| SAP-023 | `MLBAMIDByJerseyTeam` | Read/query | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:764` | `<skout-repo>/internal/store/identity_test.go:857` |
| SAP-024 | `UpdateMLBTeamByMLBAM` | Write/lifecycle | canonical identity matching, two-way preference, reconciliation, and fail-closed ambiguity; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/identity.go:873` | `<skout-repo>/internal/store/identity_test.go:545` |
| SAP-025 | `GetMLBTeamActiveRoster` | Read/query | complete per-team roster replacement and active-role reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/team_roster.go:98` | `<skout-repo>/internal/store/team_roster_test.go:59` |
| SAP-026 | `UpsertMLBTeamActiveRoster` | Write/lifecycle | complete per-team roster replacement and active-role reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/team_roster.go:210` | `<skout-repo>/internal/store/team_roster_test.go:59` |
| SAP-027 | `MLBRosteredIDsByRole` | Boundary | complete per-team roster replacement and active-role reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/team_roster.go:260` | `<skout-repo>/internal/store/team_roster_test.go:270` |
| SAP-028 | `MLBRosteredHitterPositions` | Boundary | complete per-team roster replacement and active-role reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/team_roster.go:338` | `<skout-repo>/internal/store/team_roster_test.go:553` |
| SAP-029 | `MLBRosteredStatLookups` | Boundary | complete per-team roster replacement and active-role reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/team_roster.go:379` | `<skout-repo>/internal/store/team_roster_test.go:309` |
| SAP-030 | `MLBTeamRosterFetchedAt` | Boundary | complete per-team roster replacement and active-role reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/team_roster.go:444` | `<skout-repo>/internal/store/team_roster_test.go:338` |
| SAP-031 | `UpsertStatcastHittingSeason` | Write/lifecycle | partial-column upserts and current/prior aggregate reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/statcast.go:49` | Explicit gap: no API-named assertion |
| SAP-032 | `UpsertStatcastPitchingSeason` | Write/lifecycle | partial-column upserts and current/prior aggregate reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/statcast.go:91` | Explicit gap: no API-named assertion |
| SAP-033 | `UpsertStatcastFGBatting` | Write/lifecycle | partial-column upserts and current/prior aggregate reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/statcast.go:122` | Explicit gap: no API-named assertion |
| SAP-034 | `UpsertStatcastFGPitching` | Write/lifecycle | partial-column upserts and current/prior aggregate reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/statcast.go:138` | Explicit gap: no API-named assertion |
| SAP-035 | `GetStatcastPair` | Read/query | partial-column upserts and current/prior aggregate reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/statcast.go:159` | `<skout-repo>/internal/store/store_test.go:760` |
| SAP-036 | `GetLeagueMeans` | Read/query | partial-column upserts and current/prior aggregate reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/statcast.go:199` | Explicit gap: no API-named assertion |
| SAP-037 | `GetMLBAMIDsMissingHands` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:129` | `<skout-repo>/internal/store/store_test.go:955` |
| SAP-038 | `UpdatePlayerHands` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:151` | `<skout-repo>/internal/store/store_test.go:955` |
| SAP-039 | `UpsertPQS` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:163` | `<skout-repo>/internal/store/store_test.go:873` |
| SAP-040 | `BatchUpsertPQS` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:173` | `<skout-repo>/internal/store/store_test.go:899` |
| SAP-041 | `GetLeagueGamesPlayed` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:196` | Explicit gap: no API-named assertion |
| SAP-042 | `UpsertMLBAMInjuries` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:207` | `<skout-repo>/internal/store/store_test.go:790` |
| SAP-043 | `GetMedianPQS` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:237` | Explicit gap: no API-named assertion |
| SAP-044 | `GetPlayerNames` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:251` | `<skout-repo>/internal/store/store_test.go:847` |
| SAP-045 | `GetMLBAMToPlayerIDMap` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:273` | `<skout-repo>/internal/store/identity_test.go:222` |
| SAP-046 | `GetYahooToMLBAMMap` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:299` | Explicit gap: no API-named assertion |
| SAP-047 | `ReplaceRosterSlots` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:317` | `<skout-repo>/internal/store/store_test.go:234` |
| SAP-048 | `ReplaceLeagueRosterSlots` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:346` | `<skout-repo>/internal/store/store_test.go:265` |
| SAP-049 | `UpsertMLBAMSeasonStats` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:420` | `<skout-repo>/internal/store/store_test.go:414` |
| SAP-050 | `HasSeasonStats` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:516` | `<skout-repo>/internal/store/store_test.go:1074` |
| SAP-051 | `GetMLBAMPitcherIDs` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:526` | Explicit gap: no API-named assertion |
| SAP-052 | `UpdateMLBAMQS` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:550` | `<skout-repo>/internal/store/identity_test.go:241` |
| SAP-053 | `GetRosterPlayers` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:567` | `<skout-repo>/internal/store/store_test.go:414` |
| SAP-054 | `GetLeagueFreeAgents` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:612` | `<skout-repo>/internal/store/store_test.go:466` |
| SAP-055 | `GetRosterPlayersSpring` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:660` | Explicit gap: no API-named assertion |
| SAP-056 | `GetLeagueFreeAgentsSpring` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:703` | Explicit gap: no API-named assertion |
| SAP-057 | `GetAllPlayers` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:947` | `<skout-repo>/internal/store/store_test.go:993` |
| SAP-058 | `GetPlayersByName` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1112` | Explicit gap: no API-named assertion |
| SAP-059 | `GetOwnersByPlayerName` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1171` | `<skout-repo>/internal/store/store_test.go:1658` |
| SAP-060 | `GetPlayerSeasonHistory` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1223` | `<skout-repo>/internal/store/store_test.go:1610` |
| SAP-061 | `MarkClosers` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1404` | `<skout-repo>/internal/store/player_test.go:64` |
| SAP-062 | `UpsertFanGraphsHitting` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1526` | Explicit gap: no API-named assertion |
| SAP-063 | `UpsertFanGraphsPitchingWAR` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1540` | Explicit gap: no API-named assertion |
| SAP-064 | `UpsertFantasyProsECRByYahooID` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1558` | `<skout-repo>/internal/store/ecr_test.go:56` |
| SAP-065 | `UpsertFantasyProsECRByName` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1622` | `<skout-repo>/internal/store/ecr_test.go:155` |
| SAP-066 | `UpsertFanGraphsSeasonStats` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1698` | `<skout-repo>/internal/store/identity_test.go:267` |
| SAP-067 | `HasQSData` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1715` | Explicit gap: no API-named assertion |
| SAP-068 | `GetBrowsePlayers` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1727` | `<skout-repo>/internal/store/store_test.go:1325` |
| SAP-069 | `SelectStaleBirthDateMLBAMIDs` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1790` | Explicit gap: no API-named assertion |
| SAP-070 | `UpdatePlayerBirthDateByMLBAMID` | Write/lifecycle | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1817` | Explicit gap: no API-named assertion |
| SAP-071 | `HasFanGraphsStats` | Read/query | player/stat writes and roster, browse, ownership, history, freshness reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/player.go:1825` | Explicit gap: no API-named assertion |
| SAP-072 | `UpsertOddsLines` | Write/lifecycle | game-set replacement, normalized odds reads, and max freshness; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/odds.go:64` | `<skout-repo>/internal/store/odds_test.go:8` |
| SAP-073 | `GetOddsForGames` | Read/query | game-set replacement, normalized odds reads, and max freshness; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/odds.go:126` | `<skout-repo>/internal/store/odds_test.go:93` |
| SAP-074 | `OddsCacheFetchedAt` | Read/query | game-set replacement, normalized odds reads, and max freshness; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/odds.go:209` | `<skout-repo>/internal/store/odds_test.go:70` |
| SAP-075 | `UpsertTransactions` | Write/lifecycle | append-only immutable transaction facts; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/transaction.go:22` | `<skout-repo>/internal/store/store_test.go:584` |
| SAP-076 | `UpsertLeague` | Write/lifecycle | league settings replacement and ordered category/position reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/league.go:39` | `<skout-repo>/internal/store/store_test.go:152` |
| SAP-077 | `GetLeague` | Read/query | league settings replacement and ordered category/position reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/league.go:53` | `<skout-repo>/internal/store/store_test.go:152` |
| SAP-078 | `UpsertStatCategories` | Write/lifecycle | league settings replacement and ordered category/position reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/league.go:66` | `<skout-repo>/internal/store/store_test.go:187` |
| SAP-079 | `UpsertRosterPositions` | Write/lifecycle | league settings replacement and ordered category/position reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/league.go:93` | Explicit gap: no API-named assertion |
| SAP-080 | `GetRosterPositions` | Read/query | league settings replacement and ordered category/position reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/league.go:118` | Explicit gap: no API-named assertion |
| SAP-081 | `GetStatCategories` | Read/query | league settings replacement and ordered category/position reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/league.go:139` | `<skout-repo>/internal/store/store_test.go:187` |
| SAP-082 | `UpsertTeam` | Write/lifecycle | team replacement and league/team reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/team.go:24` | `<skout-repo>/internal/store/store_test.go:211` |
| SAP-083 | `GetTeams` | Read/query | team replacement and league/team reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/team.go:38` | `<skout-repo>/internal/store/identity_test.go:295` |
| SAP-084 | `GetTeam` | Read/query | team replacement and league/team reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/team.go:65` | `<skout-repo>/internal/store/identity_test.go:295` |
| SAP-085 | `UpsertCloserDesignations` | Write/lifecycle | transactional closer-set replacement; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/closer_override.go:13` | `<skout-repo>/internal/store/closer_override_test.go:8` |
| SAP-086 | `Close` | Boundary | open, WAL, busy timeout, migration, legacy freshness, and database summary; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/store.go:59` | `<skout-repo>/internal/store/identity_test.go:825` |
| SAP-087 | `NeedsSync` | Read/query | open, WAL, busy timeout, migration, legacy freshness, and database summary; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/store.go:160` | `<skout-repo>/internal/store/store_test.go:101` |
| SAP-088 | `MarkSynced` | Write/lifecycle | open, WAL, busy timeout, migration, legacy freshness, and database summary; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/store.go:165` | Explicit gap: no API-named assertion |
| SAP-089 | `DB` | Read/query | open, WAL, busy timeout, migration, legacy freshness, and database summary; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/store.go:177` | `<skout-repo>/internal/store/identity_test.go:190` |
| SAP-090 | `SyncTime` | Read/query | open, WAL, busy timeout, migration, legacy freshness, and database summary; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/store.go:183` | Explicit gap: no API-named assertion |
| SAP-091 | `IsEmpty` | Read/query | open, WAL, busy timeout, migration, legacy freshness, and database summary; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/store.go:196` | `<skout-repo>/internal/store/store_test.go:133` |
| SAP-092 | `GetDBStats` | Read/query | open, WAL, busy timeout, migration, legacy freshness, and database summary; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/store.go:211` | Explicit gap: no API-named assertion |
| SAP-093 | `NeedsSyncItem` | Read/query | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:62` | Explicit gap: no API-named assertion |
| SAP-094 | `MarkSyncItemAttempt` | Write/lifecycle | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:82` | Explicit gap: no API-named assertion |
| SAP-095 | `MarkSyncItemSuccess` | Write/lifecycle | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:97` | Explicit gap: no API-named assertion |
| SAP-096 | `MarkSyncItemFailure` | Write/lifecycle | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:114` | Explicit gap: no API-named assertion |
| SAP-097 | `MarkSyncRowSuccess` | Write/lifecycle | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:134` | Explicit gap: no API-named assertion |
| SAP-098 | `MarkSyncRowFailure` | Write/lifecycle | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:153` | Explicit gap: no API-named assertion |
| SAP-099 | `NeedsSyncRow` | Read/query | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:175` | Explicit gap: no API-named assertion |
| SAP-100 | `SaveCommandSnapshot` | Write/lifecycle | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:190` | Explicit gap: no API-named assertion |
| SAP-101 | `GetCommandSnapshot` | Read/query | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:206` | `<skout-repo>/internal/store/snapshots_test.go:47` |
| SAP-102 | `MarkCommandSnapshotStale` | Write/lifecycle | item/row freshness and durable command fallback; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/snapshots.go:225` | Explicit gap: no API-named assertion |
| SAP-103 | `StartSyncRun` | Write/lifecycle | run lifecycle and last-run reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/syncrun.go:20` | Explicit gap: no API-named assertion |
| SAP-104 | `StartSyncRunWithOrigin` | Write/lifecycle | run lifecycle and last-run reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/syncrun.go:25` | Explicit gap: no API-named assertion |
| SAP-105 | `CompleteSyncRun` | Write/lifecycle | run lifecycle and last-run reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/syncrun.go:38` | Explicit gap: no API-named assertion |
| SAP-106 | `FailSyncRun` | Write/lifecycle | run lifecycle and last-run reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/syncrun.go:47` | Explicit gap: no API-named assertion |
| SAP-107 | `GetLastSyncRun` | Read/query | run lifecycle and last-run reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/syncrun.go:55` | Explicit gap: no API-named assertion |
| SAP-108 | `GetLastSuccessfulSyncRun` | Read/query | run lifecycle and last-run reads; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/syncrun.go:72` | Explicit gap: no API-named assertion |
| SAP-109 | `RecomputeFIP` | Write/lifecycle | stored FIP recomputation; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/fip.go:42` | Explicit gap: no API-named assertion |
| SAP-110 | `IsSeasonComplete` | Read/query | season completeness and pipeline-version gates; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/manifest.go:17` | Explicit gap: no API-named assertion |
| SAP-111 | `MarkSeasonComplete` | Write/lifecycle | season completeness and pipeline-version gates; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/manifest.go:31` | Explicit gap: no API-named assertion |
| SAP-112 | `GetSeasonStatus` | Read/query | season completeness and pipeline-version gates; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/manifest.go:48` | Explicit gap: no API-named assertion |
| SAP-113 | `MarkSeasonPartial` | Write/lifecycle | season completeness and pipeline-version gates; SQL ordering, filters, nulls, and defaults are normative | `<skout-repo>/internal/store/manifest.go:64` | Explicit gap: no API-named assertion |

## Durable Snapshot Ledger

Extraction: literal datasets passed to snapshot save/get/stale calls in authorized CLI evidence.

| ID | Dataset | Calls | Completeness/failure | Freshness | Fallback | Verification |
|---|---|---|---|---|---|---|
| SNP-001 | `game_notes_context` | `<skout-repo>/cmd/skout/playerpool.go:100`, `<skout-repo>/cmd/skout/playerpool.go:101`, `<skout-repo>/cmd/skout/playerpool.go:123` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-002 | `game_notes_schedule` | `<skout-repo>/cmd/skout/playerpool.go:104`, `<skout-repo>/cmd/skout/playerpool.go:107`, `<skout-repo>/cmd/skout/playerpool.go:109`, `<skout-repo>/cmd/skout/playerpool.go:445`, `<skout-repo>/cmd/skout/playerpool.go:451` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-003 | `match_boxscores` | `<skout-repo>/cmd/skout/match.go:47`, `<skout-repo>/cmd/skout/match.go:63`, `<skout-repo>/cmd/skout/match.go:430`, `<skout-repo>/cmd/skout/match.go:503` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-004 | `match_hitting_overlay` | `<skout-repo>/cmd/skout/match.go:443`, `<skout-repo>/cmd/skout/match.go:445`, `<skout-repo>/cmd/skout/match.go:447` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-005 | `match_pitching_overlay` | `<skout-repo>/cmd/skout/match.go:458`, `<skout-repo>/cmd/skout/match.go:460`, `<skout-repo>/cmd/skout/match.go:462` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-006 | `match_roster` | `<skout-repo>/cmd/skout/match.go:307`, `<skout-repo>/cmd/skout/match.go:309`, `<skout-repo>/cmd/skout/match.go:315`, `<skout-repo>/cmd/skout/match.go:322`, `<skout-repo>/cmd/skout/match.go:324`, `<skout-repo>/cmd/skout/match.go:331` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-007 | `match_schedule` | `<skout-repo>/cmd/skout/match.go:184`, `<skout-repo>/cmd/skout/match.go:186`, `<skout-repo>/cmd/skout/match.go:192`, `<skout-repo>/cmd/skout/match.go:334`, `<skout-repo>/cmd/skout/match.go:336`, `<skout-repo>/cmd/skout/match.go:342` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-008 | `match_scoreboard` | `<skout-repo>/cmd/skout/match.go:173`, `<skout-repo>/cmd/skout/match.go:175`, `<skout-repo>/cmd/skout/match.go:181`, `<skout-repo>/cmd/skout/match.go:639` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-009 | `mlb_standings` | `<skout-repo>/cmd/skout/teams_totals.go:42`, `<skout-repo>/cmd/skout/teams_totals.go:43`, `<skout-repo>/cmd/skout/teams_totals.go:51`, `<skout-repo>/cmd/skout/team.go:120`, `<skout-repo>/cmd/skout/team.go:123`, `<skout-repo>/cmd/skout/team.go:124` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-010 | `player_card_boxscore` | `<skout-repo>/cmd/skout/playercard_gamelog.go:166`, `<skout-repo>/cmd/skout/playercard_gamelog.go:174`, `<skout-repo>/cmd/skout/playercard_gamelog.go:182` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-011 | `player_card_schedule` | `<skout-repo>/cmd/skout/playercard_gamelog.go:141`, `<skout-repo>/cmd/skout/playercard_gamelog.go:149`, `<skout-repo>/cmd/skout/playercard_gamelog.go:157` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-012 | `player_game_log` | `<skout-repo>/cmd/skout/playerpool.go:402`, `<skout-repo>/cmd/skout/playerpool.go:404`, `<skout-repo>/cmd/skout/playerpool.go:420`, `<skout-repo>/cmd/skout/playerpool.go:422` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-013 | `roster_totals_scoreboard` | `<skout-repo>/cmd/skout/roster_totals.go:147`, `<skout-repo>/cmd/skout/roster_totals.go:148`, `<skout-repo>/cmd/skout/roster_totals.go:156`, `<skout-repo>/cmd/skout/roster_totals.go:721`, `<skout-repo>/cmd/skout/roster_totals.go:722`, `<skout-repo>/cmd/skout/roster_totals.go:727` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-014 | `roster_totals_supplements` | `<skout-repo>/cmd/skout/roster_totals.go:415`, `<skout-repo>/cmd/skout/roster_totals.go:419`, `<skout-repo>/cmd/skout/roster_totals.go:534` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-015 | `roster_totals_weekly` | `<skout-repo>/cmd/skout/roster_totals.go:195`, `<skout-repo>/cmd/skout/roster_totals.go:201`, `<skout-repo>/cmd/skout/roster_totals.go:209` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-016 | `sp_odds` | `<skout-repo>/cmd/skout/sp.go:95`, `<skout-repo>/cmd/skout/sp.go:96`, `<skout-repo>/cmd/skout/sp.go:108` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |
| SNP-017 | `sp_schedule` | `<skout-repo>/cmd/skout/sp.go:67`, `<skout-repo>/cmd/skout/sp.go:68`, `<skout-repo>/cmd/skout/sp.go:76` | Save only after complete payload marshal; failure marks stale without replacing payload | Dataset/source/scope key; latest success retained | Prior decodable payload | Explicit gap unless cited command tests exercise fallback |

## Freshness Gates

| ID | Dataset | TTL | Force/version and fallback | Evidence |
|---|---|---|---|---|
| FR-001 | League, teams | 24h, 1h | Force bypass; stored rows remain | `sync.go:31-47,1045-1090` |
| FR-002 | Rosters, free agents, transactions | 15m | Force bypass; validated complete roster set | `sync.go:489-505,1108-1157` |
| FR-003 | MLB players/QS | 24h | Force bypass; stored rows remain | `sync.go:719-870` |
| FR-004 | MLB stats/spring/rolling | 1h | Force bypass; season pipeline manifest independently gates history | `sync.go:538-610,767-850,972-1018` |
| FR-005 | MLB team rosters | 24h | Replace only after successful fetch | `sync.go:876-910`, roster sync |
| FR-006 | Schedule | 5m peak, 1h off-peak | Force bypass; store/snapshot fallback | `sync.go:914-943` |
| FR-007 | Savant/FanGraphs | 24h | Item plus per-player pipeline `v1`; retain successes | `sync.go:1165-1300` |
| FR-008 | FG closers/FantasyPros | 12h | Force bypass; per-row state | `sync.go:1501-1585,1716-1785` |
| FR-009 | Projections | 24h | Blend always eligible; source rows versioned | `sync.go:1309-1470` |
| FR-010 | Disk caches | Yahoo/MLB 60s; RotoWire 2m; FG cFIP memory 24h | Miss fetches live | provider/cache sources |
| FR-011 | Command snapshots | No store TTL rejection | Producer refreshes; failure marks stale | snapshots and ledger |

RotoWire confirmed daily lineups are fetched lazily by roster display, cached on disk for two minutes, and overlaid ahead of MLBAM lineup/probable-pitcher data. Fetch, parse, or match failure leaves the MLBAM result unchanged. Only confirmed batting-order sides with at least seven hitters resolved to durable league player identities replace MLBAM lineup order.

## Reconciliation

| ID | Authoritative set | Replacement and partial protection | Evidence |
|---|---|---|---|
| REC-001 | Yahoo league slots | Validate full set, then delete league scope and insert transactionally | `player.go:342-413` and tests |
| REC-002 | One Yahoo team | Delete/insert in transaction; rollback on failure | `player.go:317-340` |
| REC-003 | One MLB roster | Delete/insert after successful fetch | `team_roster.go:205-250` |
| REC-004 | Schedule date | Delete/insert after successful fetch | `schedule.go:14-36` |
| REC-005 | Supplied odds games | Delete only listed PKs; empty PK set is non-destructive | `odds.go:60-116` |
| REC-006 | Categories/positions | Upsert supplied keys; stale absent keys are not deleted | `league.go:65-115`; deletion gap |
| REC-007 | Closers | Clear and apply authoritative sets in one transaction | `closer_override.go` |
| REC-008 | Command dataset | Replace only after complete marshal; failure retains payload | `snapshots.go` |

## Failure Dispositions

| Boundary | Malformed/empty/partial | Unavailable/stale | Auth/circuit | Corruption |
|---|---|---|---|---|
| Yahoo | Parser error; operation empty rules; invalid roster set rejected | Error with cache/snapshot/store fallback | OAuth and persisted circuit required | Cache miss; DB error |
| MLB | Decode error; selected empty endpoints valid; seasons partial | Error with cache/snapshot/store fallback | Not applicable | Cache miss; DB error |
| Savant | CSV error; empty subfeed warns; successful subfeeds retained | Warning/error; stored rows remain | Not applicable | No persistent provider cache |
| FanGraphs | JSON/HTML error; empty writes none | Error; stored values remain | Not applicable | Memory cache not serialized |
| FantasyPros | Script error; empty writes none; one-document partial N/A | Error; stored ECR remains | Not applicable | No cache |
| ESPN | JSON error; missing odds/per-game failure degrades | Scoreboard error aborts; prior odds remain | Not applicable | No cache |
| OddsShark | JSON error; empty slate valid; one-response partial N/A | Error; stale snapshot | Not applicable | Bad snapshot decode rejects fallback |
| RotoWire | HTML error; empty lineup valid; fields may be absent | Error; fresh disk cache only | Not applicable | Bad cache is miss |
| Disk cache | Bad/missing/zero-time is miss; partial N/A | Fetch error; expired miss | Not applicable | Read/parse is miss |
| SQLite | Scan errors propagate or documented zero; transactions rollback | Open/write errors; freshness triggers sync | Not applicable | Unreadable schema version propagates |

## Cross-Workstream Evidence

| Source | Role | Manifest owner |
|---|---|---|
| `<skout-repo>/cmd/skout/sync.go` | Sequencing, TTL, write routing, manifests | CLI and operations |
| `<skout-repo>/cmd/skout/mlb_roster_sync.go` | Roster replacement/identity | CLI and operations |
| `<skout-repo>/cmd/skout/match.go` | Yahoo/MLB snapshots, ESPN odds | CLI and operations |
| `<skout-repo>/cmd/skout/sp.go` | Schedule/OddsShark snapshots | CLI and operations |
| `<skout-repo>/cmd/skout/team.go` | Standings/roster writes | CLI and operations |
| `<skout-repo>/cmd/skout/teams_totals.go` | Standings fallback | CLI and operations |
| `<skout-repo>/cmd/skout/roster_totals.go` | Scoreboard/supplement snapshots | CLI and operations |
| `<skout-repo>/cmd/skout/playerpool.go` | Game-note/log snapshots | CLI and operations |
| `<skout-repo>/cmd/skout/playercard_gamelog.go` | Card schedule/boxscore snapshots | CLI and operations |

## Source Coverage

| Source | Capability | Contract/disposition |
|---|---|---|
| `<skout-repo>/docs/api-espn.md` | PROV-ESPN | Markdown; mapped to PROV-ESPN contracts and ledgers |
| `<skout-repo>/docs/api-fangraphs.md` | PROV-FANGRAPHS | Markdown; mapped to PROV-FANGRAPHS contracts and ledgers |
| `<skout-repo>/docs/api-fantasypros.md` | PROV-FANTASYPROS | Markdown; mapped to PROV-FANTASYPROS contracts and ledgers |
| `<skout-repo>/docs/api-mlbam.md` | PROV-MLB | Markdown; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/docs/api-rotowire.md` | PROV-ROTOWIRE | Markdown; mapped to PROV-ROTOWIRE contracts and ledgers |
| `<skout-repo>/docs/api-savant.md` | PROV-SAVANT | Markdown; mapped to PROV-SAVANT contracts and ledgers |
| `<skout-repo>/docs/api-yahoo.md` | PROV-YAHOO | Markdown; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/docs/dev-canonical-identity.md` | DATA-STORE | Markdown; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/docs/yahoo-api-access-403.md` | PROV-YAHOO | Markdown; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/go.mod` | META-DEPS | Go module; mapped to META-DEPS contracts and ledgers |
| `<skout-repo>/internal/cache/disk.go` | DATA-CACHE | Go production; mapped to DATA-CACHE contracts and ledgers |
| `<skout-repo>/internal/cache/disk_test.go` | DATA-CACHE | Go test; mapped to DATA-CACHE contracts and ledgers |
| `<skout-repo>/internal/espnapi/client.go` | PROV-ESPN | Go production; mapped to PROV-ESPN contracts and ledgers |
| `<skout-repo>/internal/espnapi/client_test.go` | PROV-ESPN | Go test; mapped to PROV-ESPN contracts and ledgers |
| `<skout-repo>/internal/fangraphs/client.go` | PROV-FANGRAPHS | Go production; mapped to PROV-FANGRAPHS contracts and ledgers |
| `<skout-repo>/internal/fangraphs/client_test.go` | PROV-FANGRAPHS | Go test; mapped to PROV-FANGRAPHS contracts and ledgers |
| `<skout-repo>/internal/fangraphs/closer_chart.go` | PROV-FANGRAPHS | Go production; mapped to PROV-FANGRAPHS contracts and ledgers |
| `<skout-repo>/internal/fangraphs/closer_chart_test.go` | PROV-FANGRAPHS | Go test; mapped to PROV-FANGRAPHS contracts and ledgers |
| `<skout-repo>/internal/fangraphs/crosswalk.go` | PROV-FANGRAPHS | Go production; mapped to PROV-FANGRAPHS contracts and ledgers |
| `<skout-repo>/internal/fangraphs/projections.go` | PROV-FANGRAPHS | Go production; mapped to PROV-FANGRAPHS contracts and ledgers |
| `<skout-repo>/internal/fantasypros/client.go` | PROV-FANTASYPROS | Go production; mapped to PROV-FANTASYPROS contracts and ledgers |
| `<skout-repo>/internal/fantasypros/client_test.go` | PROV-FANTASYPROS | Go test; mapped to PROV-FANTASYPROS contracts and ledgers |
| `<skout-repo>/internal/mlb/active_roster.go` | PROV-MLB | Go production; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/active_roster_test.go` | PROV-MLB | Go test; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/client.go` | PROV-MLB | Go production; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/client_test.go` | PROV-MLB | Go test; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/enrich.go` | PROV-MLB | Go production; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/enrich_test.go` | PROV-MLB | Go test; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/models.go` | PROV-MLB | Go production; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/people.go` | PROV-MLB | Go production; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/people_test.go` | PROV-MLB | Go test; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/standings.go` | PROV-MLB | Go production; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/teams.go` | PROV-MLB | Go production; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/mlb/teams_test.go` | PROV-MLB | Go test; mapped to PROV-MLB contracts and ledgers |
| `<skout-repo>/internal/oddsshark/client.go` | PROV-ODDS | Go production; mapped to PROV-ODDS contracts and ledgers |
| `<skout-repo>/internal/oddsshark/client_test.go` | PROV-ODDS | Go test; mapped to PROV-ODDS contracts and ledgers |
| `<skout-repo>/internal/rotowire/client.go` | PROV-ROTOWIRE | Go production; mapped to PROV-ROTOWIRE contracts and ledgers |
| `<skout-repo>/internal/rotowire/match.go` | PROV-ROTOWIRE | Go production; mapped to PROV-ROTOWIRE contracts and ledgers |
| `<skout-repo>/internal/rotowire/rotowire_test.go` | PROV-ROTOWIRE | Go test; mapped to PROV-ROTOWIRE contracts and ledgers |
| `<skout-repo>/internal/rotowire/teams.go` | PROV-ROTOWIRE | Go production; mapped to PROV-ROTOWIRE contracts and ledgers |
| `<skout-repo>/internal/savant/client.go` | PROV-SAVANT | Go production; mapped to PROV-SAVANT contracts and ledgers |
| `<skout-repo>/internal/savant/client_test.go` | PROV-SAVANT | Go test; mapped to PROV-SAVANT contracts and ledgers |
| `<skout-repo>/internal/savant/enrich.go` | PROV-SAVANT | Go production; mapped to PROV-SAVANT contracts and ledgers |
| `<skout-repo>/internal/savant/models.go` | PROV-SAVANT | Go production; mapped to PROV-SAVANT contracts and ledgers |
| `<skout-repo>/internal/store/closer_override.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/closer_override_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/ecr_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/fip.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/fip_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/fold.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/fold_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/identity.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/identity_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/league.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/manifest.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/odds.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/odds_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/player.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/player_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/projection.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/projection_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/schedule.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/schedule_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/schema.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/snapshots.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/snapshots_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/statcast.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/store.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/store_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/syncrun.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/team.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/team_roster.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/team_roster_test.go` | DATA-STORE | Go test; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/testutil.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/store/transaction.go` | DATA-STORE | Go production; mapped to DATA-STORE contracts and ledgers |
| `<skout-repo>/internal/yahoo/auth.go` | PROV-YAHOO | Go production; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/internal/yahoo/auth_test.go` | PROV-YAHOO | Go test; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/internal/yahoo/client.go` | PROV-YAHOO | Go production; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/internal/yahoo/client_test.go` | PROV-YAHOO | Go test; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/internal/yahoo/models.go` | PROV-YAHOO | Go production; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/internal/yahoo/parse.go` | PROV-YAHOO | Go production; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/internal/yahoo/parse_matchup.go` | PROV-YAHOO | Go production; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/internal/yahoo/parse_matchup_test.go` | PROV-YAHOO | Go test; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/internal/yahoo/parse_test.go` | PROV-YAHOO | Go test; mapped to PROV-YAHOO contracts and ledgers |
| `<skout-repo>/internal/yahoo/sync.go` | PROV-YAHOO | Go production; mapped to PROV-YAHOO contracts and ledgers |

All 85 sources retain their original capability and workstream ownership.

## Design Recommendations

| ID | Recommendation | Evidence | Compatibility | Tradeoff | Capabilities | Status |
|---|---|---|---|---|---|---|
| DR-PS-001 | Separate transport, parsing, normalization and persistence | Provider/operation ledgers | Preserve behavior | More interfaces; isolates drift | Providers | Recommended |
| DR-PS-002 | Use explicit versioned migrations and an outer transaction where SQLite permits | Store migration, DO-004 | Preserve v36 semantics/reachable upgrades | More migration code | DATA-STORE | Recommended |
| DR-PS-003 | Model completeness, freshness and reconciliation as typed policies | Freshness/reconciliation ledgers | Preserve TTL/force/partial protection | More policy types | DATA-STORE/providers | Recommended |
| DR-PS-004 | Require captured JSON/CSV/HTML/script fixtures | Scrapers, DO-005 | Preserve accepted shapes/failures | Fixtures age | Scraping providers | Recommended |
| DR-PS-005 | Keep provider-specific auth, headers, batching and errors behind adapters | Yahoo, OddsShark, MLB people | Preserve protocol differences | Less universal sharing | Providers | Recommended |

The b9 persistence and acquisition foundation uses pinned `rusqlite =0.40.1` with bundled SQLite, `serde =1.0.229`, `serde_json =1.0.151`, `reqwest =0.13.4` with blocking Rustls, `dirs =6.0.0`, and `sha2 =0.11.0`. It retains b9 schema version four without the predecessor's historical migration mechanics.

## Existing-State Compatibility

The MLB utility integration uses schema version two for MLB identities, 40-man roster rows, season statistics, and league-scoped free agents. Complete team-directory, standings, totals, slate, and future-odds inputs use validated versioned snapshots; team rosters replace and fall back independently. The OddsShark subset is implemented through injected bounded transport with its required Referer and optional-enrichment degradation.

| Option | Result | Tradeoff | Status |
|---|---|---|---|
| Reuse Skout database | Open v36 `~/.config/skout/skout.db` | Immediate continuity; schema/concurrency coupling | Rejected for PS-1 |
| Migrate to b9 database | Guarded one-time compatible-state transfer | Explicit read-only boundary; transaction and rollback burden | Selected for fantasy context needed by MLB utilities |
| Isolated storage | Rebuild from providers at `$HOME/.config/b9/b9.db` | Simple; cold history/cache | Selected |

Observable path/state/freshness/data semantics remain distinct from exact Go SQL mechanics. b9 owns isolated schema-version-four storage, typed state and snapshots, bounded cache and transport boundaries, public Yahoo acquisition, numeric-key fantasy normalization, complete-league and free-agent persistence, foreground-only synchronization, ambiguity-safe MLB identity reconciliation, ESPN moneyline context, bounded MLB acquisition, and bounded on-demand RotoWire lineup acquisition. When b9 has no Yahoo-linked state, a guarded compatibility adapter reads the legacy database once and transactionally imports compatible fantasy context; b9 remains the sole owner of subsequent writes. Automated Savant, FanGraphs, and FantasyPros HTML acquisition remains rejected under the recorded policy evidence. Yahoo transaction history remains deferred.

## Verification

### Deterministic pre-release tests

- Generate adapter tests from operation/path ledgers with mock transports.
- Assert URLs, headers, auth, timeouts, batching, parsing, normalization and degradation.
- Reproduce cache hit/miss/expiry/corruption/permissions/pruning in temporary directories.
- Test fresh b9 version one, versionless/corrupt/future states, WAL, busy timeout, permissions and migration failure fixtures.
- Assert every store API, transaction, snapshot, freshness and reconciliation contract.
- Convert each explicit gap to a named test or later-slice exclusion.

### Live checks

- Check reachability and representative shapes only after fixtures pass.
- Use non-production Yahoo credentials only with Director authorization.
- Treat unofficial provider results as post-fixture verification, not pre-release truth.
- Observe rate behavior with one representative request and no load test.

## Rust Slices

| Slice | Prerequisite | Delivery | Tests | Exclusions |
|---|---|---|---|---|
| PS-1 Persistence core | Ratified inventory; isolated state selected | Open/schema/migrations/base APIs implemented | Schema/migration/transaction implemented | Providers/snapshots |
| PS-2 Freshness/snapshots | PS-1 | Typed item/row/season/run state and validated durable snapshots implemented | Injected clock/version/stale/completeness implemented | Transports |
| PS-3 Cache/transport | Ratified inventory | Bounded atomic disk cache and validating injectable HTTP executor implemented | Filesystem/request/redirect/limit contracts implemented | Parsing/auth adapters |
| PS-4 JSON providers | PS-2/PS-3 | Yahoo authentication and fantasy data, ESPN current odds, and bounded MLB metadata/live-game/statistics acquisition implemented | Yahoo/ESPN/MLB fixtures, transport doubles, secure boundaries, parsing variants, cache boundaries, concurrency, and store rollback implemented | Other MLB/live checks |
| PS-5 Scrapers | PS-2/PS-3 | OddsShark and bounded on-demand RotoWire lineups implemented; Savant, FanGraphs, and FantasyPros HTML rejected by the reviewed provider policy | OddsShark and RotoWire fixture coverage; official policy evidence for rejected acquisition | No rejected-provider live checks |
| PS-6 Integration | PS-4/PS-5 | Complete retained command shell, foreground Yahoo sync, free agents, reconciliation, matchup snapshots, stale fallback, display, and operations implemented | Fixture DB, orchestration, parity, fallback, and failure paths | Yahoo transactions, retired daemon, retired advisory subsystem, and rejected providers |

PS-1 through PS-6 are implemented for retained providers and commands after their acceptance tests pass. Live-provider gates and explicit excluded gaps remain tracked separately from deterministic implementation.

PS-2 uses an injected thread-safe clock, explicit source identities, typed statuses, pipeline-version gates, strict stored-state decoding, deterministic run-count JSON, and atomic snapshot replacement. It returns contextual storage and JSON failures instead of silently treating them as missing state. Normal freshness state does not infer sources from item names or fall back to a predecessor database; the later MLB utility compatibility bootstrap uses `sync_log` only as its one-time completion and ownership-freshness record. Provider TTL constants, command payload types, fallback selection, transport, and reconciliation remain deferred.

PS-3 keeps Skout's durable short-lived cache and synchronous request capabilities while replacing arbitrary paths, direct overwrites, process-global locking, silent cache-write failures, implicit clocks, unbounded bodies, and provider-owned clients. b9 uses versioned bounded cache framing, hashed logical keys, atomic last-writer-wins replacement, explicit pruning, strict path handling, a validating `HttpClient`, and an injected executor with no retries, bounded redirects, total timeouts, body limits, and sensitive-header redaction. Provider TTLs, cache keys, authentication, request construction, parsing, and error interpretation remain adapter-owned. Each provider's `docs/api-*.md` file migrates with its PS-4 or PS-5 adapter so it records these improved shared mechanics alongside the owning endpoint contract.

The ESPN PS-4 sub-slice accepts one supplied day, requests that UTC calendar day and the next, preserves first-seen event order, deduplicates identifiers, decodes only the observed scoreboard and first odds-item fields, and retains valid games when individual odds requests fail. The adapter owns endpoints, limits, parsing, and structured partial failures while shared transport owns validation and execution. Typed store APIs atomically replace only affected moneyline rows and preserve unrelated games and markets. ESPN-to-MLB game mapping, the 30-minute freshness gate, stale fallback, snapshots, warnings, and command use remain deferred to PS-6.

The MLB PS-4 sub-slices acquire typed season boundaries, hydrated schedules, boxscores, standings, 40-man rosters, people identities, single-player and bulk statistics, and hitter and pitcher game logs. They replace direct HTTP with injected validated transport, chunk deduplicated identities into stable batches of 100, preserve provider collection order and native stat strings, and expose cache disposition and write degradation explicitly. They retain Skout's 60-second schedule and date-range-stat reuse while replacing URL-derived keys, silent corrupt payload use, and hidden write failures with a bounded b9-owned cache contract. Quality-start acquisition uses deterministic five-request batches, retains successful pitcher counts, and replaces Skout's scheduling-dependent last error with ordered typed issues. All-player enumeration, injuries, player search and hands compatibility, team-games-played acquisition, normalized persistence, reconciliation, snapshots, fallback selection, and command integration remain deferred.

The Yahoo PS-4 implementation preserves public-client PKCE, secure operating-system credentials, refresh-token continuity, bearer requests, four 429 retries, and terminal 401/403 behavior. It adds bounded numeric-key fantasy parsing, array-or-object normalization, complete standings, roster, and free-agent validation, version-two normalized persistence, shared synchronization accounting, weekly snapshots, and stale fallback. It replaces fixed state, state-less bare-code acceptance, ambiguous credential failures, refresh races, unchecked query concatenation, unbounded token diagnostics, PID signaling, and direct provider clients with random one-use state, strict callback validation, typed credential outcomes, single-flight refresh, safe URL construction, redacted errors, private control, and injected boundaries. Yahoo transaction history remains deferred.
