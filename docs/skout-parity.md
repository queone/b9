# Skout Parity Catalog

> Historical reference: this catalog describes the fixed skout source baseline below. Do not treat its implementation or readiness claims as current b9 status. Use `skout-parity-checklist.md` for active parity tracking and inspect the skout source when correcting behavior.

## Source Baseline

- Source repository: `<skout-repo>`
- Commit: `cf65984024bd10a0a41faa69b8aecd3894052c31`
- Required state: exact HEAD and clean working tree
- Catalog scope: tracked Go files under `cmd/skout` and `internal`, plus `README.md`, `arch.md`, `go.mod`, and tracked `docs/*.md`
- Excluded material: secrets, credentials, tokens, keychain values, database contents, generated artifacts, ignored files, and Govna-only mechanics

## Evidence Policy

1. Prefer executable behavior and tests.
2. Prefer production code when executable evidence is absent.
3. Use current documentation only when higher-priority evidence is absent or consistent.
4. Record every disagreement in the conflict ledger.
5. Treat documented-only, missing, retired, or contradictory behavior as unresolved drift.
6. Require a Director decision before adding or dropping unresolved behavior from parity scope.

## Detailed Inventories

- Use `docs/skout-cli-operations.md` for CLI and operational contracts.
- Use `docs/skout-providers-storage.md` for provider, cache, and persistence contracts.
- Use `docs/skout-analysis-display-advisory.md` for domain, analysis, display, advisory, design, verification, implementation-slice, and replacement-readiness contracts.
- Preserve this catalog as the authoritative source manifest and capability ownership map.
- Block Analysis-display-advisory implementation until its detailed inventory is Ratified.

## b9 Closure Dispositions

Use the pinned source manifest below as historical evidence and the detailed inventories as the final implementation ledger. The retained shell is deterministic and complete; replacement readiness remains `NOT READY` until required live gates pass or receive Director-approved waivers.

| b9 surface | Disposition | Deterministic evidence | Live or residual gap |
|---|---|---|---|
| `logout` | Tested Rust improvement | Deletion-only retired-credential cleanup and idempotence tests | Transitional command; keychain observation pending |
| `st` | Required live verification | League selection, status, and CLI tests | Live Yahoo league formats pending |
| `sync` | Tested Rust improvement | Shared synchronization, execution-lock, atomic snapshot, and failure-retention tests | Live Yahoo pending |
| `start` | Tested Rust improvement | Explicit daemon startup and exclusive-ownership tests | Supported-host lifecycle check pending |
| `stop` | Tested Rust improvement | Private control and clean-shutdown tests | Supported-host lifecycle check pending |
| `restart` | Tested Rust improvement | Private control and lifecycle tests | Supported-host lifecycle check pending |
| `log` | Exact parity | Bounded tail/follow and truncation tests | Final terminal comparison pending |
| `reset` | Tested Rust improvement | Scoped, confirmed, idempotent deletion tests | Reset-safe-cancel live path pending |
| `lm` | Required live verification | Model configuration, credential, model-list, and fake-transport tests | Keychain, model list, and provider protocols pending |
| `i` | Tested Rust improvement | Embedded glossary, lookup, suggestion, help, and no-checkout tests | TTY ambiguity selector pending |
| `m` | Unsupported gap | Daily/weekly/day flags, stale/local fallback, advisory, column, and color tests | Live providers plus richer source-baseline status/totals details pending |
| `r` | Unsupported gap | Team selection, stale handling, slot order, fixed columns, and semantic color tests | PQS/PQT/StartHoldScore columns and live Yahoo pending |
| `rt` | Required live verification | League-wide season/weekly totals, shared-identity MLBAM joins, source-compatible innings notation, quality-start retention, and stale-snapshot tests | Live Yahoo and last-week record pending |
| `t` | Required live verification | MLB fixtures, snapshots, fixed columns, and semantic color tests | Live MLB and final terminal comparison pending |
| `tt` | Required live verification | MLB standings/aggregation and rendering tests | Live MLB pending |
| `sp` | Required live verification | MLB/ESPN/OddsShark fixtures, fallback, and rendering tests | Live unofficial endpoints pending |
| `h` | Unsupported gap | Browse/detail, filters, sorts, waiver gates, columns, and color tests | Deferred analytical columns, live Yahoo, and TTY ambiguity pending |
| `p` | Unsupported gap | Browse/detail, filters, sorts, waiver gates, columns, and color tests | Deferred analytical columns, live Yahoo, and TTY ambiguity pending |
| `_daemon` | Tested Rust improvement | Hidden entry, private control, and lifecycle tests | Supported-host lifecycle check pending |
| `whatis` | Exact parity | Compatibility-alias and help tests | None |

