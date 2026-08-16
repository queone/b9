# b9 Glossary

Canonical definitions for baseball, fantasy, and b9-specific terms. This file is the single source of truth — other docs reference it, never redefine terms in their own words. Computation and API docs may add implementation-specific detail; prompts may compress wording; nothing may contradict definitions here.

Historical `internal/...` entries in **Where** fields identify the pinned Go source evidence from which the definition was ported; they are not claims about Rust module paths. Delivered and deferred Rust ownership is recorded in the parity documents.

Definitions describe the shared baseball vocabulary, including source-baseline signals that remain deferred in Rust. A definition is not an implementation-readiness claim; `docs/skout-analysis-display-advisory.md` records delivered displays and residual gaps.

When a change introduces or redefines a domain term, update this file in the same pass.

---

## Coverage Checklist

Every key below must have a corresponding entry in this glossary. When a new stat or signal is added to b9, add it here.

`ab`, `abandon`, `active`, `age`, `atc`, `avg`, `available`, `babip`, `barrel_pct`, `batters_faced`, `bb`, `bb_pct`, `bench`, `blend_window`, `category_strategy`, `cfip`, `ch_pct`, `close`, `closer`, `confirmed`, `confirmed_sp`, `cs`, `dtd`, `ecr`, `empirical_bayes`, `era`, `exit_velo`, `expected`, `faab`, `fastball_velo`, `fb_pct`, `fip`, `flippable`, `fwar`, `g`, `gb_pct`, `gs`, `h2h`, `hard_hit_pct`, `hbp`, `holds`, `hr`, `hr_fb`, `il`, `injured`, `ip`, `k`, `k_bb_pct`, `k_pct`, `k9`, `last03ip`, `last10ip`, `last20pa`, `last5yrs`, `launch_angle`, `lineup_candidates`, `lineup_status`, `lost`, `na`, `next03ip`, `next10ip`, `next20pa`, `no_game`, `not_scheduled`, `obp`, `ops`, `opportunity_damping`, `out`, `own_pct`, `p_slot`, `pa`, `pitcher_day_state`, `pool`, `pos`, `pp`, `ppd`, `probable`, `probable_sp`, `protect`, `punt`, `push`, `qs`, `r`, `rbi`, `replacement_level`, `roster_moves`, `roster_moves_note`, `roster_slot`, `rp_available`, `rp_slot`, `sb`, `savant`, `slg`, `sp_slot`, `spin_rate`, `sprint_speed`, `stabilization_ramp`, `steamer`, `streaming`, `sv`, `sweet_spot_pct`, `tied`, `pqs`, `w`, `waiver_wire`, `whiff_pct`, `whip`, `wrc_plus`, `xba`, `xera`, `xfip`, `xobp`, `xslg`, `xwoba`, `yp`, `yr`, `z_score`, `zips`

---

## Baseball

### Active (`active`) [baseball]

Player status: in a starting lineup slot (C, 1B, 2B, 3B, SS, OF, Util, SP, RP). Not on the bench or injured list.

- **Where:** `internal/advisory/types.go` — `PlayerStatus`
- **Prompt:** no

### At-Bat (`ab`) [baseball]

A plate appearance that results in a hit, out, fielder's choice, or error — excludes walks, HBP, sacrifices, and catcher interference. The denominator for batting average.

- **Aliases:** AB
- **Prompt:** no

### Batters Faced (`batters_faced`) [baseball]

Total plate appearances against a pitcher. The pitching equivalent of PA; denominator for K% and BB% on the pitching side.

- **Aliases:** BF
- **Prompt:** no

### Bench (`bench`) [baseball]

Player status: rostered in a BN (bench) slot. Not in the active lineup but available for activation.

- **Where:** `internal/advisory/types.go` — `PlayerStatus`
- **Prompt:** no

### Caught Stealing (`cs`) [baseball]

Baserunner thrown out attempting to steal a base.

- **Aliases:** CS
- **Prompt:** no

### Day-to-Day (`dtd`) [baseball]

Injury status: player is nursing a minor injury but has not been placed on the injured list. May or may not play on any given day.

- **Aliases:** DTD
- **Prompt:** no

### Games (`g`) [baseball]

Total games in which a player appeared (hitters or pitchers).

- **Aliases:** G
- **Prompt:** no

### Games Started (`gs`) [baseball]

Games in which a pitcher was the starting pitcher. Used to classify SP vs RP (`GS * 2 >= G` → SP).

- **Aliases:** GS
- **Prompt:** no

### Hit By Pitch (`hbp`) [baseball]

