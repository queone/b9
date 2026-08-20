# b9 Glossary

Canonical definitions for baseball, fantasy, and b9-specific terms. This file is embedded in the Rust binary and powers `b9 i`. API and architecture documents may add implementation detail but must not contradict these definitions.

Definitions include both implemented b9 concepts and general fantasy-baseball vocabulary useful when interpreting its data. A definition alone does not claim that b9 currently computes or displays the term. A **Where** field, when present, names the current Rust implementation or durable schema location.

When a change introduces or redefines a domain term, update this file in the same pass.

---

## Coverage Checklist

Every key below must have a corresponding entry in this glossary. When a new stat or signal is added to b9, add it here.

`ab`, `abandon`, `active`, `age`, `atc`, `avg`, `available`, `babip`, `barrel_pct`, `batters_faced`, `bb`, `bb_pct`, `bench`, `blend_window`, `category_strategy`, `cfip`, `ch_pct`, `close`, `closer`, `confirmed`, `confirmed_sp`, `cs`, `dtd`, `ecr`, `empirical_bayes`, `era`, `exit_velo`, `expected`, `faab`, `fastball_velo`, `fb_pct`, `fip`, `flippable`, `fwar`, `g`, `gb_pct`, `gs`, `h2h`, `hard_hit_pct`, `hbp`, `holds`, `hr`, `hr_fb`, `il`, `injured`, `ip`, `k`, `k_bb_pct`, `k_pct`, `k9`, `last03ip`, `last10ip`, `last20pa`, `last5yrs`, `launch_angle`, `lineup_candidates`, `lineup_status`, `lost`, `na`, `next03ip`, `next10ip`, `next20pa`, `no_game`, `not_scheduled`, `obp`, `ops`, `opportunity_damping`, `out`, `own_pct`, `p_slot`, `pa`, `pitcher_day_state`, `pool`, `pos`, `pp`, `ppd`, `probable`, `probable_sp`, `protect`, `punt`, `push`, `qs`, `r`, `rbi`, `replacement_level`, `roster_moves`, `roster_moves_note`, `roster_slot`, `rp_available`, `rp_slot`, `sb`, `savant`, `slg`, `sp_slot`, `spin_rate`, `sprint_speed`, `stabilization_ramp`, `steamer`, `streaming`, `sv`, `sweet_spot_pct`, `tied`, `pqs`, `w`, `waiver_wire`, `whiff_pct`, `whip`, `wrc_plus`, `xba`, `xera`, `xfip`, `xobp`, `xslg`, `xwoba`, `yp`, `yr`, `z_score`, `zips`

---

## Baseball

### Active (`active`) [baseball]

Player status: in a starting lineup slot (C, 1B, 2B, 3B, SS, OF, Util, SP, RP). Not on the bench or injured list.

- **Where:** `src/domain.rs` — `Position`

### At-Bat (`ab`) [baseball]

A plate appearance that results in a hit, out, fielder's choice, or error — excludes walks, HBP, sacrifices, and catcher interference. The denominator for batting average.

- **Aliases:** AB

### Batters Faced (`batters_faced`) [baseball]

Total plate appearances against a pitcher. The pitching equivalent of PA; denominator for K% and BB% on the pitching side.

- **Aliases:** BF

### Bench (`bench`) [baseball]

Player status: rostered in a BN (bench) slot. Not in the active lineup but available for activation.

- **Where:** `src/domain.rs` — `Position::Bench`

### Caught Stealing (`cs`) [baseball]

Baserunner thrown out attempting to steal a base.

- **Aliases:** CS

### Day-to-Day (`dtd`) [baseball]

Injury status: player is nursing a minor injury but has not been placed on the injured list. May or may not play on any given day.

- **Aliases:** DTD

### Games (`g`) [baseball]

Total games in which a player appeared (hitters or pitchers).

- **Aliases:** G

### Games Started (`gs`) [baseball]

Games in which a pitcher was the starting pitcher. Used to classify SP vs RP (`GS * 2 >= G` → SP).

- **Aliases:** GS

### Hit By Pitch (`hbp`) [baseball]

Batter awarded first base after being struck by a pitch. Counts as a plate appearance but not an at-bat.

- **Aliases:** HBP

### Holds (`holds`) [baseball]

