# skout

Read-only decision-support CLI for Yahoo Fantasy Baseball.

## Why

Fantasy baseball managers make dozens of small decisions every day: who to start, who to bench, which categories to chase, and who to pick up. The useful data is spread across Yahoo, MLB, Baseball Savant, FanGraphs, FantasyPros, and game and odds providers. skout assembles that context into compact terminal views so you can understand the matchup, act on it, and move on.

## Overview

skout reads a configured public Yahoo league, enriches its players with MLB and analytical data, and presents matchup-aware roster, player-pool, standings, and probable-pitcher views. The primary workflow centers on `skout m`, which combines the current head-to-head matchup with today's player statistics and game state. `skout m -W` shows the running weekly player totals instead.

skout never changes a Yahoo roster. Yahoo access is unauthenticated and public-only: no developer application, OAuth token, browser login, cookie, or Keychain entry is required. Data is synchronized in the foreground and stored locally in complete snapshots so the last usable state remains available when a provider refresh fails.

For architecture and design details, see [arch.md](arch.md).

## Setup

Select your Yahoo league and fantasy team with one foreground sync:

```bash
skout sync -l 170874 -T Toros
```

The league may be a numeric ID or full Yahoo league key. The team may be a team key or name. In an interactive terminal, `skout sync` prompts when either selection is missing and saves the result for later commands.

```bash
skout st       # show local provider and snapshot status
skout sync     # refresh the saved league and team
```

Yahoo's public fantasy endpoints are unofficial and may deny access or change without notice. skout does not attempt to bypass those restrictions; it retains the last complete snapshot and reports recovery guidance.

## Usage

```bash
# Matchup — the primary command
skout m                     # today's player stats and current matchup totals
skout m -W                  # weekly running player totals
skout m -D jul-01           # one day from the active season
skout m -w 3                # a specific matchup week (weekly view)

# Roster inspection
skout r                     # your fantasy roster
skout r "team name"         # another fantasy roster
skout rt                    # roster category totals
skout rt -w                 # current weekly totals

# Player pools and detail cards
skout h                     # browse hitters
skout p                     # browse pitchers
skout h 50                  # show 50 hitters
skout h "player name"       # hitter detail card
skout p "player name"       # pitcher detail card
skout h -w                  # available Yahoo hitters to pick up
skout p -w                  # available Yahoo pitchers to pick up
skout h -s ops              # sort by a displayed field
skout h -p OF               # filter by eligible position

# MLB-wide views
skout t                     # every MLB 40-man roster
skout t pirates             # select by abbreviation, city, or nickname
skout tt                    # MLB standings and team season totals
skout sp                    # three-day probable-pitcher slate
skout sp -f                 # bypass the slate freshness gate

# Reference and diagnostics
skout i                     # browse the embedded glossary
skout i xwoba               # look up one term
skout fetch <host> <path>   # inspect an allowlisted provider response

# Local state
skout reset                 # explicitly delete the local skout database
```

Use `-l <league-key>` on fantasy commands to override the saved league for one run. Use `-d` or `--debug` to print operation diagnostics. Run `skout --help` for the complete command surface.

Player-pool views incorporate PQS analysis, FanGraphs projections and closer roles, FantasyPros ECR, and locally synchronized Yahoo rank and ownership when available. Waiver views draw from a complete, bounded Yahoo available-player snapshot and include IL, NA, and SUSP players. Players that pass the active-roster, identity, season-usage, and injury-status ranking gates appear first by PQS; remaining available players follow by Yahoo rank and name. Fixed-width tables use semantic color in supported 256-color terminals and equivalent plain text when redirected, when `NO_COLOR` is set, or when `TERM=dumb`.

## Example Use Case

It is Thursday morning and your head-to-head categories are close. Start with the daily matchup:

```bash
skout m
```

The side-by-side view shows today's player results and game state while the totals and W/T/L summary retain the whole matchup week's score. If you need to inspect every player's contribution for the week, switch views:

```bash
skout m -W
```

If stolen bases or home runs are within reach, browse the waiver pool and inspect a candidate:

```bash
skout h -w
skout h "player name"
```

The detail card combines identity, ownership, season performance, projections, Statcast context, and recent game history. skout remains advisory; make any roster move directly in Yahoo.

## Data Sources

| Source | Authentication | Used for |
|--------|----------------|----------|
| Yahoo Fantasy public endpoints | None | League settings, standings, rosters, free agents, ownership, ranks, and matchup statistics |
| [MLB StatsAPI](https://statsapi.mlb.com/api/v1) | None | Rosters, player identities, statistics, schedules, injuries, and game logs |
| [Baseball Savant](https://baseballsavant.mlb.com) | None | Statcast hitting and pitching metrics |
| [FanGraphs](https://www.fangraphs.com) | None | Projections, advanced statistics, and closer roles |
| [FantasyPros](https://www.fantasypros.com) | None | Expert Consensus Rankings |
| ESPN | None | Current-day game and odds context |
| OddsShark | None | Optional future-game odds |

ESPN and OddsShark are supplemental sources and do not own command success. OddsShark is unofficial and may degrade without failing the probable-pitcher slate.

## Building from Source

Requires a Rust toolchain with edition 2024 support.

```bash
./build.sh                                     # format, lint, test, build, and install
./build.sh prep v1.2.3 "release message"       # prepare release metadata
./build.sh v1.2.3 "release message"            # tagged release workflow
```

`./build.sh` is the canonical repository validation and release command. A successful normal build installs the `skout` binary under the active Cargo home.

## Governance

This repository is governed by an explicit session-entry contract for AI coding agents. See [govna/operator-contract-rationale.md](govna/operator-contract-rationale.md) for the design reasoning and [AGENTS.md](AGENTS.md) for the operational rules.