Intentional Rust improvements are direct foreground synchronization, explicit daemon startup, private control without PID signaling, isolated b9 state, embedded glossary data, bounded shared transport, atomic complete snapshots, and unreachable rejected-provider automation. Unsupported gaps are Yahoo transaction history, automated Savant/FanGraphs/FantasyPros HTML acquisition, PQS, undefined PQT, StartHoldScore, and their dependent display cells. Bounded on-demand RotoWire lineup acquisition supports roster status parity.

Roster totals follow the predecessor's shared-MLBAM aggregation for Yahoo split identities, including two-way players such as Shohei Ohtani. MLB innings retain the source's display notation (`6.1`/`6.2`) for aggregate parity; rate calculations still use true thirds internally.

## Capability Taxonomy

| ID | Name | Summary | Evidence | Disposition | Dependencies | Workstream |
|---|---|---|---|---|---|---|
| PRODUCT | Product contract | Read-only fantasy-baseball decision support and documented user promises | `<skout-repo>/README.md`, `<skout-repo>/arch.md`, `docs/skout-cli-operations.md#documentation-claim-extraction` | Unresolved drift | All capability groups | CLI and operations |
| META-DEPS | Go dependency surface | Runtime, CLI, keychain, terminal, OAuth, and SQLite dependencies | `<skout-repo>/go.mod` | Required | None | Providers and storage |
| CLI-ROOT | Root CLI contract | Help, version, global flags, attribution, startup hooks, and exit handling | `<skout-repo>/cmd/skout/root.go` | Required | OPS-CONFIG, OPS-DAEMON | CLI and operations |
| CLI-AUTH | Authentication commands | Login, logout, status, league selection, and credential lifecycle | `<skout-repo>/cmd/skout/auth.go` | Required | PROV-YAHOO, OPS-CONFIG | CLI and operations |
| CLI-FETCH | Raw fetch command | Authenticated raw Yahoo request surface | `<skout-repo>/cmd/skout/fetch.go` | Required | PROV-YAHOO | CLI and operations |
| CLI-GLOSSARY | Glossary command | Interactive and direct terminology lookup | `<skout-repo>/cmd/skout/whatis.go` | Required | DISP-TABLES | CLI and operations |
| CLI-PLAYER | Player browse and detail | Hitter, pitcher, waiver, sorting, filtering, and detail-card surfaces | `<skout-repo>/cmd/skout/playerpool.go` | Required | DATA-STORE, AN-SIGNALS, DISP-TABLES | CLI and operations |
| CLI-MATCH | Matchup command | Daily and weekly matchup decision surface and advisory entry | `<skout-repo>/cmd/skout/match.go` | Required | PROV-YAHOO, PROV-MLB, DATA-STORE, AN-SIGNALS, ADV-LLM, DISP-TABLES | CLI and operations |
| CLI-ROSTER | Roster command | Fantasy roster inspection and player context | `<skout-repo>/cmd/skout/roster.go` | Required | PROV-YAHOO, DATA-STORE, AN-SIGNALS, DISP-TABLES | CLI and operations |
| CLI-TOTALS | Totals commands | Fantasy-team and MLB-team aggregate views | `<skout-repo>/cmd/skout/roster_totals.go`, `<skout-repo>/cmd/skout/teams_totals.go` | Required | DATA-STORE, DISP-TABLES | CLI and operations |
| CLI-SP | Probable-pitcher command | Short-horizon starter matchup view | `<skout-repo>/cmd/skout/sp.go` | Required | PROV-MLB, DATA-STORE, DISP-TABLES | CLI and operations |
| CLI-TEAM | MLB team command | Team roster query and display | `<skout-repo>/cmd/skout/team.go` | Required | PROV-MLB, DATA-STORE, DISP-TABLES | CLI and operations |
| OPS-CONFIG | Local configuration | Active league, strategy, and local configuration behavior | `<skout-repo>/internal/config/config.go` | Required | None | CLI and operations |
| OPS-LOCAL | Local reset behavior | Database reset confirmation and local-state deletion | `<skout-repo>/cmd/skout/reset.go` | Required | DATA-STORE | CLI and operations |
| OPS-DAEMON | Service and logging | Start, stop, restart, daemon signaling, PID, and log behavior | `<skout-repo>/cmd/skout/svc.go`, `<skout-repo>/cmd/skout/log.go` | Required | OPS-SYNC | CLI and operations |
| OPS-SYNC | Synchronization orchestration | Live refresh, provider sequencing, state tracking, and snapshot inventory | `<skout-repo>/cmd/skout/sync.go` | Required | All providers, DATA-STORE, AN-SIGNALS | CLI and operations |
| PROV-YAHOO | Yahoo provider | OAuth2 fantasy league, roster, matchup, and transaction data | `<skout-repo>/internal/yahoo` | Required | OPS-CONFIG | Providers and storage |
| PROV-MLB | MLB provider | Players, schedules, games, rosters, standings, and statistics | `<skout-repo>/internal/mlb` | Required | None | Providers and storage |
| PROV-SAVANT | Baseball Savant provider | Statcast CSV acquisition and enrichment | `<skout-repo>/internal/savant` | Required | PROV-MLB | Providers and storage |
| PROV-FANGRAPHS | FanGraphs provider | Leaderboards, projections, constants, and closer data | `<skout-repo>/internal/fangraphs` | Required | PROV-MLB | Providers and storage |
| PROV-FANTASYPROS | FantasyPros provider | Expert-consensus ranking acquisition and identity matching | `<skout-repo>/internal/fantasypros` | Required | PROV-YAHOO, PROV-MLB | Providers and storage |
| PROV-ESPN | ESPN provider | Supplemental sports data client | `<skout-repo>/internal/espnapi` | Required | None | Providers and storage |
| PROV-ODDS | Odds providers | OddsShark acquisition and persisted/displayed odds behavior | `<skout-repo>/internal/oddsshark` | Required | PROV-MLB, DATA-STORE | Providers and storage |
| PROV-ROTOWIRE | RotoWire provider | Match and team-name acquisition | `<skout-repo>/internal/rotowire` | Required | None | Providers and storage |
| DATA-CACHE | Disk cache | Local cached payload lifecycle | `<skout-repo>/internal/cache` | Required | None | Providers and storage |
| DATA-STORE | SQLite store | Schema, migrations, normalized state, freshness, snapshots, and reconciliation | `<skout-repo>/internal/store` | Required | CORE-DOMAIN | Providers and storage |
| CORE-DOMAIN | Domain model | League, matchup, player, and roster types | `<skout-repo>/internal/domain` | Required | None | Analysis, display, and advisory |
| AN-SIGNALS | Analysis engine | Rankings, blending, roles, roster evaluation, trades, and waivers | `<skout-repo>/internal/analysis` | Required | CORE-DOMAIN, DATA-STORE | Analysis, display, and advisory |
| DISP-TABLES | Display layer | Tables, layouts, color, status, player cards, matchup, and odds rendering | `<skout-repo>/internal/display` | Required | CORE-DOMAIN, AN-SIGNALS | Analysis, display, and advisory |
| ADV-LLM | Advisory layer | Decision context, prompts, providers, parsing, strategy, keychain, and validation | `<skout-repo>/internal/advisory` | Required | CORE-DOMAIN, AN-SIGNALS, DATA-STORE | Analysis, display, and advisory |