Batter awarded first base after being struck by a pitch. Counts as a plate appearance but not an at-bat.

- **Aliases:** HBP
- **Prompt:** no

### Holds (`holds`) [baseball]

Relief pitcher enters with a lead of 3 or fewer runs (or tying run on base/at bat/on deck), records at least one out, and leaves without relinquishing the lead. Not a standard Yahoo scoring category in most leagues.

- **Aliases:** HLD
- **Prompt:** no

### Injured List (`il`) [baseball]

MLB designation for players unable to play due to injury. Variants: IL10 (10-day), IL15 (15-day), IL60 (60-day). In fantasy, rostering a player on IL frees an active roster spot.

- **Aliases:** IL, IL10, IL15, IL60, DL (legacy)
- **Prompt:** no

### Innings Pitched (`ip`) [baseball]

Outs recorded divided by 3. MLBAM notation: `7.1` = 7⅓ innings (7 full innings + 1 out), `7.2` = 7⅔ innings.

- **Aliases:** IP
- **Prompt:** yes

### Not Active (`na`) [baseball]

Status for players not on the MLB active roster — typically minor league or restricted list players. In Yahoo fantasy, these players occupy an NA slot.

- **Aliases:** NA
- **Prompt:** no

### Plate Appearance (`pa`) [baseball]

Any completed turn at bat — includes hits, outs, walks, HBP, sacrifices. The most inclusive batting denominator.

- **Aliases:** PA
- **Prompt:** no

### Quality Start (`qs`) [baseball]

Starting pitcher completes at least 6 innings with 3 or fewer earned runs allowed.

- **Aliases:** QS
- **Prompt:** yes

### Save Opportunity (`sv`) [baseball]

Closer enters with a lead of 3 or fewer runs (or tying run on base/at bat/on deck) and finishes the game preserving the lead. A standard Yahoo pitching category.

- **Aliases:** SV, Save
- **Prompt:** no

---

## Fantasy

### Abandon (`abandon`) [fantasy]

Category strategy classification: category is either lost (gap too large to close) or explicitly punted via `strategy.yaml`. No resources should be directed here.

- **Where:** `internal/advisory/payload.go` — `ComputeCategoryStrategy`
- **Prompt:** yes

### Available (`available`) [fantasy]

Lineup status for a relief pitcher whose team has a game today. The RP may pitch but has no scheduled start.

- **Where:** `internal/advisory/lineup.go` — `pitcherDayState`
- **Prompt:** yes

### Close (`close`) [fantasy]

Category gap status: user is ahead but the margin is thin enough to flip. Treated as a protect-priority category.

- **Prompt:** no

### Confirmed (`confirmed`) [fantasy]

Lineup status: hitter verified in today's batting order (from RotoWire or MLBAM confirmed lineups), or pitcher confirmed as today's starter (RotoWire).

- **Where:** `internal/advisory/lineup.go`
- **Prompt:** yes

### Confirmed SP (`confirmed_sp`) [fantasy]

Pitcher day state: RotoWire has confirmed this pitcher as today's starting pitcher. Highest confidence level for SP lineup decisions.

- **Where:** `internal/advisory/lineup.go` — `pitcherDayState`
- **Prompt:** no

### Expert Consensus Rank (`ecr`) [fantasy]

Aggregate ranking from FantasyPros combining multiple expert rankings. Lower = better. Displayed as `CR` column.

- **Aliases:** ECR, CR
- **Where:** `internal/fantasypros/`; `display/table.go`
- **Prompt:** no

### Expected (`expected`) [fantasy]

Lineup status: hitter whose team has a game today but the batting order has not been posted yet. Default assumption is the player will play.

- **Prompt:** yes

### FAAB (`faab`) [fantasy]

Free Agent Acquisition Budget. Fixed dollar amount each team can bid on waiver claims over the season. Tracked in `yahoo_leagues.faab_budget`.

- **Aliases:** FAAB budget
- **Prompt:** no

### Flippable (`flippable`) [fantasy]

A category where the gap between user and opponent is small enough that roster decisions this week could change the outcome. Categories with status `behind`, `tied`, or `close` are flippable (unless punted).

- **Prompt:** yes

### Head-to-Head Categories (`h2h`) [fantasy]

League format where two teams compete each week across scoring categories. Each category is a win, loss, or tie. Standard Yahoo format: 5 hitting (R, HR, RBI, SB, AVG) + 5 pitching (W, SV, K, ERA, WHIP).

- **Aliases:** H2H
- **Prompt:** no

### Injured (`injured`) [fantasy]