Relief pitcher enters with a lead of 3 or fewer runs (or tying run on base/at bat/on deck), records at least one out, and leaves without relinquishing the lead. Not a standard Yahoo scoring category in most leagues.

- **Aliases:** HLD

### Injured List (`il`) [baseball]

MLB designation for players unable to play due to injury. Variants: IL10 (10-day), IL15 (15-day), IL60 (60-day). In fantasy, rostering a player on IL frees an active roster spot.

- **Aliases:** IL, IL10, IL15, IL60, DL (legacy)

### Innings Pitched (`ip`) [baseball]

Outs recorded divided by 3. MLBAM notation: `7.1` = 7⅓ innings (7 full innings + 1 out), `7.2` = 7⅔ innings.

- **Aliases:** IP

### Not Active (`na`) [baseball]

Status for players not on the MLB active roster — typically minor league or restricted list players. In Yahoo fantasy, these players occupy an NA slot.

- **Aliases:** NA

### Plate Appearance (`pa`) [baseball]

Any completed turn at bat — includes hits, outs, walks, HBP, sacrifices. The most inclusive batting denominator.

- **Aliases:** PA

### Quality Start (`qs`) [baseball]

Starting pitcher completes at least 6 innings with 3 or fewer earned runs allowed.

- **Aliases:** QS

### Save Opportunity (`sv`) [baseball]

Closer enters with a lead of 3 or fewer runs (or tying run on base/at bat/on deck) and finishes the game preserving the lead. A standard Yahoo pitching category.

- **Aliases:** SV, Save

---

## Fantasy

### Abandon (`abandon`) [fantasy]

General category-strategy classification: a category is conceded because its gap is impractical to close or the manager has chosen to punt it. b9 does not currently emit automated abandon recommendations.

### Available (`available`) [fantasy]

Lineup status for a relief pitcher whose team has a game today. The RP may pitch but has no scheduled start.


### Close (`close`) [fantasy]

Category gap status: user is ahead but the margin is thin enough to flip. Treated as a protect-priority category.


### Confirmed (`confirmed`) [fantasy]

Lineup status: hitter verified in today's batting order (from RotoWire or MLBAM confirmed lineups), or pitcher confirmed as today's starter (RotoWire).

- **Where:** `src/player_commands.rs`; `src/providers/rotowire.rs`

### Confirmed SP (`confirmed_sp`) [fantasy]

Pitcher day state: RotoWire has confirmed this pitcher as today's starting pitcher. Highest confidence level for SP lineup decisions.


### Expert Consensus Rank (`ecr`) [fantasy]

Aggregate ranking from FantasyPros combining multiple expert rankings. Lower is better. Displayed as the `ECR` column.

- **Aliases:** ECR, CR
- **Where:** `src/providers/fantasypros.rs`; `src/player_display.rs`

### Expected (`expected`) [fantasy]

Lineup status: hitter whose team has a game today but the batting order has not been posted yet. Default assumption is the player will play.


### FAAB (`faab`) [fantasy]

Free Agent Acquisition Budget. Fixed dollar amount each team can bid on waiver claims over the season. Tracked in `yahoo_leagues.faab_budget`.

- **Aliases:** FAAB budget

### Flippable (`flippable`) [fantasy]

A category where the gap between user and opponent is small enough that roster decisions this week could change the outcome. Categories with status `behind`, `tied`, or `close` are flippable (unless punted).


### Head-to-Head Categories (`h2h`) [fantasy]

League format where two teams compete each week across scoring categories. Each category is a win, loss, or tie. Standard Yahoo format: 5 hitting (R, HR, RBI, SB, AVG) + 5 pitching (W, SV, K, ERA, WHIP).

- **Aliases:** H2H

### Injured (`injured`) [fantasy]

Player status: rostered in an IL (Injured List) slot. Cannot be placed in active lineup slots.


### Lineup Status (`lineup_status`) [fantasy]

General classification of same-day availability using values such as `confirmed`, `probable`, `expected`, `out`, `not_scheduled`, `no_game`, and `available`. b9 renders the underlying lineup, probable-pitcher, injury, schedule, and game-state facts rather than emitting this classification as a separate advisory payload.

### Lineup Candidates (`lineup_candidates`) [fantasy]