## Coverage Manifest

| Path | Source kind | Subsystem | Capability IDs | Evidence role | Discovery workstream |
|---|---|---|---|---|---|
| `<skout-repo>/README.md` | Markdown | reference | PRODUCT | current documentation | CLI and operations |
| `<skout-repo>/arch.md` | Markdown | reference | PRODUCT | current documentation | CLI and operations |
| `<skout-repo>/cmd/skout/auth.go` | Go production | command | CLI-AUTH | production code | CLI and operations |
| `<skout-repo>/cmd/skout/avg_test.go` | Go test | command | AN-SIGNALS | executable test | Analysis, display, and advisory |
| `<skout-repo>/cmd/skout/fetch.go` | Go production | command | CLI-FETCH | production code | CLI and operations |
| `<skout-repo>/cmd/skout/glossary_test.go` | Go test | command | CLI-GLOSSARY | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/hitters.go` | Go production | command | CLI-PLAYER | production code | CLI and operations |
| `<skout-repo>/cmd/skout/hitters_test.go` | Go test | command | CLI-PLAYER | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/lm.go` | Go production | command | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/cmd/skout/log.go` | Go production | command | OPS-DAEMON | production code | CLI and operations |
| `<skout-repo>/cmd/skout/log_test.go` | Go test | command | OPS-DAEMON | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/main.go` | Go production | command | CLI-ROOT | production code | CLI and operations |
| `<skout-repo>/cmd/skout/main_test.go` | Go test | command | CLI-ROOT | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/match.go` | Go production | command | CLI-MATCH | production code | CLI and operations |
| `<skout-repo>/cmd/skout/match_test.go` | Go test | command | CLI-MATCH | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/mlb_roster_sync.go` | Go production | command | OPS-SYNC | production code | CLI and operations |
| `<skout-repo>/cmd/skout/mlb_roster_sync_test.go` | Go test | command | OPS-SYNC | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/pitchers.go` | Go production | command | CLI-PLAYER | production code | CLI and operations |
| `<skout-repo>/cmd/skout/pitchers_test.go` | Go test | command | CLI-PLAYER | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/playercard_gamelog.go` | Go production | command | CLI-PLAYER | production code | CLI and operations |
| `<skout-repo>/cmd/skout/playercard_gamelog_test.go` | Go test | command | CLI-PLAYER | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/playerpool.go` | Go production | command | CLI-PLAYER | production code | CLI and operations |
| `<skout-repo>/cmd/skout/reset.go` | Go production | command | OPS-LOCAL | production code | CLI and operations |
| `<skout-repo>/cmd/skout/reset_test.go` | Go test | command | OPS-LOCAL | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/root.go` | Go production | command | CLI-ROOT | production code | CLI and operations |
| `<skout-repo>/cmd/skout/roster.go` | Go production | command | CLI-ROSTER | production code | CLI and operations |
| `<skout-repo>/cmd/skout/roster_test.go` | Go test | command | CLI-ROSTER | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/roster_totals.go` | Go production | command | CLI-TOTALS | production code | CLI and operations |
| `<skout-repo>/cmd/skout/roster_totals_test.go` | Go test | command | CLI-TOTALS | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/snapshot_inventory.go` | Go production | command | OPS-SYNC | production code | CLI and operations |
| `<skout-repo>/cmd/skout/snapshot_inventory_test.go` | Go test | command | OPS-SYNC | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/sp.go` | Go production | command | CLI-SP | production code | CLI and operations |
| `<skout-repo>/cmd/skout/sp_test.go` | Go test | command | CLI-SP | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/svc.go` | Go production | command | OPS-DAEMON | production code | CLI and operations |
| `<skout-repo>/cmd/skout/svc_test.go` | Go test | command | OPS-DAEMON | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/sync.go` | Go production | command | OPS-SYNC | production code | CLI and operations |
| `<skout-repo>/cmd/skout/sync_fantasypros_test.go` | Go test | command | OPS-SYNC | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/sync_test.go` | Go test | command | OPS-SYNC | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/team.go` | Go production | command | CLI-TEAM | production code | CLI and operations |
| `<skout-repo>/cmd/skout/team_test.go` | Go test | command | CLI-TEAM | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/teams_totals.go` | Go production | command | CLI-TOTALS | production code | CLI and operations |
| `<skout-repo>/cmd/skout/teams_totals_test.go` | Go test | command | CLI-TOTALS | executable test | CLI and operations |
| `<skout-repo>/cmd/skout/tui.go` | Go production | command | CLI-ROOT | production code | CLI and operations |
| `<skout-repo>/cmd/skout/whatis.go` | Go production | command | CLI-GLOSSARY | production code | CLI and operations |
| `<skout-repo>/docs/api-espn.md` | Markdown | reference | PROV-ESPN | current documentation | Providers and storage |
| `<skout-repo>/docs/api-fangraphs.md` | Markdown | reference | PROV-FANGRAPHS | current documentation | Providers and storage |
| `<skout-repo>/docs/api-fantasypros.md` | Markdown | reference | PROV-FANTASYPROS | current documentation | Providers and storage |
| `<skout-repo>/docs/api-mlbam.md` | Markdown | reference | PROV-MLB | current documentation | Providers and storage |
| `<skout-repo>/docs/api-openai.md` | Markdown | reference | ADV-LLM | current documentation | Analysis, display, and advisory |
| `<skout-repo>/docs/api-rotowire.md` | Markdown | reference | PROV-ROTOWIRE | current documentation | Providers and storage |
| `<skout-repo>/docs/api-savant.md` | Markdown | reference | PROV-SAVANT | current documentation | Providers and storage |
| `<skout-repo>/docs/api-yahoo.md` | Markdown | reference | PROV-YAHOO | current documentation | Providers and storage |
| `<skout-repo>/docs/calc-pqs.md` | Markdown | reference | AN-SIGNALS | current documentation | Analysis, display, and advisory |
| `<skout-repo>/docs/computations.md` | Markdown | reference | AN-SIGNALS | current documentation | Analysis, display, and advisory |
| `<skout-repo>/docs/dev-canonical-identity.md` | Markdown | reference | DATA-STORE | current documentation | Providers and storage |
| `<skout-repo>/docs/glossary.md` | Markdown | reference | CLI-GLOSSARY | current documentation | CLI and operations |
| `<skout-repo>/docs/handling-stats.md` | Markdown | reference | AN-SIGNALS | current documentation | Analysis, display, and advisory |
| `<skout-repo>/docs/projection-hr.md` | Markdown | reference | AN-SIGNALS | current documentation | Analysis, display, and advisory |
| `<skout-repo>/docs/signal_audit.md` | Markdown | reference | AN-SIGNALS | current documentation | Analysis, display, and advisory |
| `<skout-repo>/docs/skout-sync.md` | Markdown | reference | OPS-SYNC | current documentation | CLI and operations |
| `<skout-repo>/docs/stat-fwar.md` | Markdown | reference | AN-SIGNALS | current documentation | Analysis, display, and advisory |
| `<skout-repo>/docs/stat-wrcplus.md` | Markdown | reference | AN-SIGNALS | current documentation | Analysis, display, and advisory |
| `<skout-repo>/docs/yahoo-api-access-403.md` | Markdown | reference | PROV-YAHOO | current documentation | Providers and storage |
| `<skout-repo>/go.mod` | Go module | dependencies | META-DEPS | dependency manifest | Providers and storage |
| `<skout-repo>/internal/advisory/advisory_test.go` | Go test | advisory | ADV-LLM | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/alerts.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/category.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/context.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/debug_test.go` | Go test | advisory | ADV-LLM | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/glossary.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/glossary_test.go` | Go test | advisory | ADV-LLM | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/keychain.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/lineup.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/lineup_test.go` | Go test | advisory | ADV-LLM | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/llm.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/llm_compat_test.go` | Go test | advisory | ADV-LLM | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/moves.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/openai_models.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/openai_models_test.go` | Go test | advisory | ADV-LLM | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/parse_test.go` | Go test | advisory | ADV-LLM | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/payload.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/payload_test.go` | Go test | advisory | ADV-LLM | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/pp.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/pp_compute.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/pp_test.go` | Go test | advisory | ADV-LLM | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/prompt.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/strategy.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/types.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/advisory/validate.go` | Go production | advisory | ADV-LLM | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/birthdates.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/birthdates_test.go` | Go test | analysis | AN-SIGNALS | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/blend.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/blend_test.go` | Go test | analysis | AN-SIGNALS | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/browse_sort.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/browse_sort_test.go` | Go test | analysis | AN-SIGNALS | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/drop.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/pitcher_role.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/pitcher_role_test.go` | Go test | analysis | AN-SIGNALS | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/pqs.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/pqs_test.go` | Go test | analysis | AN-SIGNALS | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/roster.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/stat_weights.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/statcast_blend.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/trade.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/waiver.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/waiver_test.go` | Go test | analysis | AN-SIGNALS | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/window_proj.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/window_proj_test.go` | Go test | analysis | AN-SIGNALS | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/wire_threshold.go` | Go production | analysis | AN-SIGNALS | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/analysis/wire_threshold_test.go` | Go test | analysis | AN-SIGNALS | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/cache/disk.go` | Go production | cache | DATA-CACHE | production code | Providers and storage |
| `<skout-repo>/internal/cache/disk_test.go` | Go test | cache | DATA-CACHE | executable test | Providers and storage |
| `<skout-repo>/internal/config/config.go` | Go production | config | OPS-CONFIG | production code | CLI and operations |
| `<skout-repo>/internal/config/config_test.go` | Go test | config | OPS-CONFIG | executable test | CLI and operations |
| `<skout-repo>/internal/display/advisory.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/breakdown_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/glossary.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/match_header_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/matchup.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/matchup_layout_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/matchup_status_nogame_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/matchup_status_ppd_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/matchup_totals_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/mlb_standings.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/mlb_standings_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/odds.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/odds_recap_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/odds_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/playercard.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/playercard_gamelog.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/playercard_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/poscell.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/poscell_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/roster_totals.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/sp.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/sp_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/table.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/table_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/display/team_roster.go` | Go production | display | DISP-TABLES | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/display/team_roster_test.go` | Go test | display | DISP-TABLES | executable test | Analysis, display, and advisory |
| `<skout-repo>/internal/domain/league.go` | Go production | domain | CORE-DOMAIN | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/domain/matchup.go` | Go production | domain | CORE-DOMAIN | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/domain/player.go` | Go production | domain | CORE-DOMAIN | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/domain/roster.go` | Go production | domain | CORE-DOMAIN | production code | Analysis, display, and advisory |
| `<skout-repo>/internal/espnapi/client.go` | Go production | espnapi | PROV-ESPN | production code | Providers and storage |
| `<skout-repo>/internal/espnapi/client_test.go` | Go test | espnapi | PROV-ESPN | executable test | Providers and storage |
| `<skout-repo>/internal/fangraphs/client.go` | Go production | fangraphs | PROV-FANGRAPHS | production code | Providers and storage |
| `<skout-repo>/internal/fangraphs/client_test.go` | Go test | fangraphs | PROV-FANGRAPHS | executable test | Providers and storage |
| `<skout-repo>/internal/fangraphs/closer_chart.go` | Go production | fangraphs | PROV-FANGRAPHS | production code | Providers and storage |
| `<skout-repo>/internal/fangraphs/closer_chart_test.go` | Go test | fangraphs | PROV-FANGRAPHS | executable test | Providers and storage |
| `<skout-repo>/internal/fangraphs/crosswalk.go` | Go production | fangraphs | PROV-FANGRAPHS | production code | Providers and storage |
| `<skout-repo>/internal/fangraphs/projections.go` | Go production | fangraphs | PROV-FANGRAPHS | production code | Providers and storage |
| `<skout-repo>/internal/fantasypros/client.go` | Go production | fantasypros | PROV-FANTASYPROS | production code | Providers and storage |
| `<skout-repo>/internal/fantasypros/client_test.go` | Go test | fantasypros | PROV-FANTASYPROS | executable test | Providers and storage |
| `<skout-repo>/internal/mlb/active_roster.go` | Go production | mlb | PROV-MLB | production code | Providers and storage |
| `<skout-repo>/internal/mlb/active_roster_test.go` | Go test | mlb | PROV-MLB | executable test | Providers and storage |
| `<skout-repo>/internal/mlb/client.go` | Go production | mlb | PROV-MLB | production code | Providers and storage |
| `<skout-repo>/internal/mlb/client_test.go` | Go test | mlb | PROV-MLB | executable test | Providers and storage |
| `<skout-repo>/internal/mlb/enrich.go` | Go production | mlb | PROV-MLB | production code | Providers and storage |
| `<skout-repo>/internal/mlb/enrich_test.go` | Go test | mlb | PROV-MLB | executable test | Providers and storage |
| `<skout-repo>/internal/mlb/models.go` | Go production | mlb | PROV-MLB | production code | Providers and storage |
| `<skout-repo>/internal/mlb/people.go` | Go production | mlb | PROV-MLB | production code | Providers and storage |
| `<skout-repo>/internal/mlb/people_test.go` | Go test | mlb | PROV-MLB | executable test | Providers and storage |
| `<skout-repo>/internal/mlb/standings.go` | Go production | mlb | PROV-MLB | production code | Providers and storage |
| `<skout-repo>/internal/mlb/teams.go` | Go production | mlb | PROV-MLB | production code | Providers and storage |
| `<skout-repo>/internal/mlb/teams_test.go` | Go test | mlb | PROV-MLB | executable test | Providers and storage |
| `<skout-repo>/internal/oddsshark/client.go` | Go production | oddsshark | PROV-ODDS | production code | Providers and storage |
| `<skout-repo>/internal/oddsshark/client_test.go` | Go test | oddsshark | PROV-ODDS | executable test | Providers and storage |
| `<skout-repo>/internal/rotowire/client.go` | Go production | rotowire | PROV-ROTOWIRE | production code | Providers and storage |
| `<skout-repo>/internal/rotowire/match.go` | Go production | rotowire | PROV-ROTOWIRE | production code | Providers and storage |
| `<skout-repo>/internal/rotowire/rotowire_test.go` | Go test | rotowire | PROV-ROTOWIRE | executable test | Providers and storage |
| `<skout-repo>/internal/rotowire/teams.go` | Go production | rotowire | PROV-ROTOWIRE | production code | Providers and storage |
| `<skout-repo>/internal/savant/client.go` | Go production | savant | PROV-SAVANT | production code | Providers and storage |
| `<skout-repo>/internal/savant/client_test.go` | Go test | savant | PROV-SAVANT | executable test | Providers and storage |
| `<skout-repo>/internal/savant/enrich.go` | Go production | savant | PROV-SAVANT | production code | Providers and storage |
| `<skout-repo>/internal/savant/models.go` | Go production | savant | PROV-SAVANT | production code | Providers and storage |
| `<skout-repo>/internal/store/closer_override.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/closer_override_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/ecr_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/fip.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/fip_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/fold.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/fold_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/identity.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/identity_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/league.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/manifest.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/odds.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/odds_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/player.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/player_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/projection.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/projection_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/schedule.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/schedule_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/schema.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/snapshots.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/snapshots_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/statcast.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/store.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/store_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/syncrun.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/team.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/team_roster.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/team_roster_test.go` | Go test | store | DATA-STORE | executable test | Providers and storage |
| `<skout-repo>/internal/store/testutil.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/store/transaction.go` | Go production | store | DATA-STORE | production code | Providers and storage |
| `<skout-repo>/internal/yahoo/auth.go` | Go production | yahoo | PROV-YAHOO | production code | Providers and storage |
| `<skout-repo>/internal/yahoo/auth_test.go` | Go test | yahoo | PROV-YAHOO | executable test | Providers and storage |
| `<skout-repo>/internal/yahoo/client.go` | Go production | yahoo | PROV-YAHOO | production code | Providers and storage |
| `<skout-repo>/internal/yahoo/client_test.go` | Go test | yahoo | PROV-YAHOO | executable test | Providers and storage |
| `<skout-repo>/internal/yahoo/models.go` | Go production | yahoo | PROV-YAHOO | production code | Providers and storage |
| `<skout-repo>/internal/yahoo/parse.go` | Go production | yahoo | PROV-YAHOO | production code | Providers and storage |
| `<skout-repo>/internal/yahoo/parse_matchup.go` | Go production | yahoo | PROV-YAHOO | production code | Providers and storage |
| `<skout-repo>/internal/yahoo/parse_matchup_test.go` | Go test | yahoo | PROV-YAHOO | executable test | Providers and storage |
| `<skout-repo>/internal/yahoo/parse_test.go` | Go test | yahoo | PROV-YAHOO | executable test | Providers and storage |
| `<skout-repo>/internal/yahoo/sync.go` | Go production | yahoo | PROV-YAHOO | production code | Providers and storage |

## Conflict Ledger

| ID | Competing evidence | Current observed behavior | Compatibility question | Director decision |
|---|---|---|---|---|
| CF-001 | `<skout-repo>/arch.md:113` and `<skout-repo>/arch.md:199` describe `skout odds` and `internal/oddsapi`; no matching command or package exists in the tracked source | Odds data is stored and displayed through existing store/display code, but no `odds` command is registered | Restore a dedicated command, preserve only reachable odds behavior, or retire the documented surface? | Resolved — preserve current executable behavior; do not restore the retired standalone command, totals, or strikeout props; see `docs/skout-cli-operations.md#settled-drift` |
| CF-002 | `<skout-repo>/README.md:68` describes top 10 player browse results; `<skout-repo>/cmd/skout/playerpool.go:171` defaults to 20 | Executable command behavior takes precedence over the README claim | Preserve the executable default or intentionally restore the documented default? | Resolved — preserve the executable default of 20; see `docs/skout-cli-operations.md#settled-drift` |
| CF-003 | `<skout-repo>/arch.md:162` documents `--no-advise/-A`; `<skout-repo>/cmd/skout/match.go:661` registers `--advise/-a` | Advisory is opt-in through `--advise/-a` | Preserve opt-in advisory or reintroduce an opt-out surface? | Resolved — preserve opt-in `--advise/-a`; see `docs/skout-cli-operations.md#settled-drift` |

