CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS sync_log (
    table_name  TEXT    PRIMARY KEY,
    synced_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_item_state (
    source              TEXT NOT NULL,
    item                TEXT NOT NULL,
    scope               TEXT NOT NULL DEFAULT '',
    last_attempted_at   INTEGER NOT NULL DEFAULT 0,
    last_successful_at  INTEGER NOT NULL DEFAULT 0,
    status              TEXT NOT NULL DEFAULT 'unknown',
    error_message       TEXT NOT NULL DEFAULT '',
    pipeline_version    TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (source, item, scope)
);

CREATE TABLE IF NOT EXISTS sync_row_state (
    source              TEXT NOT NULL,
    item                TEXT NOT NULL,
    scope               TEXT NOT NULL DEFAULT '',
    entity_kind         TEXT NOT NULL,
    entity_key          TEXT NOT NULL,
    local_id            INTEGER,
    last_attempted_at   INTEGER NOT NULL DEFAULT 0,
    last_successful_at  INTEGER NOT NULL DEFAULT 0,
    status              TEXT NOT NULL DEFAULT 'unknown',
    error_message       TEXT NOT NULL DEFAULT '',
    pipeline_version    TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (source, item, scope, entity_kind, entity_key)
);

CREATE TABLE IF NOT EXISTS command_snapshots (
    dataset             TEXT NOT NULL,
    source              TEXT NOT NULL,
    scope               TEXT NOT NULL DEFAULT '',
    snapshot_version    TEXT NOT NULL DEFAULT '',
    payload             TEXT NOT NULL,
    last_successful_at  INTEGER NOT NULL,
    stale               INTEGER NOT NULL DEFAULT 0,
    error_message       TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (dataset, source, scope)
);

CREATE TABLE IF NOT EXISTS yahoo_leagues (
    league_key      TEXT    PRIMARY KEY,
    name            TEXT    NOT NULL,
    season          INTEGER NOT NULL,
    num_teams       INTEGER NOT NULL,
    scoring_type    TEXT    NOT NULL,
    current_week    INTEGER,
    faab_budget     INTEGER,
    max_weekly_adds INTEGER,
    trade_deadline  TEXT,
    min_ip          INTEGER,
    waiver_type     TEXT,
    synced_at       INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS yahoo_stat_categories (
    league_key   TEXT    NOT NULL,
    stat_id      INTEGER NOT NULL,
    abbr         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    sort_order   INTEGER NOT NULL DEFAULT 1,
    display_only INTEGER NOT NULL DEFAULT 0,
    seq          INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (league_key, stat_id)
);

CREATE TABLE IF NOT EXISTS yahoo_roster_positions (
    league_key TEXT    NOT NULL,
    position   TEXT    NOT NULL,
    count      INTEGER NOT NULL,
    PRIMARY KEY (league_key, position)
);

CREATE TABLE IF NOT EXISTS yahoo_teams (
    team_key          TEXT    PRIMARY KEY,
    league_key        TEXT    NOT NULL,
    team_id           INTEGER NOT NULL,
    name              TEXT    NOT NULL,
    manager_nickname  TEXT,
    manager_guid      TEXT,
    waiver_priority   INTEGER,
    faab_balance      INTEGER,
    wins              INTEGER,
    losses            INTEGER,
    ties              INTEGER,
    moves             INTEGER,
    draft_position    INTEGER,
    rank              INTEGER,
    synced_at         INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS players (
    id                  INTEGER PRIMARY KEY,   -- local surrogate key (auto-increment)
    mlbam_id            INTEGER,               -- MLBAM external ID; NULL if unmatched (not UNIQUE: two-way players share one MLBAM ID)
    yahoo_player_id     INTEGER UNIQUE,        -- Yahoo external ID; NULL if MLBAM-only
    name                TEXT    NOT NULL,
    mlb_team            TEXT,
    display_position    TEXT,
    position_type       TEXT,
    eligible_positions  TEXT,
    status              TEXT,
    percent_owned       REAL,
    ownership_delta     REAL,
    is_undroppable      INTEGER NOT NULL DEFAULT 0,
    is_closer           INTEGER NOT NULL DEFAULT 0,
    yahoo_rank          INTEGER,
    bat_side            TEXT,
    pitch_hand          TEXT,
    pct_started         REAL,
    injury_note         TEXT,
    injury_note_ts      INTEGER,
    mlbam_injury_note   TEXT,
    pqs                 REAL,                  -- Player Quality Score (internal only — never displayed)
    fangraphs_war       REAL,                  -- FanGraphs fWAR for current season; NULL if not found
    wrc_plus            INTEGER,               -- FanGraphs wRC+; NULL if not found (pitchers)
    ecr                 INTEGER,               -- FantasyPros ECR; NULL if not found
    mlbam_match_source  TEXT,                  -- how mlbam_id was established: 'seed' (MLBAM bulk stats), '40man' (/people resolve), 'name' / 'name+team' / 'name+pos' / 'name+pos+team' (reconciler name tiers), 'name+twoway' (two-way player branch), 'injury_tx' (MLBAM transactions/injured), 'jersey+team' (jersey+team reconciler tier), 'manual' (one-shot SQL fix), NULL
    mlbam_matched_at    INTEGER,               -- unix timestamp of when mlbam_id was matched; NULL if unmatched
    birth_date          TEXT,                  -- ISO YYYY-MM-DD; populated from MLBAM /people; NULL when mlbam_id is NULL or not yet fetched
    birth_date_fetched_at INTEGER,             -- unix timestamp of last birth_date fetch; gates 30-day backstop refresh
    jersey_number       TEXT,                  -- Yahoo uniform_number / MLBAM jerseyNumber; nullable; used by reconciler's jersey+team tier
    synced_at           INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS statcast_seasons (
    player_id   INTEGER NOT NULL,              -- references players.id
    season      INTEGER NOT NULL,
    stat_group  TEXT    NOT NULL,              -- 'batting' or 'pitching'
    -- Denominators
    pa          INTEGER,                       -- PA (batting) or BF (pitching)
    bbe         INTEGER,                       -- batted ball events (batting) or BBE-against (pitching)
    -- Hitting metrics (Statcast)
    exit_velo_avg       REAL,
    barrel_pct          REAL,
    hard_hit_pct        REAL,
    xba                 REAL,
    xslg                REAL,
    launch_angle_avg    REAL,
    sweet_spot_pct      REAL,
    xwoba               REAL,
    xobp                REAL,
    sprint_speed        REAL,
    strikeout_pct       REAL,                  -- Savant-computed K% for batting or pitching
    walk_pct            REAL,                  -- Savant-computed BB% for batting or pitching
    ops                 REAL,                  -- Savant-computed hitter OPS
    -- Additional batting metrics
    fb_pct              REAL,                  -- fly ball rate (batters)
    hr_fb_pct           REAL,                  -- HR/FB ratio (batters, FanGraphs)
    -- Pitching metrics (Statcast)
    fastball_velo       REAL,
    spin_rate           REAL,
    hard_hit_pct_pit    REAL,
    xera                REAL,
    -- Additional pitching metrics
    whiff_pct           REAL,
    chase_pct           REAL,
    gb_pct              REAL,
    fb_pct_pit          REAL,
    xfip                REAL,
    -- Metadata
    fetched_at          INTEGER,               -- unix timestamp of last write
    PRIMARY KEY (player_id, season, stat_group)
);

CREATE TABLE IF NOT EXISTS yahoo_roster_slots (
    team_key      TEXT    NOT NULL,
    player_id     INTEGER NOT NULL,             -- references players.id
    slot_position TEXT    NOT NULL,
    synced_at     INTEGER NOT NULL,
    PRIMARY KEY (team_key, player_id)
);

CREATE TABLE IF NOT EXISTS yahoo_free_agents (
    league_key  TEXT    NOT NULL,
    player_id   INTEGER NOT NULL,
    synced_at   INTEGER NOT NULL,
    PRIMARY KEY (league_key, player_id)
);

CREATE TABLE IF NOT EXISTS mlbam_season_stats (
    player_id   INTEGER NOT NULL,              -- references players.id
    season      INTEGER NOT NULL,
    stat_group  TEXT    NOT NULL,   -- 'hitting' or 'pitching'
    -- hitting
    g           INTEGER,
    pa          INTEGER,
    ab          INTEGER,
    h           INTEGER,
    hr          INTEGER,
    rbi         INTEGER,
    r           INTEGER,
    sb          INTEGER,
    avg         REAL,
    obp         REAL,
    so_bat      INTEGER,            -- batter strikeouts
    doubles     INTEGER,
    triples     INTEGER,
    cs          INTEGER,            -- caught stealing
    bb          INTEGER,            -- batter walks (baseOnBalls)
    hbp         INTEGER,            -- hit by pitch
    tb          INTEGER,            -- total bases
    slg         REAL,
    ops         REAL,
    sf          INTEGER,            -- sacrifice flies
    sh          INTEGER,            -- sacrifice bunts (sac hits)
    gidp        INTEGER,            -- grounded into double play
    ibb         INTEGER,            -- intentional walks (batter)
    babip       REAL,               -- BABIP
    -- pitching
    w           INTEGER,
    l           INTEGER,
    sv          INTEGER,
    hld         INTEGER,
    k           INTEGER,            -- pitcher strikeouts
    era         REAL,
    whip        REAL,
    ip          REAL,
    qs          INTEGER,
    gs          INTEGER,            -- games started
    h_pit       INTEGER,            -- hits allowed
    r_pit       INTEGER,            -- runs allowed
    er          INTEGER,            -- earned runs
    hr_pit      INTEGER,            -- HR allowed
    bb_pit      INTEGER,            -- pitcher walks (baseOnBalls)
    hbp_pit     INTEGER,            -- hit batsmen
    bk          INTEGER,            -- balks
    wp          INTEGER,            -- wild pitches
    bf          INTEGER,            -- batters faced
    gf          INTEGER,            -- games finished
    svo         INTEGER,            -- save opportunities
    bs          INTEGER,            -- blown saves
    cg          INTEGER,            -- complete games
    sho         INTEGER,            -- shutouts
    ibb_pit     INTEGER,            -- intentional walks issued
    k9          REAL,               -- K/9 (MLBAM pre-computed)
    bb9         REAL,               -- BB/9
    h9          REAL,               -- H/9
    hr9         REAL,               -- HR/9
    kbb         REAL,               -- K/BB ratio
    inherited_runners        INTEGER, -- runners on base when P enters
    inherited_runners_scored INTEGER, -- of those, how many scored
    pickoffs    INTEGER,            -- pickoffs
    sb_allowed  INTEGER,            -- stolen bases allowed
    cs_allowed  INTEGER,            -- caught stealing against
    pitches     INTEGER,            -- total pitches thrown
    pitches_per_inn REAL,           -- pitches per inning (MLBAM pre-computed)
    fip         REAL,               -- Fielding Independent Pitching (computed from stored counts)
    -- FanGraphs (enriched by syncFanGraphs)
    fangraphs_war REAL,             -- FanGraphs fWAR for this season; NULL if not enriched
    wrc_plus    INTEGER,            -- FanGraphs wRC+ (batters only); NULL if not enriched
    synced_at   INTEGER NOT NULL,
    PRIMARY KEY (player_id, season, stat_group)
);

CREATE TABLE IF NOT EXISTS mlb_game_schedule (
    game_date  TEXT    NOT NULL,   -- "2026-03-25"
    team_abbr  TEXT    NOT NULL,   -- "NYY", "BOS", etc.
    status     TEXT    NOT NULL,   -- "7:05p", "Live", "Final"
    synced_at  INTEGER NOT NULL,
    PRIMARY KEY (game_date, team_abbr)
);

CREATE TABLE IF NOT EXISTS season_sync_status (
    source           TEXT    NOT NULL,   -- "mlbam_hitting", "mlbam_pitching", "fangraphs", etc.
    season           INTEGER NOT NULL,
    status           TEXT    NOT NULL,   -- "complete", "partial", "failed"
    fetched_at       INTEGER NOT NULL,
    record_count     INTEGER NOT NULL DEFAULT 0,
    pipeline_version INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (source, season)
);

CREATE TABLE IF NOT EXISTS sync_runs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    mode       TEXT    NOT NULL,   -- "live", "events", "history"
    started_at INTEGER NOT NULL,
    ended_at   INTEGER,
    status     TEXT    NOT NULL DEFAULT 'running', -- "running", "complete", "failed"
    origin     TEXT    NOT NULL DEFAULT 'automatic', -- "manual", "automatic", "startup"
    counts     TEXT                                -- JSON: {"mlbam_hitting": 450, ...}
);

CREATE TABLE IF NOT EXISTS dashboard_status (
    id                         INTEGER PRIMARY KEY CHECK (id = 1),
    last_run_at                INTEGER,
    last_run_status            TEXT,
    next_run_at                INTEGER,
    provider_last_success_at   INTEGER,
    provider_last_failure_at   INTEGER,
    provider_failure_count     INTEGER NOT NULL DEFAULT 0,
    circuit_open               INTEGER NOT NULL DEFAULT 0,
    last_error                 TEXT NOT NULL DEFAULT '',
    provider_freshness_at      INTEGER
);

CREATE TABLE IF NOT EXISTS player_projections (
    player_id   INTEGER NOT NULL,
    season      INTEGER NOT NULL,
    source      TEXT    NOT NULL,   -- 'steamer', 'zips', 'atc', 'blend'
    stat_group  TEXT    NOT NULL,   -- 'batting', 'pitching'
    pa          REAL,               -- projected plate appearances (hitters)
    ip          REAL,               -- projected innings pitched (pitchers)
    hr          REAL,
    r           REAL,
    rbi         REAL,
    sb          REAL,
    avg         REAL,
    obp         REAL,
    slg         REAL,
    era         REAL,
    whip        REAL,
    k           REAL,
    w           REAL,
    sv          REAL,
    bb          REAL,
    fetched_at  INTEGER NOT NULL,
    PRIMARY KEY (player_id, season, source, stat_group)
);

CREATE TABLE IF NOT EXISTS fangraphs_batted_ball (
    player_id   INTEGER NOT NULL,
    season      INTEGER NOT NULL,
    fb_pct      REAL,
    hr_fb_pct   REAL,
    fetched_at  INTEGER NOT NULL,
    PRIMARY KEY (player_id, season)
);

CREATE TABLE IF NOT EXISTS yahoo_transactions (
    transaction_key TEXT    NOT NULL,
    league_key      TEXT    NOT NULL,
    type            TEXT    NOT NULL,
    status          TEXT    NOT NULL,
    ts              INTEGER NOT NULL,
    player_key      TEXT    NOT NULL,
    player_name     TEXT,
    source_type     TEXT,
    dest_type       TEXT,
    source_team_key TEXT,
    dest_team_key   TEXT,
    PRIMARY KEY (transaction_key, player_key)
);

CREATE TABLE IF NOT EXISTS mlb_team_active_rosters (
    team_abbr     TEXT    NOT NULL,
    mlbam_id      INTEGER NOT NULL,
    primary_type  TEXT    NOT NULL CHECK (primary_type IN ('H','P')),
    status        TEXT    NOT NULL DEFAULT 'A',
    jersey_number TEXT,
    fetched_at    INTEGER NOT NULL,
    PRIMARY KEY (team_abbr, mlbam_id, primary_type)
);

CREATE TABLE IF NOT EXISTS mlb_odds (
    game_pk         INTEGER NOT NULL,
    market          TEXT    NOT NULL CHECK (market IN ('moneyline','total','pitcher_strikeouts')),
    side            TEXT    NOT NULL CHECK (side IN ('home','away','over','under')),
    line            REAL,                                  -- O/U value; NULL for moneyline
    price           INTEGER NOT NULL,                      -- American odds (e.g. -110, +145)
    player_mlbam_id INTEGER NOT NULL DEFAULT 0,            -- 0 sentinel for game-level markets; MLBAM ID for pitcher_strikeouts (SQLite forbids expressions in PK; using NOT NULL+sentinel)
    sportsbook      TEXT    NOT NULL,
    fetched_at      INTEGER NOT NULL,
    PRIMARY KEY (game_pk, market, side, player_mlbam_id, sportsbook)
);

-- fantasy_players (src/store/fantasy.rs) filters and joins on these columns
-- via correlated subqueries; none of the three tables' primary keys lead
-- with the column this query filters by, so each needed its own index.
-- Kept identical to store.rs's FANTASY_PLAYERS_INDEXES so a fresh database
-- and one migrated from an earlier version end up the same.
CREATE INDEX IF NOT EXISTS idx_mlb_team_active_rosters_mlbam_id ON mlb_team_active_rosters(mlbam_id);
CREATE INDEX IF NOT EXISTS idx_players_mlbam_id ON players(mlbam_id);
CREATE INDEX IF NOT EXISTS idx_yahoo_roster_slots_player_id ON yahoo_roster_slots(player_id);