General decision-support term for position-eligible bench players who could replace active players. b9 currently presents roster and game-state facts but does not emit an automated swap list.

### Lost (`lost`) [fantasy]

Category gap status: the gap is too large to realistically close given remaining games. Treated as abandon unless the user overrides.


### No Game (`no_game`) [fantasy]

Lineup status: player's MLB team has no game scheduled today.


### Not Scheduled (`not_scheduled`) [fantasy]

Lineup status for an SP-eligible pitcher: their team has a game today but this pitcher is not the scheduled starter.


### Out (`out`) [fantasy]

Lineup status: today's batting order has been confirmed and this hitter is not in it.


### Ownership Percentage (`own_pct`) [fantasy]

Percentage of Yahoo leagues in which a player is rostered. Displayed as `%OWN`.

- **Aliases:** %OWN, percent_owned
- **Where:** `src/player_display.rs`

### Probable (`probable`) [fantasy]

Lineup status for a pitcher listed as MLBAM probable starter but not yet confirmed by RotoWire.


### Postponed / PPD (`ppd`) [fantasy]

Game postponed due to weather or another cause. A confirmed postponement means players from that game will not accrue statistics unless the game is rescheduled and played within the scoring period.

- **Aliases:** PPD, rainout

### Probable SP (`probable_sp`) [fantasy]

Pitcher day state: MLBAM lists this pitcher as the probable starter but RotoWire has not yet confirmed. Lower confidence than `confirmed_sp` but still a strong signal.


### Protect (`protect`) [fantasy]