## Initial Design Observations

| ID | Evidence | Risk hypothesis | Affected capability IDs | Owning workstream |
|---|---|---|---|---|
| DO-001 | `<skout-repo>/cmd/skout/root.go`, `<skout-repo>/go.mod` | Cobra registration plus separately rendered help can drift between executable parsing and documented output | CLI-ROOT | CLI and operations |
| DO-002 | `<skout-repo>/cmd/skout/sync.go` | A large orchestration unit may couple provider acquisition, persistence, computation, logging, and failure policy | OPS-SYNC, DATA-STORE, AN-SIGNALS | CLI and operations |
| DO-003 | `<skout-repo>/cmd/skout/svc.go`, `<skout-repo>/internal/advisory/keychain.go` | Unix signals, PID files, browser launch, terminal interaction, and OS keychains create platform-specific boundaries | OPS-DAEMON, CLI-AUTH, ADV-LLM | CLI and operations |
| DO-004 | `<skout-repo>/internal/store/schema.go`, `<skout-repo>/internal/store/store.go` | Monolithic schema ownership and additive in-place upgrades may make migration and rollback behavior difficult to isolate | DATA-STORE | Providers and storage |
| DO-005 | `<skout-repo>/internal/fangraphs/client.go`, `<skout-repo>/internal/fantasypros/client.go`, `<skout-repo>/internal/savant/client.go` | HTML, embedded JavaScript, and CSV scraping boundaries may be fragile under upstream format changes | PROV-FANGRAPHS, PROV-FANTASYPROS, PROV-SAVANT | Providers and storage |
| DO-006 | `<skout-repo>/internal/advisory/llm.go`, `<skout-repo>/internal/advisory/openai_models.go` | Provider-specific request and model-list behavior may leak into the shared advisory abstraction | ADV-LLM | Analysis, display, and advisory |