Player status: rostered in an IL (Injured List) slot. Cannot be placed in active lineup slots.

- **Prompt:** no

### Lineup Status (`lineup_status`) [fantasy]

Per-player field in the advisory payload describing same-day game availability. Values: `confirmed`, `probable`, `expected`, `out`, `not_scheduled`, `no_game`, `available`. Derived from pitcher day state (pitchers) or confirmed lineup data (hitters). The LLM uses this to reason about which players are actually playing today.

- **Where:** `internal/advisory/payload.go` — `PayloadPlayer.LineupStatus`
- **Prompt:** yes

### Lineup Candidates (`lineup_candidates`) [fantasy]

Pre-computed list of position-eligible swap suggestions: bench players who can replace active players. The LLM advisory may only reference swaps from this list (ACTION SOURCES constraint).

- **Where:** `internal/advisory/lineup.go` — `ComputeLineupCandidates`
- **Prompt:** yes

### Lost (`lost`) [fantasy]

Category gap status: the gap is too large to realistically close given remaining games. Treated as abandon unless the user overrides.

- **Prompt:** no

### No Game (`no_game`) [fantasy]

Lineup status: player's MLB team has no game scheduled today.

- **Prompt:** yes

### Not Scheduled (`not_scheduled`) [fantasy]

Lineup status for an SP-eligible pitcher: their team has a game today but this pitcher is not the scheduled starter.

- **Where:** `internal/advisory/lineup.go` — `pitcherDayState`
- **Prompt:** yes

### Out (`out`) [fantasy]

Lineup status: today's batting order has been confirmed and this hitter is not in it.

- **Prompt:** yes

### Ownership Percentage (`own_pct`) [fantasy]

Percentage of Yahoo leagues in which a player is rostered. Displayed as `%OWN`.

- **Aliases:** %OWN, percent_owned
- **Where:** `display/table.go`
- **Prompt:** no

### Probable (`probable`) [fantasy]

Lineup status for a pitcher listed as MLBAM probable starter but not yet confirmed by RotoWire.

- **Prompt:** yes

### Postponed / PPD (`ppd`) [fantasy]

Game postponed due to weather or other reasons. Displayed in two forms: (a) **forecast warning** — yellow `PPD?` indicator suffixed to a scheduled game's status when precipitation probability exceeds 50% (the game may yet be postponed); (b) **confirmed postponement** — the literal token `PPD` in medium-dim red (color.Red3, idx 124) replaces the entire status cell once MLB's API reports `Status.DetailedState == "Postponed"`. Affects lineup planning — players from PPD games will not accrue stats.

- **Aliases:** PPD, rainout
- **Where:** `internal/advisory/alerts.go`; `internal/display/matchup.go`; `internal/mlb/teams.go` (`gameDisplayStatus` maps `"Postponed"` → `"PPD"` token)
- **Prompt:** yes

### Probable SP (`probable_sp`) [fantasy]

Pitcher day state: MLBAM lists this pitcher as the probable starter but RotoWire has not yet confirmed. Lower confidence than `confirmed_sp` but still a strong signal.

- **Where:** `internal/advisory/lineup.go` — `pitcherDayState`
- **Prompt:** no

### Protect (`protect`) [fantasy]

