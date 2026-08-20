# b9

Read-only decision-support CLI for Yahoo Fantasy Baseball.

## Why

Fantasy baseball managers make dozens of small decisions every day: who to start, who to bench, which categories to chase, and who to pick up. The useful data is spread across Yahoo, MLB, Baseball Savant, FanGraphs, FantasyPros, and game and odds providers. b9 assembles that context into compact terminal views so you can understand the matchup, act on it, and move on.

## Overview

b9 reads a configured public Yahoo league, enriches its players with MLB and analytical data, and presents matchup-aware roster, player-pool, standings, and probable-pitcher views. The primary workflow centers on `b9 m`, which combines the current head-to-head matchup with today's player statistics and game state. `b9 m -W` shows the running weekly player totals instead.

b9 never changes a Yahoo roster. Yahoo access is unauthenticated and public-only: no developer application, OAuth token, browser login, cookie, or Keychain entry is required. Data is synchronized in the foreground and stored locally in complete snapshots so the last usable state remains available when a provider refresh fails.

For architecture and design details, see [arch.md](arch.md).

## Setup

Select your Yahoo league and fantasy team with one foreground sync:

```bash
b9 sync -l 170874 -T Toros
```

The league may be a numeric ID or full Yahoo league key. The team may be a team key or name. In an interactive terminal, `b9 sync` prompts when either selection is missing and saves the result for later commands.

```bash
b9 st       # show local provider and snapshot status
b9 sync     # refresh the saved league and team
```

Yahoo's public fantasy endpoints are unofficial and may deny access or change without notice. b9 does not attempt to bypass those restrictions; it retains the last complete snapshot and reports recovery guidance.

## Usage

```bash
# Matchup — the primary command
b9 m                     # today's player stats and current matchup totals
b9 m -W                  # weekly running player totals
b9 m -D jul-01           # one day from the active season
b9 m -w 3                # a specific matchup week (weekly view)

# Roster inspection
b9 r                     # your fantasy roster
b9 r "team name"         # another fantasy roster
b9 rt                    # roster category totals
b9 rt -w                 # current weekly totals

# Player pools and detail cards
b9 h                     # browse hitters
b9 p                     # browse pitchers
b9 h 50                  # show 50 hitters
b9 h "player name"       # hitter detail card
b9 p "player name"       # pitcher detail card
b9 h -w                  # hitter waiver candidates
b9 p -w                  # pitcher waiver candidates
b9 h -s ops              # sort by a displayed field
b9 h -p OF               # filter by eligible position

# MLB-wide views
b9 t                     # every MLB 40-man roster
b9 t pirates             # select by abbreviation, city, or nickname
b9 tt                    # MLB standings and team season totals
b9 sp                    # three-day probable-pitcher slate
b9 sp -f                 # bypass the slate freshness gate

# Reference and diagnostics
b9 i                     # browse the embedded glossary
b9 i xwoba               # look up one term
b9 fetch <host> <path>   # inspect an allowlisted provider response

# Local state
b9 reset                 # explicitly delete the local b9 database
```

Use `-l <league-key>` on fantasy commands to override the saved league for one run. Use `-d` or `--debug` to print operation diagnostics. Run `b9 --help` for the complete command surface.

Player-pool views incorporate PQS analysis, FanGraphs projections and closer roles, FantasyPros ECR, and locally synchronized Yahoo rank and ownership when available. Fixed-width tables use semantic color in supported 256-color terminals and equivalent plain text when redirected, when `NO_COLOR` is set, or when `TERM=dumb`.

## Example Use Case

It is Thursday morning and your head-to-head categories are close. Start with the daily matchup:

```bash
b9 m
```

The side-by-side view shows today's player results and game state while the totals and W/T/L summary retain the whole matchup week's score. If you need to inspect every player's contribution for the week, switch views:

```bash
b9 m -W
```

If stolen bases or home runs are within reach, browse the waiver pool and inspect a candidate:

```bash
b9 h -w
b9 h "player name"
```

The detail card combines identity, ownership, season performance, projections, Statcast context, and recent game history. b9 remains advisory; make any roster move directly in Yahoo.

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
./build.sh                                      # format, lint, test, build, and install
./build.sh prep v1.2.3 "release message"       # prepare release metadata
./build.sh v1.2.3 "release message"            # tagged release workflow
```

`./build.sh` is the canonical repository validation and release command. A successful normal build installs the `b9` binary under the active Cargo home.

## Governance

This repository is governed by an explicit session-entry contract for AI coding agents. See [govna/operator-contract-rationale.md](govna/operator-contract-rationale.md) for the design reasoning and [AGENTS.md](AGENTS.md) for the operational rules.
