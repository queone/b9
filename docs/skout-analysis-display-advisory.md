# Skout Analysis, Display, and Advisory Inventory

## Source Baseline

- Use source repository `<skout-repo>` at commit `cf65984024bd10a0a41faa69b8aecd3894052c31`.
- Require an exact HEAD and clean working tree.
- Exclude secrets, credentials, database contents, generated files, ignored files, and live interactions.
- Treat executable behavior and tests as authoritative over documentation.

## Ownership Coverage

| Capability | Assigned sources | Detailed contracts | Disposition |
|---|---:|---|---|
| CORE-DOMAIN | 4 | DOM-LEAGUE through DOM-ROSTER | Required |
| AN-SIGNALS | 29 | AN-BIRTH through AN-WIRE, FORM-001 through FORM-018 | Required |
| DISP-TABLES | 26 | DISP-ADV through DISP-TEAM | Required |
| ADV-LLM | 27 | ADV-ALERT through ADV-VALIDATE | Required |

Map each of the 86 assigned manifest sources exactly once in [Source Coverage](#source-coverage). Keep command evidence separate because its capability ownership remains in the CLI inventory.

## Domain Contracts

| ID | Type and fields | Semantics and invariants | Producers and consumers | Evidence | Assertion |
|---|---|---|---|---|---|
| DOM-LEAGUE | `ScoringType`; all `League` fields | Preserve three scoring values and provider ordering for categories and roster positions | Yahoo/store produce; commands, analysis, and advisory consume | `<skout-repo>/internal/domain/league.go` | Fixture round trip and enum cases |
| DOM-MATCHUP | All `Matchup` and `MatchupTeam` fields | Preserve ISO dates and phase status; make `Score()` wins; sum completed, live, and remaining in `TotalGames()`; retain raw stat strings | Yahoo/store produce; analysis, display, and advisory consume | `<skout-repo>/internal/domain/matchup.go` | Method, status, date, and raw-stat fixtures |
| DOM-WEEK | All `PlayerWeekStats` and `RosterWeekStats` fields | Partition B/P and preserve roster order; retain AVG, ERA, WHIP, IP, and H/AB strings | Yahoo/store and command overlays produce; display and advisory consume | `<skout-repo>/internal/domain/matchup.go` | Partition, ordering, and formatting fixtures |
| DOM-POS | `Position` values C, 1B, 2B, 3B, SS, OF, SP, RP, Util, BN, IL | Use exact strings for eligibility, sorting, compression, and lineup validation | Providers/store produce; all three workstreams consume | `<skout-repo>/internal/domain/player.go` | Exhaustive value and eligibility fixtures |
| DOM-BAT | All `BattingStats` fields | Preserve season counts/rates; use K and BB for PQS rates | MLB/store produce; analysis and display consume | `<skout-repo>/internal/domain/player.go` | Numeric fixture with zero denominators |
| DOM-PIT | All `PitchingStats` fields | Treat IP as decimal innings in analysis; distinguish SO fantasy total from K/BF rate inputs | MLB/store produce; analysis and display consume | `<skout-repo>/internal/domain/player.go` | Numeric fixture with zero denominators |
| DOM-STATCAST | All `StatcastData` fields | Keep raw and empirical-Bayes blended values distinct; use PA and BBE as gates | Store and enrichment produce; PQS/cards consume | `<skout-repo>/internal/domain/player.go` | Raw/blended separation fixture |
| DOM-GAMELOG | All `GameLogRow` fields | Use ten calendar days for hitters and ten appearances for pitchers; distinguish batting-order zero by result | Player-card commands produce; card renderer consumes | `<skout-repo>/internal/domain/player.go` | Hitter blank-day and pitcher appearance fixtures |
| DOM-PLAYER | All `Player` fields | Make `IsBatter()` depend on non-nil `Batting` and `IsPitcher()` depend on non-nil `Pitching`; use `PrimaryType` separately for analysis classification; use exact eligible-position membership; report unknown age without a complete date | Providers/store/enrichment produce; all three workstreams consume | `<skout-repo>/internal/domain/player.go`, `<skout-repo>/internal/analysis/pqs.go` | Identity, pointer-role, analysis-classification, eligibility, and age fixtures |
| DOM-ROSTER | All `Roster` fields | Exclude BN/IL from active; preserve order in role filters; compare names case-insensitively | Store produces; analysis, display, and advisory consume | `<skout-repo>/internal/domain/roster.go` | Active/filter/lookup fixtures |

Keep advisory payload and response types under Advisory Contracts. Keep store rows and provider wire types under the providers and storage inventory.

### Domain Field Ledger

- Map `League` fields `LeagueKey`, `Name`, `Season`, `NumTeams`, `ScoringType`, `RosterPositions`, `BattingCategories`, and `PitchingCategories` to DOM-LEAGUE.
- Map `Matchup` fields `Week`, `WeekStart`, `WeekEnd`, `Status`, and `Teams` and `MatchupTeam` fields `TeamKey`, `TeamID`, `Name`, `IsCurrentLogin`, `Stats`, `Wins`, `Losses`, `Ties`, `CompletedGames`, `LiveGames`, and `RemainingGames` to DOM-MATCHUP.
- Map `PlayerWeekStats` fields `YahooPlayerID`, `Name`, `Team`, `PositionType`, `SlotPosition`, `EligiblePositions`, `InjuryStatus`, `HAB`, `R`, `HR`, `RBI`, `SB`, `AVG`, `IP`, `W`, `SV`, `K`, `ERA`, and `WHIP` and all four `RosterWeekStats` fields to DOM-WEEK.
- Map `BattingStats` fields `PA`, `AVG`, `OBP`, `SLG`, `OPS`, `HR`, `RBI`, `R`, `SB`, `K`, and `BB` to DOM-BAT.
- Map `PitchingStats` fields `G`, `GS`, `IP`, `ERA`, `WHIP`, `SO`, `K9`, `BB9`, `FIP`, `XFIP`, `Wins`, `Saves`, `Holds`, `QS`, `K`, `BB`, and `BF` to DOM-PIT.
- Map `StatcastData` batting fields `ExitVeloAvg`, `BarrelPct`, `HardHitPct`, `XBA`, `XSLG`, `XWOBA`, `LaunchAngleAvg`, `SweetSpotPct`, `SprintSpeed`, `FBPct`, and `HRFBPct`; pitching fields `FastballVelo`, `SpinRate`, `WhiffPct`, `ChasePct`, `HardHitPctPit`, `GBPct`, `FBPctPit`, `XERA`, and `XFIP`; and denominator fields `PA` and `BBE` to DOM-STATCAST.
- Map `GameLogRow` fields `Date`, `OpponentAbbr`, `IsHome`, `TeamResult`, `BattingOrder`, `HAB`, `R`, `HR`, `RBI`, `SB`, `AVG`, `IPDecimal`, `W`, `SV`, `K`, `ERA`, and `WHIP` to DOM-GAMELOG.
- Map `Player` identity fields `ID`, `YahooPlayerKey`, `MLBPlayerID`, `Name`, `Team`, `Positions`, `BatSide`, `PitchHand`, `BirthDate`, and `JerseyNumber`; roster fields `RosterPosition`, `InjuryStatus`, `InjuryNote`, `MLBAMInjuryNote`, `OwnershipPct`, `OwnershipDelta`, `PctStarted`, and `YahooRank`; stat fields `Batting`, `Pitching`, `StatcastRaw`, and `StatcastBlended`; classification field `PrimaryType`; computed fields `PQS`, `IsCloser`, `SpringOnly`, `ProjectedProduction`, and `IsRecentCallup`; third-party fields `ECR`, `FanGraphsWar`, and `WRCP`; and `Owner` to DOM-PLAYER.
- Map `Roster` fields `LeagueKey`, `Season`, `TeamKey`, `TeamName`, and `Players` to DOM-ROSTER.

## Analysis Candidate Ledger

| ID | Source | Operations | Helpers and constants | Classification and test status |
|---|---|---|---|---|
| AN-BIRTH | `internal/analysis/birthdates.go` | `EnsureBirthDates` | `BirthDateStore`, `BirthDateFetcher` | Operation plus ports; tested |
| AN-BLEND | `internal/analysis/blend.go` | `BlendPlayerStats` | `batterOf`, `pitcherOf`, `blendBatting`, `blendPitching`, `safeB`, `safeP`, `blendFloat`, `blendInt` | Operation/helpers; tested |
| AN-SORT | `internal/analysis/browse_sort.go` | `ValidateSortColumn`, `SortBrowsePlayers` | `BrowseSortColumn`, registries, `sortValue`, `stringSortKey` | Operations/helpers; tested |
| AN-DROP | `internal/analysis/drop.go` | `FindDropCandidates` | `DropCandidate`, `sharesPosition`, `dropReason`, `hasReplacement`, `hasUpgrade` | Operation/helpers; test gap |
| AN-ROLE | `internal/analysis/pitcher_role.go` | `ClassifyPitcherRole` | `PitcherRole`, values, `PitcherRoleGameLog` | Operation/contracts; tested |
| AN-PQS | `internal/analysis/pqs.go` | `ComputePQS` | signal registries, role checks, talent/emphasis/context/scarcity/population helpers and constants | Operation/helpers/constants; tested |
| AN-ROSTER | `internal/analysis/roster.go` | `EvaluateRoster` | `RosterReport`, `top`, `identifyWeakSpots` | Operation/helpers; test gap |
| AN-WEIGHTS | `internal/analysis/stat_weights.go` | `ComputeStatWeights`, `OpportunityDampen` | `StatWeights`, thresholds, `redistributeWeights` | Operations/helpers/constants; tested |
| AN-STATCAST | `internal/analysis/statcast_blend.go` | `BlendStatcastBatting`, `BlendStatcastPitching` | gate maps, `BlendedResult`, `fbVal` | Operations/helpers/constants; test gap |
| AN-TRADE | `internal/analysis/trade.go` | `EvaluateTrade` | `TradeReport`, `findByName` | Operation/helper; test gap |
| AN-WAIVER | `internal/analysis/waiver.go` | `FilterWaiverFAs`, `BuildHitterFloors`, `ResolveHitterFloor`, `ApplyRecentActivityBypass` | position/activity constants | Operations/constants; tested |
| AN-WINDOW | `internal/analysis/window_proj.go` | next-window and four aggregate-window operations | window/base/stat types, four base helpers, `roundInt` | Operations/helpers; tested |
| AN-WIRE | `internal/analysis/wire_threshold.go` | all threshold strategy methods | interfaces/types, percentile helpers | Operations/helpers; tested |

Classify every production function in `internal/analysis`; exclude none. Classify interface methods and threshold implementations as operations. Classify anonymous signal extractors and registry literals as constant-only executable contracts.

### Analysis Function Enumeration

- Classify `EnsureBirthDates`; `BlendPlayerStats`, `batterOf`, `pitcherOf`, `blendBatting`, `blendPitching`, `safeB`, `safeP`, `blendFloat`, `blendInt`; `ValidateSortColumn`, `sortValue`, `stringSortKey`, `SortBrowsePlayers`; `sharesPosition`, `FindDropCandidates`, `dropReason`, `hasReplacement`, `hasUpgrade`; and `ClassifyPitcherRole` under their AN-* rows.
- Classify `isBatter`, `isPitcher`, `clampZ`, `ComputePQS`, `computeTalentPQS`, `applyEmphasis`, `computeEmphasizedWeights`, `applyContext`, `computePositionalScarcity`, `positionalScarcityBonus`, `sortDescending`, `popMean`, and `popStddev` under AN-PQS.
- Classify `EvaluateRoster`, `top`, `identifyWeakSpots`; `ComputeStatWeights`, `OpportunityDampen`, `redistributeWeights`; `BlendStatcastBatting`, `BlendStatcastPitching`, `fbVal`; `EvaluateTrade`, `findByName`; and all four named AN-WAIVER operations under their AN-* rows.
- Classify `NextHitterWindow`, `NextPitcherWindow`, `projectedHitterBase`, `recentHitterBase`, `projectedPitcherBase`, `recentPitcherBase`, `roundInt`, `AggregateLastHitterWindow`, `AggregateLastPitcherWindow`, `AggregateRecentHitterWindow`, and `AggregateRecentPitcherWindow` under AN-WINDOW.
- Classify all nine threshold methods plus `percentileFloats` and `percentile` under AN-WIRE.

## Semantic Formula Ledger

| ID | Operation | Formula, threshold, ordering, and missing-data contract | Evidence | Assertion |
|---|---|---|---|---|
| FORM-001 | AN-WEIGHTS | Use weights 0/.90/.10 preseason; .15/.80/.05 through game 7; .25/.75/0 through 14; .50/.50/0 through 27; 1/0/0 afterward | `stat_weights.go` | Exact table |
| FORM-002 | AN-WEIGHTS | Multiply scheduled current by `min(opportunity/threshold,1)` using PA 150 or IP 40; move unused current to prior; redistribute unavailable sources proportionally | `stat_weights.go` | Tolerance `1e-9` |
| FORM-003 | AN-BLEND | Blend supported fields by source weights; round integer fields; take identity from first available current, prior, spring record | `blend.go` | Field-complete fixture |
| FORM-004 | AN-PQS | Standardize each signal by population, clamp z to ±2, apply `min(1,sample/threshold)`, normalize weights, and apply context | `pqs.go` | Existing PQS fixtures |
| FORM-005 | AN-PQS | Use hitter weights xwOBA .30, K% .15 inverse, BB% .10, sprint .20, FB% .10, HR/FB .15; preserve pitcher registries exactly | `pqs.go` | Registry snapshot |
| FORM-006 | AN-PQS | Apply category emphasis; cap positional scarcity at .15 using depth 12; preserve role gating and spring-only exclusion | `pqs.go` | Existing emphasis/scarcity tests |
| FORM-007 | AN-ROLE | Prefer probable status, then recent use, then season GS/G; preserve ambiguous band and default | `pitcher_role.go` | Existing role tests |
| FORM-008 | AN-SORT | Apply configured direction, sort missing numerics last, and break ties by normalized name | `browse_sort.go` | Existing sort tests |
| FORM-009 | AN-WIRE | Interpolate sorted percentile; use .60 for waiver role and median for hitter cohort; scale alternate thresholds by games | `wire_threshold.go`, `playerpool.go` | Threshold fixtures |
| FORM-010 | AN-WINDOW | Convert projection/recent rates to PA/IP windows, blend available bases, round counts, and bound recent windows by threshold/date | `window_proj.go` | Existing window tests |
| FORM-011 | AN-STATCAST | Blend current/prior metrics toward means with metric-specific PA/BBE gates and fastball fallback | `statcast_blend.go` | Add metric matrix |
| FORM-012 | ADV-CATEGORY | Parse H/AB; invert lower-better deltas; use tie tolerance .0001, lead threshold opponent games×.8, and lost threshold own games×1 | `advisory/category.go` | Existing category tests |
| FORM-013 | ADV-PP | Standardize role metrics, damp rates for low PA/IP, scale, and clamp PP to 0–100 | `advisory/pp.go` | Existing PP tests |
| FORM-014 | DISP-ODDS | Convert positive line with `100/(price+100)`, negative with `-price/(-price+100)`, then normalize both sides | `display/odds.go` | Existing odds tests |
| FORM-015 | DISP-TOTALS | Compute AVG H/AB, OBP `(H+BB+HBP)/(AB+BB+HBP)`, SLG TB/AB, OPS OBP+SLG; preserve zero guards | display totals files | Existing/new zero fixtures |
| FORM-016 | CMD-MATCH | Select day/week range, overlay live/day stats, backfill blank bench lines, preserve snapshot fallback, and zero-guard AVG | `cmd/skout/match.go` | Match/snapshot fixtures |
| FORM-017 | CMD-RT | Tally weekly W/L/T, supplement totals, sort rank, and resolve week/date offsets | `cmd/skout/roster_totals.go` | Totals fixtures |
| FORM-018 | CMD-CARD | Build ten-day hitter logs, appearance pitcher logs, schedule/result cells, batting order, and next start | `cmd/skout/playercard_gamelog.go` | Card-log fixtures |

Treat formulas in the seven assigned analysis documents as explanatory evidence only. Resolve differences in favor of executable code. Record no executable conflict at this baseline.

## Cross-Workstream Candidate Ledger

| ID | Source | Computation candidates | Routing or rendering candidates | Disposition |
|---|---|---|---|---|
| CMD-MATCH | `cmd/skout/match.go` | snapshot age, range, blank/backfill, overlays, AVG, odds freshness | local/stale fallback, display/advisory orchestration, timing | Evidence; retain CLI-MATCH |
| CMD-ROSTER | `cmd/skout/roster.go` | spring presence, team matching | refresh, resolution, render | Evidence; retain CLI-ROSTER |
| CMD-RT | `cmd/skout/roster_totals.go` | standings, totals, date/week, rank | live/stale routing | Evidence; retain CLI-TOTALS |
| CMD-TT | `cmd/skout/teams_totals.go` | roster aggregation/count | totals routing | Evidence; retain CLI-TOTALS |
| CMD-POOL | `cmd/skout/playerpool.go` | waiver/activity/role/Statcast/position logic | browse/card routing | Evidence; retain CLI-PLAYER |
| CMD-CARD | `cmd/skout/playercard_gamelog.go` | log, schedule/result, slot, next start | card routing | Evidence; retain CLI-PLAYER |
| CMD-H | `cmd/skout/hitters.go` | None | shared-pool alias | Routing; retain CLI-PLAYER |
| CMD-P | `cmd/skout/pitchers.go` | None | shared-pool alias | Routing; retain CLI-PLAYER |
| CMD-SP | `cmd/skout/sp.go` | slate/ownership/time/future odds | slate routing | Evidence; retain CLI-SP |
| CMD-TEAM | `cmd/skout/team.go` | cache age/abbreviations | fetch/render routing | Evidence; retain CLI-TEAM |
| CMD-WHAT | `cmd/skout/whatis.go` | aliases/Levenshtein | glossary routing | Evidence; retain CLI-GLOSSARY |
| CMD-LM | `cmd/skout/lm.go` | dated-model detection | provider/model/key routing | Evidence; retain ADV-LLM assignment |

Classify every named function in these files under its row. Treat command declarations and `init` as routing, timing helpers as library mechanics, and fetch/store helpers as routing rather than analysis ownership.

### Cross-Workstream Function Enumeration

- Map all 24 named functions and methods in `match.go` from `restoreMatchBoxscoreSnapshot` through `isBlanked` to CMD-MATCH.
- Map `refreshRostersForDisplay`, `hasSpringStats`, `resolveRosterTeamKey`, `matchFantasyTeams`, and `init` to CMD-ROSTER.
- Map all 17 named functions in `roster_totals.go` from `runSeasonRosterTotals` through `init` to CMD-RT, and map both computations plus `init` in `teams_totals.go` to CMD-TT.
- Map all 18 named functions in `playerpool.go` from `enrichPlayerPool` through `idSet` to CMD-POOL, and map all 13 named functions in `playercard_gamelog.go` from `buildHitterGameLogWithStore` through `gameNotStarted` to CMD-CARD.
- Map each `init` in `hitters.go` and `pitchers.go` to CMD-H and CMD-P respectively.
- Map `init`, `runSP`, `buildSPSlateDay`, `slateRows`, `collectPitcherNames`, `pitcherIsFA`, `pitcherIsMine`, `gameTimeLocal`, and `futureOddsFromLines` to CMD-SP.
- Map `loadTeamRoster`, `joinAbbrs`, `compactDuration`, and `init` to CMD-TEAM; map `lookupGlossary`, `containsAlias`, `suggestGlossaryKeys`, `levenshtein`, `min3`, and `init` to CMD-WHAT; and map `init`, `pickProvider`, `pickModel`, `hasEmbeddedDate`, and `readAPIKey` to CMD-LM.

## Display Candidate Ledger

| ID | Source | Surface entries | Output-affecting helpers | Observable contract and assertion |
|---|---|---|---|---|
| DISP-ADV | `display/advisory.go` | breakdown, summary | labels, wrap, paragraphs, W/T/L, flippable | Ordered sections and 80-column prose; tests |
| DISP-GLOSS | `display/glossary.go` | entry/full glossary | grouping/class | Class and token layout; tests |
| DISP-MATCH | `display/matchup.go` | matchup/local/stale/week | all totals, divider, slot, row, odds, name, status, color, pad, group, filter, record helpers | Fixed halves, slot anchors, BN/IL color, ANSI status, odds bars; tests |
| DISP-STAND | `display/mlb_standings.go` | standings | league/header/row/int | AL/NL and PCT/GB/YP/PA layout; tests |
| DISP-ODDS | `display/odds.go` | odds table | row/probability/total/K/name/age | Probabilities, totals, K props, dashes, age; tests |
| DISP-CARD | `display/playercard.go` | hitter/pitcher card | all identity, season, AVG162G, injury, Statcast, rate helpers | Sections, splits, dashes, roles; tests |
| DISP-LOG | `display/playercard_gamelog.go` | card log printers | next-start/row cells | Calendar versus appearance semantics; tests |
| DISP-POS | `display/poscell.go` | None | all compression/closer helpers | Five visible columns and closer marker; tests |
| DISP-RT | `display/roster_totals.go` | totals print/compute | padding/rank/WLT/PCT/GB/rate/emoji/leader | Aggregates/ranks/alignment; tests |
| DISP-SP | `display/sp.go` | SP slate | row/name | Day groups and widths/ownership; tests |
| DISP-TABLE | `display/table.go` | roster/reports/browse | every row/header/name/status/PQS/pad/detail/rate/IP/season/Statcast/injury/position helper | Exact columns, names, nulls, ANSI, rates; tests |
| DISP-TEAM | `display/team_roster.go` | team rosters | every section/split/two-way/row/name/position/owner/float helper | Ordering, suffix, owner tiers, lineup, closer; tests |

Exclude no production display function. Classify table construction and raw rune/emoji detection as library mechanics covered through consumers. Use stdout except command-owned warnings and errors.

### Display Function Enumeration

- Map `advLabel`, `PrintBreakdown`, `PrintSummary`, `wrapProse`, `collectSummaryParagraphs`, `printWTLLine`, and `printFlippable` to DISP-ADV; map all four glossary functions to DISP-GLOSS.
- Map every one of the 56 named functions in `display/matchup.go`, from `PrintMatchup` through `ordinal`, to DISP-MATCH; classify printing/formatting/status helpers as output-affecting and `parseStatFloat`, `computeCatWinners`, `opposingSPID`, `findBattingOrder`, `splitGroups`, `filterAndSort`, and `slotOrd` as operation helpers.
- Map all five standings functions to DISP-STAND; all eight odds functions to DISP-ODDS; all thirteen card functions to DISP-CARD; all five game-log functions to DISP-LOG; and all five position-cell functions to DISP-POS.
- Map all thirteen roster-total functions to DISP-RT and all three probable-pitcher slate functions to DISP-SP.
- Map every one of the 42 named functions in `display/table.go`, from `PrintRosterTable` through `posCell`, to DISP-TABLE; classify `newTable` and `isDoubleWidth` as library mechanics and every other helper as output-affecting or computation supporting a surface.
- Map all twelve team-roster functions to DISP-TEAM; classify `runeLen` as library mechanics and every other helper as output-affecting.

## Advisory Contracts

| ID | Source | Contract | Deterministic evidence | Live disposition |
|---|---|---|---|---|
| ADV-ALERT | `advisory/alerts.go` | Phase, schedule, lineup, and >50% precipitation alerts; skip live/final risk | advisory tests | Not required |
| ADV-CATEGORY | `advisory/category.go` | Ordered category gaps, direction, status, flip text | advisory tests | Not required |
| ADV-CONTEXT | `advisory/context.go` | Assemble all matchup decision context | JSON fixture | Not required |
| ADV-GLOSSARY | `advisory/glossary.go` | Parse/filter/select/format glossary | glossary tests | Not required |
| ADV-KEY | `advisory/keychain.go` | Store/retrieve provider keys without disclosure | fake store | LIVE-KEY |
| ADV-LINEUP | `advisory/lineup.go` | Grounded swaps, SP/RP starts, slot gaps | lineup tests | Not required |
| ADV-CLIENT | `advisory/llm.go` | Provider requests, debug files, fenced/partial JSON, discarded fields, contextual errors | parse/debug/compat tests | LIVE-PROTOCOL |
| ADV-MOVES | `advisory/moves.go` | Position-compatible free-agent/drop candidates helping flippable categories | fixture gap | Not required |
| ADV-MODELS | `advisory/openai_models.go` | Filter/sort/cap/inject/label/recommend models | model tests | LIVE-MODELS |
| ADV-PAYLOAD | `advisory/payload.go` | Credential-free players/stats/status/injuries/moves/strategy payload | payload tests | Not required |
| ADV-PP | `advisory/pp.go`, `pp_compute.go` | Compute 0–100 projection percentile for all players | PP tests | Not required |
| ADV-PROMPT | `advisory/prompt.go` | Separate active/retro JSON prompts with grounding/risk rules | golden gap | Not required |
| ADV-STRATEGY | `advisory/strategy.go` | Defaults, YAML, case-insensitive punts | strategy tests | Not required |
| ADV-TYPES | `advisory/types.go` | All advisor, response, provider, context, candidate, alert, status, strategy types and legacy fallback | JSON/line tests | Not required |
| ADV-VALIDATE | `advisory/validate.go` | Provider-specific validation with recovery context | fake-server gap | LIVE-PROTOCOL |
| ADV-CLI | `cmd/skout/lm.go`, `cmd/skout/match.go` | Preserve opt-in `--advise/-a` and separate configuration | CLI fixtures | LIVE-PROTOCOL |

Keep provider values and request schemas as compatibility surfaces behind provider-specific adapters. Ground every action against supplied candidates. Preserve partially valid responses. Expose discarded diagnostics only through debug behavior. Persist no key or raw live payload in fixtures.

### Advisory Function Enumeration

- Map all functions in `alerts.go`, `category.go`, `context.go`, `glossary.go`, `keychain.go`, and `lineup.go` to ADV-ALERT through ADV-LINEUP according to their source row.
- Map all fifteen functions and methods in `llm.go`, from `NewAdvisor` through `lastIndexOf`, to ADV-CLIENT; classify transport/debug/string helpers as operation helpers.
- Map all four move functions to ADV-MOVES; all seven model functions to ADV-MODELS; all eight payload functions to ADV-PAYLOAD; all eight PP functions across both files to ADV-PP; both prompt functions to ADV-PROMPT; all four strategy functions to ADV-STRATEGY; all five summary helper methods/functions to ADV-TYPES; and `ValidateAPIKey` to ADV-VALIDATE.

## Design Findings

| Decision | Recommendation | Evidence | Compatibility impact | Tradeoff | Capabilities | Status |
|---|---|---|---|---|---|---|
| DESIGN-001 | Use pure functions for formulas, sorting, candidates, validation | Deterministic operations | None | Explicit inputs | AN-SIGNALS, ADV-LLM | Resolved |
| DESIGN-002 | Insert view models before terminal rendering | Display mixes computation/ANSI/printing | Preserve output | Conversion code | DISP-TABLES | Resolved |
| DESIGN-003 | Use provider-neutral advisor plus request adapters | DO-006 provider leakage | Preserve differences | Explicit duplication | ADV-LLM | Resolved |
| DESIGN-004 | Test rows structurally and full surfaces with goldens | Mixed current tests | Preserve output | Controlled ANSI/width | DISP-TABLES | Resolved |
| DESIGN-005 | Separate deterministic parity from qualitative usefulness | External variability | None | Human evaluation non-gating | ADV-LLM | Resolved |

Select no Rust crate here. Preserve behavior, not Go libraries, table mechanics, HTTP-client structure, JSON implementation, prompt concatenation, or keychain library choice.

## Verification Matrix

| ID | Kind | Gating | Environment and credentials | Action and prerequisite | Success/failure | Waiver |
|---|---|---|---|---|---|---|
| TEST-DOM | Deterministic | Yes | Fixtures; none | Run after domain port | All invariants pass; failure blocks | None |
| TEST-AN | Deterministic | Yes | Fixtures; none | Run formula/sort/role/decision tests | Exact/tolerance outputs pass; failure blocks | None |
| TEST-DISP | Deterministic | Yes | Fixed width/locale/color; none | Run structural/goldens after view models | Visible contract passes; failure blocks | None |
| TEST-ADV | Deterministic | Yes | Fixtures/fake servers; fake keys | Run payload/prompt/parse/grounding/failure tests | No invented actions; failure blocks | None |
| LIVE-TERM | Live | Yes | Supported terminal; none | Inspect after TEST-DISP | Alignment/color/truncation match | Director with accepted terminal risk |
| LIVE-MODELS | Live | Yes | Network; configured key | List after model fixtures | Filtering/selection work | Director with accepted availability risk |
| LIVE-KEY | Live | Yes | OS keychain; disposable key | Round trip after fake-store tests | No disclosure | Director with accepted platform risk |
| LIVE-PROTOCOL | Live | Yes | Network; provider keys | One bounded request/provider after TEST-ADV | Parse and grounding pass | Director with accepted provider risk |
| EVAL-USE | Manual | No | Frozen matchups; no secrets | Score factuality/actionability/grounding/clarity 0–2 | Record only; never gates parity | Not applicable |

Require the Director to record rationale and accept residual risk before a waived live gate supports readiness.

## Rust Implementation Slices

| Slice | Prerequisites | Delivered contracts | Tests | Exclusions |
|---|---|---|---|---|
| SLICE-DOM | Ratified inventories | Domain records/invariants/conversion | TEST-DOM | Fetch/persistence |
| SLICE-AN-CORE | Domain/store reads | Weights/blends/PQS/role/sort/threshold/window | TEST-AN core | CLI/rendering |
| SLICE-AN-DECISION | Core/roster reads | Roster/trade/drop/waiver | TEST-AN decision | Transactions |
| SLICE-VIEW | Domain/analysis | Display view models | Structural tests | Terminal crate |
| SLICE-DISP | View models | Exact terminal surfaces/routing | TEST-DISP, LIVE-TERM | Acquisition |
| SLICE-ADV-DET | Domain/decisions | Context/gaps/alerts/candidates/PP/payload/prompt/parse/grounding | TEST-ADV | Network/keychain |
| SLICE-ADV-ADAPTERS | Deterministic advisory | Adapters/models/keys/debug | Fake-server and live gates | Prompt tuning |
| SLICE-PARITY | All slices and prior inventories | End-to-end wiring/readiness evidence | All gates | Supplant claim |

## Replacement Readiness

| Capability | Inventory | Slice | Implementation | Deterministic | Live | Decision | Evidence | Overall |
|---|---|---|---|---|---|---|---|---|
| CORE-DOMAIN | DOM-* | SLICE-DOM | COMPLETE | PASS | NOT REQUIRED | NONE | `src/domain.rs` and `tests/domain.rs`; no migration required for the internal, non-persisted model | READY |
| AN-SIGNALS | AN-*, FORM-* | analysis slices | NOT STARTED | NOT RUN | NOT REQUIRED | NONE | No conflict | NOT READY |
| DISP-TABLES | DISP-* | view/display slices | PARTIAL | NOT RUN | PENDING | NONE | Plain glossary rendering implemented; ANSI, visible-width tables, and LIVE-TERM pending | NOT READY |
| ADV-LLM | ADV-* | advisory slices | NOT STARTED | NOT RUN | PENDING | RESOLVED | DESIGN-003 resolves DO-006; live gates pending | NOT READY |
| CLI-PLAYER | CMD-POOL/CARD/H/P | SLICE-PARITY | NOT STARTED | NOT RUN | NOT REQUIRED | NONE | CLI inventory authoritative | NOT READY |
| CLI-MATCH | CMD-MATCH/ADV-CLI | SLICE-PARITY | NOT STARTED | NOT RUN | PENDING | NONE | Prior inventories authoritative | NOT READY |
| CLI-ROSTER | CMD-ROSTER | SLICE-PARITY | NOT STARTED | NOT RUN | NOT REQUIRED | NONE | CLI inventory authoritative | NOT READY |
| CLI-TOTALS | CMD-RT/TT | SLICE-PARITY | NOT STARTED | NOT RUN | NOT REQUIRED | NONE | CLI inventory authoritative | NOT READY |
| CLI-SP | CMD-SP | SLICE-PARITY | NOT STARTED | NOT RUN | NOT REQUIRED | NONE | Provider inventory authoritative | NOT READY |
| CLI-TEAM | CMD-TEAM | SLICE-PARITY | NOT STARTED | NOT RUN | NOT REQUIRED | NONE | Prior inventories authoritative | NOT READY |
| CLI-GLOSSARY | CMD-WHAT/DISP-GLOSS | SLICE-PARITY | PARTIAL | PASS | PENDING | NONE | Embedded parsing, lookup, suggestions, and plain rendering tested; selector and ANSI deferred | NOT READY |

Use only the fixed AC status vocabularies. Mark a Required capability READY only after COMPLETE implementation, PASS deterministic tests, PASS live checks or an accepted waiver, resolved decisions, and no executable conflict. Keep aggregate readiness NOT READY while any Required capability is NOT READY.

## Source Coverage

- Map CORE-DOMAIN to the four `internal/domain` production files through DOM-*.
- Map AN-SIGNALS to `cmd/skout/avg_test.go`, seven computation documents, thirteen analysis production files, and eight analysis test files through AN-*, FORM-*, and TEST-AN.
- Map DISP-TABLES to eleven display production files and fifteen display test files through DISP-* and TEST-DISP.
- Map ADV-LLM to `cmd/skout/lm.go`, `docs/api-openai.md`, sixteen advisory production files, and nine advisory test files through ADV-* and TEST-ADV.
- Use `docs/calc-pqs.md`, `docs/computations.md`, `docs/handling-stats.md`, `docs/projection-hr.md`, `docs/signal_audit.md`, `docs/stat-fwar.md`, and `docs/stat-wrcplus.md` as the exact analysis document set.
- Use the 86 exact manifest paths in `docs/skout-parity.md` as the machine-enumerable ownership mapping; retain each single manifest row and its capability.

## Residual Risks

- Add deterministic coverage for drop, roster evaluation, trade, Statcast matrices, prompts, moves, and validation errors during their Rust slices.
- Keep terminal, keychain, model-list, and provider protocol checks pending until implementation exists.
- Keep qualitative usefulness non-gating.
- Keep aggregate replacement readiness NOT READY.