No design recommendation or compatibility choice is settled by this catalog.

## Discovery Program

Use outside-in discovery order: establish observable CLI and operational contracts before analyzing the provider, storage, analysis, display, and advisory internals that satisfy them. This is not the implementation order.

1. **CLI and operations** — detailed inventory: [`docs/skout-cli-operations.md`](skout-cli-operations.md). Expand CLI-ROOT through OPS-SYNC into command, argument, flag, output, error, exit, configuration, filesystem, daemon, logging, debug, and reset contracts; resolve its assigned conflicts.
2. **Providers and storage** — detailed inventory: [`docs/skout-providers-storage.md`](skout-providers-storage.md). Expand META-DEPS, provider, cache, and store capabilities into authentication, fetch, normalization, schema, migration, write, snapshot, freshness, fallback, reconciliation, and failure contracts.
3. **Analysis, display, and advisory** — expand domain, analysis, display, and advisory capabilities into computation, terminology, layout, TUI, prompt, provider, parsing, and decision-grounding contracts.

Each detailed-discovery AC must define deterministic pre-release tests separately from live-provider checks, reconcile its assigned conflicts through explicit Director decisions, and remain Ratified before Rust feature implementation begins for its capabilities. Final parity and replacement-readiness evidence will be defined only after all three detailed inventories are complete.