Category strategy classification: user is ahead or close in this category (and it's not punted). Priority is risk mitigation — avoid losing ground.


### Punt (`punt`) [fantasy]

Deliberate decision to concede a scoring category and redirect roster resources to other categories. This is a manager policy choice; b9 does not currently store a strategy configuration.

- **Aliases:** punting

### Push (`push`) [fantasy]

Category strategy classification: user is behind or tied in this category (and it's not punted). Priority is gaining ground — maximize production in this category.


### Replacement Level (`replacement_level`) [fantasy]

The talent level of the best freely available player at a position. In a 12-team league, roughly the 12th-best player at each position. Used as the baseline for positional scarcity adjustments in PQS.


### Roster Moves Note (`roster_moves_note`) [fantasy]

General decision-context note describing constraints such as remaining weekly adds or FAAB budget. b9 does not currently generate an advisory payload or automated pickup/drop recommendation.

### Roster Slot (`roster_slot`) [fantasy]

A lineup position in Yahoo Fantasy where a player is placed. Distinct from player position eligibility — a player eligible at SP and RP can occupy an SP slot, RP slot, P slot, or BN. Each league defines how many of each slot type exist. Slot types: C, 1B, 2B, 3B, SS, OF, Util (any hitter), SP, RP, P (any pitcher), BN (bench), IL (injured list).

- **Aliases:** slot, lineup slot

### SP Slot (`sp_slot`) [fantasy]

Yahoo roster slot reserved for starting pitchers. Only players with SP eligibility can be placed here. The number of SP slots is league-configured (typically 2). A pitcher in an SP slot is "active" regardless of whether they are actually starting today.

- **Avoid:** confusing SP slot (roster placement) with SP role (pitcher who starts games)

### RP Slot (`rp_slot`) [fantasy]

Yahoo roster slot reserved for relief pitchers. Only players with RP eligibility can be placed here. The number of RP slots is league-configured (typically 2).

- **Avoid:** confusing RP slot (roster placement) with RP role (pitcher who relieves)

### P Slot (`p_slot`) [fantasy]

Yahoo roster slot that accepts any pitcher (SP or RP eligible). Provides overflow capacity beyond dedicated SP and RP slots. The number of P slots is league-configured (typically 2).


### RP Available (`rp_available`) [fantasy]

Pitcher day state: relief pitcher whose team has a game today. Eligible to pitch but has no scheduled start — availability is implicit.


### Roster Moves (`roster_moves`) [fantasy]

General decision-support term for pickup/drop candidates ranked by category fit. b9 currently ranks waiver candidates but does not emit paired roster-move recommendations.

### Streaming (`streaming`) [fantasy]

Strategy of frequently adding and dropping pitchers, usually starters, to maximize counting statistics by targeting favorable matchups.


### Tied (`tied`) [fantasy]

Category gap status: user and opponent have the same score in this category.


### Waiver Wire (`waiver_wire`) [fantasy]

The pool of unrostered players available for pickup. In FAAB leagues, claims require a dollar bid.

- **Aliases:** FA, free agents

### Yahoo Rank (`yr`) [fantasy]

Yahoo's pre-calculated overall player rank for the season. Lower = better. Displayed as `YR` column.

- **Aliases:** YR
- **Where:** `src/player_display.rs`

---

## Stats

### Batting Average (`avg`) [stat]

Hits divided by at-bats (H/AB). A standard Yahoo hitting category. Rate stat — not scaled by volume.

- **Aliases:** AVG, BA

### BB% (`bb_pct`) [stat]

Walk rate: walks divided by plate appearances (hitters) or batters faced (pitchers). Higher is better for hitters (discipline); lower is better for pitchers (command).

- **Aliases:** Walk Rate

### BABIP (`babip`) [stat]

Batting Average on Balls In Play: `(H - HR) / (AB - K - HR + SF)`. Measures how often batted balls (excluding home runs) fall for hits. Useful for identifying luck-driven AVG spikes or slumps — league average is roughly .300.

- **Aliases:** Batting Average on Balls In Play
- **Where:** `mlbam_season_stats.babip`

### Barrel% (`barrel_pct`) [stat]

Percentage of batted ball events in the "barrel" zone — optimal combination of exit velocity and launch angle that produces extra-base hits at an elite rate. Best single power/HR signal from Statcast.

- **Where:** `statcast_seasons.barrel_pct`

### Chase% (`ch_pct`) [stat]

Percentage of pitches outside the strike zone that the batter swings at. For pitchers, higher = better (inducing swings on bad pitches). A command and swing-and-miss proxy.

- **Aliases:** O-Swing%, Chase Rate
- **Where:** `statcast_seasons.chase_pct`

### Earned Run Average (`era`) [stat]

Earned runs allowed per 9 innings pitched: `(ER / IP) * 9`. A standard Yahoo pitching category. Lower is better.

- **Aliases:** ERA

### Exit Velocity (`exit_velo`) [stat]

Average speed of the ball off the bat in miles per hour. Higher exit velocity correlates with more extra-base hits and home runs.

- **Aliases:** EV, Exit Velo
- **Where:** `statcast_seasons.exit_velo_avg`

### Expected Batting Average (`xba`) [stat]

Statcast-derived expected batting average based on exit velocity and launch angle of batted balls. Strips out fielding and luck — shows true contact quality.

- **Aliases:** xBA
- **Where:** `statcast_seasons.xba`

### Expected ERA (`xera`) [stat]

Statcast-derived expected ERA based on quality of contact allowed and K/BB rates. Predicts future ERA better than raw ERA.

- **Aliases:** xERA
- **Where:** `statcast_seasons.xera`

### Expected FIP (`xfip`) [stat]

FIP with home runs normalized to league-average HR/FB rate. Removes HR luck — a more stable pitcher evaluation than FIP.

- **Aliases:** xFIP
- **Where:** `statcast_seasons.xfip`

### Expected OBP (`xobp`) [stat]

Statcast-derived expected on-base percentage based on batted ball quality and plate discipline outcomes. Stored alongside other expected metrics.

- **Aliases:** xOBP
- **Where:** `statcast_seasons.xobp`

### Expected Slugging (`xslg`) [stat]

Statcast-derived expected slugging percentage based on batted ball quality. Higher = more expected power production.

- **Aliases:** xSLG
- **Where:** `statcast_seasons.xslg`

### Expected wOBA (`xwoba`) [stat]

Statcast-derived expected weighted on-base average. The single best all-around offensive quality metric from Statcast. Combines contact quality with plate discipline outcomes.

- **Aliases:** xwOBA
- **Where:** `statcast_seasons.xwoba`

### FIP (`fip`) [stat]

Fielding Independent Pitching: `(13*HR + 3*(BB+HBP) - 2*K) / IP + cFIP`. Isolates what a pitcher controls (HR, BB, K) from fielding. Lower is better.

- **Aliases:** Fielding Independent Pitching
- **Where:** `mlbam_season_stats.fip`

### FIP Constant (`cfip`) [stat]

League-level constant that aligns FIP to the league ERA scale. b9 retains FIP-related schema fields but does not currently fetch or expose a cFIP value.

### Fastball Velocity (`fastball_velo`) [stat]

Average velocity of a pitcher's fastball in miles per hour. A raw stuff indicator — harder throwers generate more swings and misses. EB-blended in PQS computation (pitcher signal, 0.15 weight).

- **Aliases:** FastballV, Fastball Velo
- **Where:** `statcast_seasons.fastball_velo`; `src/player_display.rs`

### Fly Ball% (`fb_pct`) [stat]

Percentage of batted balls that are fly balls. For hitters, higher FB% combined with high HR/FB = power profile. For pitchers, higher FB% = more HR risk.

- **Aliases:** FB%
- **Where:** `statcast_seasons.fb_pct`

### Ground Ball% (`gb_pct`) [stat]

Percentage of batted balls that are ground balls. For pitchers, higher = fewer home runs allowed. ERA suppression signal.

- **Aliases:** GB%
- **Where:** `statcast_seasons.gb_pct`

### Hard Hit% (`hard_hit_pct`) [stat]

Percentage of batted balls with exit velocity >= 95 mph. For hitters, higher = consistent power. For pitchers, lower = better contact suppression.

- **Aliases:** Hard%
- **Where:** `statcast_seasons.hard_hit_pct`

### Home Run (`hr`) [stat]

A hit where the batter rounds all bases and scores. A standard Yahoo hitting category (counting stat).

- **Aliases:** HR

### HR/FB (`hr_fb`) [stat]

Home run per fly ball ratio. Measures how efficiently a hitter converts fly balls into home runs. Used as a PQS signal alongside FB%.

- **Aliases:** HR/FB ratio
- **Where:** `statcast_seasons.hr_fb_pct`

### K% (`k_pct`) [stat]

Strikeout rate: strikeouts divided by plate appearances (hitters) or batters faced (pitchers). For hitters, lower is better (contact ability, AVG floor). For pitchers, higher is better (dominance).

- **Aliases:** Strikeout Rate

### K-BB% (`k_bb_pct`) [stat]

Strikeout rate minus walk rate as a percentage of batters faced. Composite command + stuff metric for pitchers. Higher is better.

- **Where:** `src/analysis/pqs.rs`

### K/9 (`k9`) [stat]

Strikeouts per 9 innings pitched: `(K / IP) * 9`. Volume-adjusted strikeout rate.

- **Aliases:** K/9

### Launch Angle (`launch_angle`) [stat]

Average angle of the ball off the bat in degrees. Higher launch angles produce more fly balls and home runs; ground balls are near 0°.

- **Aliases:** LA, Launch°
- **Where:** `statcast_seasons.launch_angle_avg`

### OBP (`obp`) [stat]

On-Base Percentage: `(H + BB + HBP) / (AB + BB + HBP + SF)`. Measures how often a hitter reaches base.

- **Aliases:** On-Base Percentage

### OPS (`ops`) [stat]

On-Base Plus Slugging: OBP + SLG. Quick composite offensive value metric.


### Runs (`r`) [stat]

Times a player crosses home plate to score. A standard Yahoo hitting category (counting stat).

- **Aliases:** R

### RBI (`rbi`) [stat]

Runs Batted In — runs that score as a direct result of the batter's action. A standard Yahoo hitting category (counting stat).

- **Aliases:** RBI

### SLG (`slg`) [stat]

Slugging Percentage: total bases divided by at-bats. Measures raw power — higher = more extra-base hits.

- **Aliases:** Slugging

### Spin Rate (`spin_rate`) [stat]

Average spin rate on fastballs in revolutions per minute (RPM). Higher spin rate correlates with more swing-and-miss on elevated fastballs.

- **Where:** `statcast_seasons.spin_rate`

### Sprint Speed (`sprint_speed`) [stat]

Savant-measured speed in feet per second, based on a player's fastest competitive runs. The primary speed and stolen base potential signal.

- **Aliases:** Spd, SB Spd
- **Where:** `statcast_seasons.sprint_speed`

### Stolen Base (`sb`) [stat]

Baserunner advances a base without a hit, error, or walk. A standard Yahoo hitting category (counting stat).

- **Aliases:** SB

### Strikeout (`k`) [stat]

Batter fails to put the ball in play after three strikes, or pitcher records an out via three strikes. A standard Yahoo pitching category (counting stat, displayed as K or SO).

- **Aliases:** K, SO

### Sweet Spot% (`sweet_spot_pct`) [stat]

Percentage of batted balls in the 8-32 degree launch angle range — the zone that produces the highest batting average. Correlates with AVG and SLG stability.

- **Aliases:** Sweet%
- **Where:** `statcast_seasons.sweet_spot_pct`

### Walks (`bb`) [stat]

Batter awarded first base after four balls (hitter stat) or pitcher issues a base on balls (pitcher stat). Part of BB% and WHIP calculations.

- **Aliases:** BB, Base on Balls

### WAR (`fwar`) [stat]

Wins Above Replacement. Composite metric estimating total player value in wins compared to a replacement-level player. The pinned Go baseline displayed FanGraphs fWAR; b9 does not acquire or display it while automated FanGraphs access remains rejected.

- **Aliases:** fWAR
- **Where:** `players.fangraphs_war`

### WHIP (`whip`) [stat]

Walks + Hits per Innings Pitched: `(BB + H) / IP`. A standard Yahoo pitching category. Lower is better.

- **Aliases:** Walks + Hits per IP

### Whiff% (`whiff_pct`) [stat]

Percentage of swings that result in a miss. The best single strikeout predictor — higher = more Ks. FanGraphs-sourced, EB-blended in PQS.

- **Aliases:** Whiff Rate
- **Where:** `statcast_seasons.whiff_pct`

### Wins (`w`) [stat]

Pitcher of record when their team takes the lead and holds it. A standard Yahoo pitching category (counting stat).

- **Aliases:** W

### wRC+ (`wrc_plus`) [stat]

Weighted Runs Created Plus. FanGraphs metric where 100 = league average. Adjusts for park and league. Higher = better offensive production.

- **Where:** `players.wrc_plus`

---

## b9 Signals

### Blend Window (`blend_window`) [b9]

Season-phase-based weighting of current-season, prior-season, and spring training data for PQS computation. Transitions from prior-heavy early in the season to current-only once league games played reaches 28.

- **Where:** `src/analysis/blend.rs`

### Category Strategy (`category_strategy`) [b9]

General deterministic classification of a scoring category as push, protect, or abandon. b9 currently shows category totals and W/T/L outcomes but does not emit this higher-level strategy classification.

### Closer (`closer`) [b9]

The designated closer for each MLB team. b9 resolves FanGraphs RosterResource tags with an SV-leader fallback, displays `RP1`, and applies the settled PQS save-category multiplier.

- **Where:** `src/providers/fangraphs_closer_chart.rs`; `src/store/projections.rs`

### Empirical-Bayes Blending (`empirical_bayes`) [b9]

Stabilization method for Statcast metrics: `blended = w * current + (1 - w) * prior`, where `w = sample / (sample + k)`. Each metric has its own k-value. Higher k = more regression to prior. Applied in `BlendStatcast`.

- **Where:** `src/analysis/statcast_blend.rs`

### Opportunity Damping (`opportunity_damping`) [b9]

Current-season weight in PQS blend is scaled by `min(PA/150, 1)` for hitters and `min(IP/40, 1)` for pitchers. Prevents small-sample current stats from dominating the blend early in the season.

- **Where:** `src/analysis/blend.rs`

### Pitcher Day State (`pitcher_day_state`) [b9]

General classification of a pitcher's same-day availability: `confirmed_sp` (RotoWire confirmed), `probable_sp` (MLB probable), `rp_available` (reliever whose team plays), `not_scheduled` (starter-eligible but not scheduled), or `no_game` (team off). b9 renders the underlying status facts without exposing a separate pitcher-day-state field.

### Pool (`pool`) [b9]

The selected player collection used as the normalization baseline for PQS z-scores. `pool_scores` extracts each available signal across that collection and excludes insufficient samples signal by signal.

- **Where:** `src/analysis/pqs.rs` — `pool_scores`


### POS Column (`pos`) [b9]

Width-five rendering of Yahoo position eligibility. b9 omits generic `Util` and `P` when specific positions exist, preserves a literal list when it fits, and otherwise compresses positions in defensive order (`C`, `1`, `2`, `3`, `S`, `O`, `P`, `R`). Six or more specific positions render as `All`. A closer receives a trailing `1`, such as `RP1`.

- **Aliases:** position, positions, POS
- **Where:** `src/player_display.rs` — `display_positions`

### Projected Production (`pp`) [b9]

Reference score for projected fantasy-category production, distinct from observed player quality. b9 stores and blends rest-of-season projections for detail-card windows but does not currently expose a standalone 0-100 PP score.

- **Aliases:** PP

### Stabilization Ramp (`stabilization_ramp`) [b9]

Signal weight scaled by `min(1.0, sample / threshold)` in PQS computation. Signals with insufficient sample size contribute less to the score. EB-blended signals use threshold=1 (already stabilized); raw signals use real thresholds (e.g., 50 PA for K%).

- **Where:** `src/analysis/pqs.rs`

### Steamer (`steamer`) [b9]

FanGraphs rest-of-season projection system providing projected counting and rate statistics. b9 retains the source projection and a provider-supplied or synchronized blend for detail-card windows.


### Player Quality Score (`pqs`) [b9]

On-demand quality model using stabilized skill signals. Hitter signals: xwOBA (0.30), K% (0.15), BB% (0.10), Sprint Speed (0.20), FB% (0.10), HR/FB (0.15). Pitcher signals: Whiff% (0.30), Chase% (0.20), GB% (0.15), Fastball Velo (0.15), K-BB% (0.20). Each signal is z-scored against the current player pool, clamped to ±2.0, weighted, and adjusted for opportunity, positional scarcity, and closer role. The score is computed at display time and feeds waiver ranking.

- **Aliases:** PQS, TS, TalentScore (legacy)
- **Where:** `src/analysis/pqs.rs` — `pool_scores`

### Z-Score (`z_score`) [b9]

`(value - pool_mean) / pool_stddev`. Per-signal z-score clamped to ±2.0 in PQS computation.


### ZiPS (`zips`) [b9]

Dan Szymborski's rest-of-season projection system, published by FanGraphs. b9 can retain ZiPS rows as projection inputs.


### ATC (`atc`) [b9]

Ariel Cohen's projection system that aggregates multiple forecasting models. b9 can retain ATC rows as projection inputs.

### Yahoo Players (`yp`) [b9]

Per-MLB-team count of players currently rostered by a fantasy team in the selected Yahoo league. Active, bench, injured, and not-active slots count; two-way players count once per MLB team. Displayed as `YP` in `b9 tt`.

- **Aliases:** YP
- **Where:** `src/store/mlb.rs`; `src/mlb_display.rs`

### Age (`age`) [b9]

Player age in whole years, derived at render time from the MLBAM `birthDate` stored in `players.birth_date`. Reduced by one if the birthday has not yet occurred in the current calendar year. Renders `-` when `birth_date` is NULL (no MLBAM identity yet, or never fetched). Used in the `b9 h <name>` and `b9 p <name>` detail card identity headers.

- **Where:** `src/player_display.rs` — `player_age`

### AVG162G (`avg162g`) [b9]

The Baseball-Reference-style 162-game pace row at the top of a player detail card's SPLIT table. It aggregates completed seasons, excludes the current season, and scales counting statistics by `162 / sum_games`. Rate statistics are recomputed from cumulative counts and are not scaled. With no completed-season games, cells render as unavailable.

- **Where:** `src/player_display.rs`

### GAME LOG (`game-log`) [b9]

The recent-results section below the SPLIT table on a player detail card. Hitter rows walk the last ten calendar days and use schedule plus boxscore data to distinguish starts, non-appearances, and off days. Pitcher rows show the last ten appearances with positive innings pitched.

- **Where:** `src/player_commands.rs`; `src/player_display.rs`; `src/providers/mlb.rs`

### Savant (`savant`) [b9]

The literal source label on hitter and pitcher detail-card Statcast rows. Some adjacent advanced metrics may be FanGraphs-derived, so treat the label as a display convention rather than strict cell-by-cell provenance.

- **Where:** `src/player_display.rs`