Category strategy classification: user is ahead or close in this category (and it's not punted). Priority is risk mitigation — avoid losing ground.

- **Where:** `internal/advisory/payload.go` — `ComputeCategoryStrategy`
- **Prompt:** yes

### Punt (`punt`) [fantasy]

Deliberate decision to concede a scoring category for the season, redirecting roster resources to other categories. Configured in `strategy.yaml` as `punt_categories`.

- **Aliases:** punting
- **Prompt:** no

### Push (`push`) [fantasy]

Category strategy classification: user is behind or tied in this category (and it's not punted). Priority is gaining ground — maximize production in this category.

- **Where:** `internal/advisory/payload.go` — `ComputeCategoryStrategy`
- **Prompt:** yes

### Replacement Level (`replacement_level`) [fantasy]

The talent level of the best freely available player at a position. In a 12-team league, roughly the 12th-best player at each position. Used as the baseline for positional scarcity adjustments in PQS.

- **Prompt:** no

### Roster Moves Note (`roster_moves_note`) [fantasy]

Advisory payload field containing a short text note about available roster moves (e.g., remaining weekly adds, FAAB budget). Displayed to the LLM as context for pickup/drop recommendations. The LLM must not fabricate roster moves beyond what `roster_moves` provides.

- **Where:** `internal/advisory/payload.go` — `AdvisoryPayload.RosterMovesNote`
- **Prompt:** yes

### Roster Slot (`roster_slot`) [fantasy]

A lineup position in Yahoo Fantasy where a player is placed. Distinct from player position eligibility — a player eligible at SP and RP can occupy an SP slot, RP slot, P slot, or BN. Each league defines how many of each slot type exist. Slot types: C, 1B, 2B, 3B, SS, OF, Util (any hitter), SP, RP, P (any pitcher), BN (bench), IL (injured list).

- **Aliases:** slot, lineup slot
- **Prompt:** no

### SP Slot (`sp_slot`) [fantasy]

Yahoo roster slot reserved for starting pitchers. Only players with SP eligibility can be placed here. The number of SP slots is league-configured (typically 2). A pitcher in an SP slot is "active" regardless of whether they are actually starting today.

- **Avoid:** confusing SP slot (roster placement) with SP role (pitcher who starts games)
- **Prompt:** no

### RP Slot (`rp_slot`) [fantasy]

Yahoo roster slot reserved for relief pitchers. Only players with RP eligibility can be placed here. The number of RP slots is league-configured (typically 2).

- **Avoid:** confusing RP slot (roster placement) with RP role (pitcher who relieves)
- **Prompt:** no

### P Slot (`p_slot`) [fantasy]

Yahoo roster slot that accepts any pitcher (SP or RP eligible). Provides overflow capacity beyond dedicated SP and RP slots. The number of P slots is league-configured (typically 2).

- **Prompt:** no

### RP Available (`rp_available`) [fantasy]

Pitcher day state: relief pitcher whose team has a game today. Eligible to pitch but has no scheduled start — availability is implicit.

- **Where:** `internal/advisory/lineup.go` — `pitcherDayState`
- **Prompt:** no

### Roster Moves (`roster_moves`) [fantasy]

Pre-computed list of waiver wire pickup/drop suggestions ranked by category fit. The LLM advisory may only reference roster moves from this list (ACTION SOURCES constraint).

- **Where:** `internal/advisory/moves.go` — `ComputeRosterMoves`
- **Prompt:** yes

### Streaming (`streaming`) [fantasy]

Strategy of frequently adding and dropping pitchers (usually SPs) to maximize counting stats (W, K) by targeting favorable matchups. Configured in `strategy.yaml`.

- **Prompt:** no

### Tied (`tied`) [fantasy]

Category gap status: user and opponent have the same score in this category.

- **Prompt:** no

### Waiver Wire (`waiver_wire`) [fantasy]

The pool of unrostered players available for pickup. In FAAB leagues, claims require a dollar bid.

- **Aliases:** FA, free agents
- **Prompt:** no

### Yahoo Rank (`yr`) [fantasy]

Yahoo's pre-calculated overall player rank for the season. Lower = better. Displayed as `YR` column.

- **Aliases:** YR
- **Where:** `display/table.go`
- **Prompt:** no

---

## Stats

### Batting Average (`avg`) [stat]

Hits divided by at-bats (H/AB). A standard Yahoo hitting category. Rate stat — not scaled by volume.

- **Aliases:** AVG, BA
- **Prompt:** yes

### BB% (`bb_pct`) [stat]

Walk rate: walks divided by plate appearances (hitters) or batters faced (pitchers). Higher is better for hitters (discipline); lower is better for pitchers (command).

- **Aliases:** Walk Rate
- **Prompt:** no

### BABIP (`babip`) [stat]

Batting Average on Balls In Play: `(H - HR) / (AB - K - HR + SF)`. Measures how often batted balls (excluding home runs) fall for hits. Useful for identifying luck-driven AVG spikes or slumps — league average is roughly .300.

- **Aliases:** Batting Average on Balls In Play
- **Where:** `mlbam_season_stats.babip`
- **Prompt:** no

### Barrel% (`barrel_pct`) [stat]

Percentage of batted ball events in the "barrel" zone — optimal combination of exit velocity and launch angle that produces extra-base hits at an elite rate. Best single power/HR signal from Statcast.

- **Where:** `statcast_seasons.barrel_pct`
- **Prompt:** no

### Chase% (`ch_pct`) [stat]

Percentage of pitches outside the strike zone that the batter swings at. For pitchers, higher = better (inducing swings on bad pitches). A command and swing-and-miss proxy.

- **Aliases:** O-Swing%, Chase Rate
- **Where:** `statcast_seasons.chase_pct`
- **Prompt:** no

### Earned Run Average (`era`) [stat]

Earned runs allowed per 9 innings pitched: `(ER / IP) * 9`. A standard Yahoo pitching category. Lower is better.

- **Aliases:** ERA
- **Prompt:** yes

### Exit Velocity (`exit_velo`) [stat]

Average speed of the ball off the bat in miles per hour. Higher exit velocity correlates with more extra-base hits and home runs.

- **Aliases:** EV, Exit Velo
- **Where:** `statcast_seasons.exit_velo_avg`
- **Prompt:** no

### Expected Batting Average (`xba`) [stat]

Statcast-derived expected batting average based on exit velocity and launch angle of batted balls. Strips out fielding and luck — shows true contact quality.

- **Aliases:** xBA
- **Where:** `statcast_seasons.xba`
- **Prompt:** no

### Expected ERA (`xera`) [stat]

Statcast-derived expected ERA based on quality of contact allowed and K/BB rates. Predicts future ERA better than raw ERA.

- **Aliases:** xERA
- **Where:** `statcast_seasons.xera`
- **Prompt:** no

### Expected FIP (`xfip`) [stat]

FIP with home runs normalized to league-average HR/FB rate. Removes HR luck — a more stable pitcher evaluation than FIP.

- **Aliases:** xFIP
- **Where:** `statcast_seasons.xfip`
- **Prompt:** no

### Expected OBP (`xobp`) [stat]

Statcast-derived expected on-base percentage based on batted ball quality and plate discipline outcomes. Stored alongside other expected metrics.

- **Aliases:** xOBP
- **Where:** `statcast_seasons.xobp`
- **Prompt:** no

### Expected Slugging (`xslg`) [stat]

Statcast-derived expected slugging percentage based on batted ball quality. Higher = more expected power production.

- **Aliases:** xSLG
- **Where:** `statcast_seasons.xslg`
- **Prompt:** no

### Expected wOBA (`xwoba`) [stat]

Statcast-derived expected weighted on-base average. The single best all-around offensive quality metric from Statcast. Combines contact quality with plate discipline outcomes.

- **Aliases:** xwOBA
- **Where:** `statcast_seasons.xwoba`
- **Prompt:** no

### FIP (`fip`) [stat]

Fielding Independent Pitching: `(13*HR + 3*(BB+HBP) - 2*K) / IP + cFIP`. Isolates what a pitcher controls (HR, BB, K) from fielding. Lower is better.

- **Aliases:** Fielding Independent Pitching
- **Where:** `mlbam_season_stats.fip`
- **Prompt:** no

### FIP Constant (`cfip`) [stat]

League-level constant that aligns FIP to the league ERA scale. Fetched from FanGraphs Guts! page; falls back to 3.17 on failure.

- **Where:** `internal/store/fip.go` — `FIPConstant`
- **Prompt:** no

### Fastball Velocity (`fastball_velo`) [stat]

Average velocity of a pitcher's fastball in miles per hour. A raw stuff indicator — harder throwers generate more swings and misses. EB-blended in PQS computation (pitcher signal, 0.15 weight).

- **Aliases:** FastballV, Fastball Velo
- **Where:** `statcast_seasons.fastball_velo`; `internal/display/table.go`
- **Prompt:** no

### Fly Ball% (`fb_pct`) [stat]

Percentage of batted balls that are fly balls. For hitters, higher FB% combined with high HR/FB = power profile. For pitchers, higher FB% = more HR risk.

- **Aliases:** FB%
- **Where:** `statcast_seasons.fb_pct`
- **Prompt:** no

### Ground Ball% (`gb_pct`) [stat]

Percentage of batted balls that are ground balls. For pitchers, higher = fewer home runs allowed. ERA suppression signal.

- **Aliases:** GB%
- **Where:** `statcast_seasons.gb_pct`
- **Prompt:** no

### Hard Hit% (`hard_hit_pct`) [stat]

Percentage of batted balls with exit velocity >= 95 mph. For hitters, higher = consistent power. For pitchers, lower = better contact suppression.

- **Aliases:** Hard%
- **Where:** `statcast_seasons.hard_hit_pct`
- **Prompt:** no

### Home Run (`hr`) [stat]

A hit where the batter rounds all bases and scores. A standard Yahoo hitting category (counting stat).

- **Aliases:** HR
- **Prompt:** no

### HR/FB (`hr_fb`) [stat]

Home run per fly ball ratio. Measures how efficiently a hitter converts fly balls into home runs. Used as a PQS signal alongside FB%.

- **Aliases:** HR/FB ratio
- **Where:** `statcast_seasons.hr_fb_pct`
- **Prompt:** no

### K% (`k_pct`) [stat]

Strikeout rate: strikeouts divided by plate appearances (hitters) or batters faced (pitchers). For hitters, lower is better (contact ability, AVG floor). For pitchers, higher is better (dominance).

- **Aliases:** Strikeout Rate
- **Prompt:** no

### K-BB% (`k_bb_pct`) [stat]

Strikeout rate minus walk rate as a percentage of batters faced. Composite command + stuff metric for pitchers. Higher is better.

- **Where:** Computed in `pqs.go` from `(K - BB) / BF`
- **Prompt:** no

### K/9 (`k9`) [stat]

Strikeouts per 9 innings pitched: `(K / IP) * 9`. Volume-adjusted strikeout rate.

- **Aliases:** K/9
- **Prompt:** no

### Launch Angle (`launch_angle`) [stat]

Average angle of the ball off the bat in degrees. Higher launch angles produce more fly balls and home runs; ground balls are near 0°.

- **Aliases:** LA, Launch°
- **Where:** `statcast_seasons.launch_angle_avg`
- **Prompt:** no

### OBP (`obp`) [stat]

On-Base Percentage: `(H + BB + HBP) / (AB + BB + HBP + SF)`. Measures how often a hitter reaches base.

- **Aliases:** On-Base Percentage
- **Prompt:** no

### OPS (`ops`) [stat]

On-Base Plus Slugging: OBP + SLG. Quick composite offensive value metric.

- **Prompt:** no

### Runs (`r`) [stat]

Times a player crosses home plate to score. A standard Yahoo hitting category (counting stat).

- **Aliases:** R
- **Prompt:** no

### RBI (`rbi`) [stat]

Runs Batted In — runs that score as a direct result of the batter's action. A standard Yahoo hitting category (counting stat).

- **Aliases:** RBI
- **Prompt:** no

### SLG (`slg`) [stat]

Slugging Percentage: total bases divided by at-bats. Measures raw power — higher = more extra-base hits.

- **Aliases:** Slugging
- **Prompt:** no

### Spin Rate (`spin_rate`) [stat]

Average spin rate on fastballs in revolutions per minute (RPM). Higher spin rate correlates with more swing-and-miss on elevated fastballs.

- **Where:** `statcast_seasons.spin_rate`
- **Prompt:** no

### Sprint Speed (`sprint_speed`) [stat]

Savant-measured speed in feet per second, based on a player's fastest competitive runs. The primary speed and stolen base potential signal.

- **Aliases:** Spd, SB Spd
- **Where:** `statcast_seasons.sprint_speed`
- **Prompt:** no

### Stolen Base (`sb`) [stat]

Baserunner advances a base without a hit, error, or walk. A standard Yahoo hitting category (counting stat).

- **Aliases:** SB
- **Prompt:** no

### Strikeout (`k`) [stat]

Batter fails to put the ball in play after three strikes, or pitcher records an out via three strikes. A standard Yahoo pitching category (counting stat, displayed as K or SO).

- **Aliases:** K, SO
- **Prompt:** no

### Sweet Spot% (`sweet_spot_pct`) [stat]

Percentage of batted balls in the 8-32 degree launch angle range — the zone that produces the highest batting average. Correlates with AVG and SLG stability.

- **Aliases:** Sweet%
- **Where:** `statcast_seasons.sweet_spot_pct`
- **Prompt:** no

### Walks (`bb`) [stat]

Batter awarded first base after four balls (hitter stat) or pitcher issues a base on balls (pitcher stat). Part of BB% and WHIP calculations.

- **Aliases:** BB, Base on Balls
- **Prompt:** no

### WAR (`fwar`) [stat]

Wins Above Replacement. Composite metric estimating total player value in wins compared to a replacement-level player. The pinned Go baseline displayed FanGraphs fWAR; b9 does not acquire or display it while automated FanGraphs access remains rejected.

- **Aliases:** fWAR
- **Where:** `players.fangraphs_war`
- **Prompt:** no

### WHIP (`whip`) [stat]

Walks + Hits per Innings Pitched: `(BB + H) / IP`. A standard Yahoo pitching category. Lower is better.

- **Aliases:** Walks + Hits per IP
- **Prompt:** yes

### Whiff% (`whiff_pct`) [stat]

Percentage of swings that result in a miss. The best single strikeout predictor — higher = more Ks. FanGraphs-sourced, EB-blended in PQS.

- **Aliases:** Whiff Rate
- **Where:** `statcast_seasons.whiff_pct`
- **Prompt:** no

### Wins (`w`) [stat]

Pitcher of record when their team takes the lead and holds it. A standard Yahoo pitching category (counting stat).

- **Aliases:** W
- **Prompt:** no

### wRC+ (`wrc_plus`) [stat]

Weighted Runs Created Plus. FanGraphs metric where 100 = league average. Adjusts for park and league. Higher = better offensive production.

- **Where:** `players.wrc_plus`
- **Prompt:** no

---

## b9 Signals

### Blend Window (`blend_window`) [b9]

Season-phase-based weighting of current-season, prior-season, and spring training data for PQS computation. Transitions from prior-heavy early in the season to current-only once league games played reaches 28.

- **Where:** `internal/analysis/stat_weights.go` — `ComputeStatWeights`
- **Prompt:** no

### Category Strategy (`category_strategy`) [b9]

Deterministic classification of each scoring category as push, protect, or abandon. Computed from `CategoryGap` status — not LLM-generated. The LLM echoes this classification verbatim.

- **Where:** `internal/advisory/payload.go` — `ComputeCategoryStrategy`
- **Prompt:** yes

### Closer (`closer`) [b9]

The designated closer for each MLB team. The pinned Go baseline used FanGraphs RosterResource tags with an SV-leader fallback and displayed `RP1`. b9 retains only deterministic data available from approved providers; FanGraphs enrichment and the PQS multiplier remain deferred.

- **Where:** `internal/store/player.go` — `MarkClosers`
- **Prompt:** no

### Empirical-Bayes Blending (`empirical_bayes`) [b9]

Stabilization method for Statcast metrics: `blended = w * current + (1 - w) * prior`, where `w = sample / (sample + k)`. Each metric has its own k-value. Higher k = more regression to prior. Applied in `BlendStatcast`.

- **Where:** `internal/analysis/statcast_blend.go`
- **Prompt:** no

### Opportunity Damping (`opportunity_damping`) [b9]

Current-season weight in PQS blend is scaled by `min(PA/150, 1)` for hitters and `min(IP/40, 1)` for pitchers. Prevents small-sample current stats from dominating the blend early in the season.

- **Where:** `internal/analysis/stat_weights.go` — `OpportunityDampen`
- **Prompt:** no

### Pitcher Day State (`pitcher_day_state`) [b9]

Classification of a pitcher's same-day availability: `confirmed_sp` (RotoWire confirmed), `probable_sp` (MLBAM probable), `rp_available` (RP, team has game), `not_scheduled` (SP-eligible, not today's starter), `no_game` (team off). Determines lineup candidate eligibility.

- **Where:** `internal/advisory/lineup.go` — `pitcherDayState`
- **Prompt:** no

### Pool (`pool`) [b9]

The set of active MLB players used as the normalization baseline for z-scoring in PQS computation. Defined as the `pqsMap` keys from `ComputePQS` — every player with sufficient signal data.

- **Prompt:** no

### POS Column (`pos`) [b9]

Display rendering of a player's Yahoo eligibility positions. Width-5 cell: if the comma-joined literal form fits (≤ 5 visible chars), render it as-is (`SP,RP`, `C,1B`, `OF`, `Util`, `RP*`). Otherwise compress to single-letter codes in defensive-value order: `C, 1B, 2B, 3B, SS, OF, SP, RP, Util` → `C, 1, 2, 3, S, O, P, R, U`. Cap compressed form at 5 letters; the super-rare 6-position monster (C + every hitter position) renders as `All`. Closers carry a trailing `*` (`RP*` literal, `R*` / `PR*` compressed). The MLBAM-only fallback (Yahoo eligibility absent) prepends `*` to the cell — distinct semantic from the closer marker, same glyph.

- **Aliases:** position, positions, POS
- **Where:** `internal/display/poscell.go` — `compressPositions`
- **Prompt:** yes

### Projected Production (`pp`) [b9]

0-100 score derived from blended rest-of-season projections (Steamer 0.40, ZiPS 0.35, ATC 0.25). Represents forecasted fantasy category production, not talent. Hitter PP: z-scored HR, R, RBI, SB with PA-damped AVG. Pitcher PP: z-scored inverted ERA/WHIP (IP-damped) plus K, W, SV.

- **Aliases:** PP
- **Where:** `internal/advisory/pp.go` — `ComputePP`
- **Prompt:** yes

### Stabilization Ramp (`stabilization_ramp`) [b9]

Signal weight scaled by `min(1.0, sample / threshold)` in PQS computation. Signals with insufficient sample size contribute less to the score. EB-blended signals use threshold=1 (already stabilized); raw signals use real thresholds (e.g., 50 PA for K%).

- **Where:** `internal/analysis/pqs.go`
- **Prompt:** no

### Steamer (`steamer`) [b9]

FanGraphs rest-of-season projection system. Weight 0.40 in PP blend. Provides projected counting and rate stats.

- **Prompt:** no

### Player Quality Score (`pqs`) [b9]

Internal quality-based model using stabilized skill signals. Not displayed to users. Hitter signals: xwOBA (0.30), K% (0.15), BB% (0.10), Sprint Speed (0.20), FB% (0.10), HR/FB (0.15). Pitcher signals: Whiff% (0.30), Chase% (0.20), GB% (0.15), Fastball Velo (0.15), K-BB% (0.20). Each signal z-scored against the player pool, clamped ±2.0, weighted, summed. Category emphasis and context multipliers applied. Stored in `players.pqs`. Feeds waiver ranking and the browse sort tiebreaker.

- **Aliases:** PQS, TS, TalentScore (legacy)
- **Where:** `internal/analysis/pqs.go` — `ComputePQS`
- **Prompt:** yes

### Z-Score (`z_score`) [b9]

`(value - pool_mean) / pool_stddev`. Per-signal z-score clamped to ±2.0 in PQS computation.

- **Prompt:** no

### ZiPS (`zips`) [b9]

Baseball Think Factory rest-of-season projection system. Weight 0.35 in PP blend.

- **Prompt:** no

### ATC (`atc`) [b9]

Average Total Cost — rest-of-season projection system. Weight 0.25 in PP blend.

- **Aliases:** Air and Time Coach
- **Prompt:** no

### Yahoo Players (`yp`) [b9]

Per-MLB-team count of players whose joined Yahoo `Owner` is non-empty — i.e. they currently occupy a roster slot (active, BN, IL, or NA) on one of the fantasy teams in the user's league. Two-way players count once per MLB team. Displayed as a dark-gray integer column in `b9 tt`, between `GB` and `PA`.

- **Aliases:** YP
- **Where:** `cmd/skout/teams_totals.go` — `countYahooRosteredOnMLBTeam`
- **Prompt:** no

### Age (`age`) [b9]

Player age in whole years, derived at render time from the MLBAM `birthDate` stored in `players.birth_date`. Reduced by one if the birthday has not yet occurred in the current calendar year. Renders `-` when `birth_date` is NULL (no MLBAM identity yet, or never fetched). Used in the `b9 h <name>` and `b9 p <name>` detail card identity headers.

- **Where:** `internal/domain/player.go` — `Player.Age(now)`
- **Prompt:** no

### AVG162G (`avg162g`) [b9]

The Baseball-Reference-style 162-game pace row at the top of the SPLIT table on the player detail card (AC137). Aggregates counting stats across the rolling window of completed seasons (current season excluded), then scales them by `162 / sum_games` — i.e. what this player produces per 162 of his own games at his historical play rate, not per 162 calendar games. Rate stats (AVG/OBP/OPS for hitters; ERA/WHIP for pitchers) are recomputed cumulatively from the summed counting fields, unaffected by the scale. With zero completed-season games, every cell renders as `-`.

- **Where:** `internal/display/playercard.go` — `avg162GHitterCells`, `avg162GPitcherCells`
- **Prompt:** no

### GAME LOG (`game-log`) [b9]

The per-day (hitter) or per-appearance (pitcher) section below the SPLIT table on the player detail card (AC137). Hitter rows walk the last 10 calendar days, using the player's MLB team schedule + boxscore lineup to surface non-appearance days; the indicator column shows a green batting-order digit when the player started, a red `X` when the team played but he didn't appear, and a blank when the team had no game. Pitcher rows show the last 10 appearances filtered by `mlb.ParseIP(InningsPitched) > 0` so gaps stay invisible; the indicator is a green `●`.

- **Where:** `cmd/skout/playercard_gamelog.go` — `buildHitterGameLog`, `buildPitcherGameLog`; `internal/display/playercard_gamelog.go` — `printHitterGameLog`, `printPitcherGameLog`, `hitterGameLogCells`, `pitcherGameLogCells`
- **Prompt:** no

### Savant (`savant`) [b9]

The literal SOURCE-column label on both hitter and pitcher detail-card Statcast rows. The label is kept for visual consistency with the AC mockup even though some pitcher cells (WHIFF%, CH%, GB%) are FanGraphs-derived rather than strictly Baseball Savant. Treat the label as a display convention, not a strict provenance claim.

- **Where:** `internal/display/playercard.go` — `RenderHitterCard`, `RenderPitcherCard`
- **Prompt:** no
