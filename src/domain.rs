//! Fantasy-baseball domain records shared by b9 application layers.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// One current MLB club used by command selection and league grouping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MlbTeam {
    pub id: i64,
    pub name: String,
    pub location: String,
    pub club_name: String,
    pub abbreviation: String,
    pub league_id: i64,
}

/// One normalized member of an MLB 40-man roster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MlbRosterPlayer {
    pub team_abbreviation: String,
    pub mlbam_id: i64,
    pub name: String,
    pub position: String,
    pub primary_type: String,
    pub status: String,
    #[serde(default)]
    pub injury_status: String,
    #[serde(default)]
    pub game_status: String,
    #[serde(default)]
    pub is_closer: bool,
    pub jersey_number: String,
    #[serde(default)]
    pub eligible_positions: String,
    #[serde(default)]
    pub bat_side: String,
    #[serde(default)]
    pub pitch_hand: String,
    #[serde(default)]
    pub yahoo_rank: Option<i64>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub in_yahoo_pool: bool,
    #[serde(default)]
    pub plate_appearances: i64,
    #[serde(default)]
    pub on_base_percentage: f64,
    #[serde(default)]
    pub runs: i64,
    #[serde(default)]
    pub home_runs: i64,
    #[serde(default)]
    pub runs_batted_in: i64,
    #[serde(default)]
    pub stolen_bases: i64,
    #[serde(default)]
    pub batting_average: f64,
    #[serde(default)]
    pub innings_pitched: f64,
    #[serde(default)]
    pub quality_starts: i64,
    #[serde(default)]
    pub wins: i64,
    #[serde(default)]
    pub saves: i64,
    #[serde(default)]
    pub strikeouts: i64,
    #[serde(default)]
    pub earned_run_average: f64,
    #[serde(default)]
    pub whip: f64,
}

/// One MLB standings row with its resolved club identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MlbStanding {
    pub team: MlbTeam,
    pub wins: i64,
    pub losses: i64,
    pub games_back: String,
}

/// Aggregated season totals for one MLB club.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MlbTeamTotals {
    pub team: MlbTeam,
    pub batting: BattingStats,
    pub pitching: PitchingStats,
    pub yahoo_players: Option<i64>,
    pub players_available: Option<i64>,
}

/// One probable-pitcher side in a three-day MLB slate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MlbSlateRow {
    pub date: String,
    pub game_id: i64,
    pub game_time: String,
    pub away_team: String,
    pub home_team: String,
    pub away_pitcher: String,
    pub home_pitcher: String,
    pub win_probability: Option<f64>,
    #[serde(default)]
    pub away_free_agent: bool,
    #[serde(default)]
    pub home_free_agent: bool,
    #[serde(default)]
    pub away_mine: bool,
    #[serde(default)]
    pub home_mine: bool,
}

/// A fantasy-league scoring format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScoringType {
    Rotisserie,
    HeadToHead,
    Points,
    Other(String),
}

impl From<&str> for ScoringType {
    fn from(value: &str) -> Self {
        match value {
            "rotisserie" => Self::Rotisserie,
            "head-to-head" => Self::HeadToHead,
            "points" => Self::Points,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl From<String> for ScoringType {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl fmt::Display for ScoringType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Rotisserie => "rotisserie",
            Self::HeadToHead => "head-to-head",
            Self::Points => "points",
            Self::Other(value) => value,
        };
        formatter.write_str(value)
    }
}

/// A fantasy-roster position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    Catcher,
    FirstBase,
    SecondBase,
    ThirdBase,
    Shortstop,
    Outfield,
    StartingPitcher,
    ReliefPitcher,
    Utility,
    Bench,
    InjuredList,
    Other(String),
}

impl From<&str> for Position {
    fn from(value: &str) -> Self {
        match value {
            "C" => Self::Catcher,
            "1B" => Self::FirstBase,
            "2B" => Self::SecondBase,
            "3B" => Self::ThirdBase,
            "SS" => Self::Shortstop,
            "OF" => Self::Outfield,
            "SP" => Self::StartingPitcher,
            "RP" => Self::ReliefPitcher,
            "Util" => Self::Utility,
            "BN" => Self::Bench,
            "IL" => Self::InjuredList,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl From<String> for Position {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl fmt::Display for Position {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Catcher => "C",
            Self::FirstBase => "1B",
            Self::SecondBase => "2B",
            Self::ThirdBase => "3B",
            Self::Shortstop => "SS",
            Self::Outfield => "OF",
            Self::StartingPitcher => "SP",
            Self::ReliefPitcher => "RP",
            Self::Utility => "Util",
            Self::Bench => "BN",
            Self::InjuredList => "IL",
            Self::Other(value) => value,
        };
        formatter.write_str(value)
    }
}

/// Fantasy-league metadata and scoring configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct League {
    pub league_key: String,
    pub name: String,
    pub season: i32,
    pub num_teams: i32,
    pub scoring_type: ScoringType,
    pub roster_positions: Vec<Position>,
    pub batting_categories: Vec<String>,
    pub pitching_categories: Vec<String>,
}

/// A head-to-head matchup for one week.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Matchup {
    pub week: i32,
    pub week_start: String,
    pub week_end: String,
    pub status: String,
    pub teams: [MatchupTeam; 2],
}

/// One fantasy team's state within a weekly matchup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchupTeam {
    pub team_key: String,
    pub team_id: i64,
    pub name: String,
    pub is_current_login: bool,
    pub stats: HashMap<String, String>,
    pub wins: i32,
    pub losses: i32,
    pub ties: i32,
    pub completed_games: i32,
    pub live_games: i32,
    pub remaining_games: i32,
}

impl MatchupTeam {
    /// Returns category wins, the score displayed for the matchup.
    #[must_use]
    pub const fn score(&self) -> i32 {
        self.wins
    }

    /// Returns completed, live, and remaining games combined.
    #[must_use]
    pub const fn total_games(&self) -> i32 {
        self.completed_games + self.live_games + self.remaining_games
    }
}

/// One player's weekly statistics and roster state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerWeekStats {
    pub yahoo_player_id: i64,
    pub name: String,
    pub team: String,
    pub position_type: String,
    pub slot_position: Position,
    pub eligible_positions: Vec<Position>,
    pub injury_status: String,
    pub hab: String,
    pub runs: i32,
    pub home_runs: i32,
    pub runs_batted_in: i32,
    pub stolen_bases: i32,
    pub batting_average: String,
    pub innings_pitched: String,
    pub wins: i32,
    pub saves: i32,
    pub strikeouts: i32,
    pub earned_run_average: String,
    pub whip: String,
}

/// A fantasy team's roster and weekly player statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterWeekStats {
    pub team_key: String,
    pub team_name: String,
    pub week: i32,
    pub players: Vec<PlayerWeekStats>,
}

/// One Yahoo fantasy team normalized independently from persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FantasyTeam {
    pub team_key: String,
    pub league_key: String,
    pub team_id: i64,
    pub name: String,
    pub manager_name: String,
    pub is_owned_by_current_login: bool,
}

/// One Yahoo fantasy player normalized independently from persistence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FantasyPlayer {
    pub yahoo_player_id: i64,
    pub name: String,
    pub mlb_team: String,
    pub display_position: String,
    pub position_type: String,
    pub eligible_positions: Vec<Position>,
    pub injury_status: String,
    pub percent_owned: Option<f64>,
    pub yahoo_rank: Option<i64>,
}

/// One player assembled from durable fantasy ownership and MLB season state.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredFantasyPlayer {
    pub yahoo_player_id: Option<i64>,
    pub mlbam_id: Option<i64>,
    pub name: String,
    pub team: String,
    pub role: String,
    pub positions: String,
    pub status: String,
    pub rank: Option<i64>,
    pub percent_owned: Option<f64>,
    pub owner: Option<String>,
    pub slot: Option<String>,
    pub batting: [f64; 7],
    pub pitching: [f64; 7],
}

/// One rendered player-game-log line retained for an offline player card.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlayerGameLog {
    pub date: String,
    pub opponent: String,
    pub line: String,
}

/// One Yahoo roster ownership row using provider identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FantasyRosterSlot {
    pub team_key: String,
    pub yahoo_player_id: i64,
    pub slot_position: Position,
}

impl RosterWeekStats {
    /// Returns weekly batter records in roster order.
    #[must_use]
    pub fn batters(&self) -> Vec<&PlayerWeekStats> {
        self.players
            .iter()
            .filter(|player| player.position_type == "B")
            .collect()
    }

    /// Returns weekly pitcher records in roster order.
    #[must_use]
    pub fn pitchers(&self) -> Vec<&PlayerWeekStats> {
        self.players
            .iter()
            .filter(|player| player.position_type == "P")
            .collect()
    }
}

/// Standard season batting statistics.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BattingStats {
    pub plate_appearances: i32,
    pub batting_average: f64,
    pub on_base_percentage: f64,
    pub slugging_percentage: f64,
    pub on_base_plus_slugging: f64,
    pub home_runs: i32,
    pub runs_batted_in: i32,
    pub runs: i32,
    pub stolen_bases: i32,
    pub strikeouts: i32,
    pub walks: i32,
}

/// Standard season pitching statistics.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PitchingStats {
    pub games: i32,
    pub games_started: i32,
    pub innings_pitched: f64,
    pub earned_run_average: f64,
    pub whip: f64,
    pub strikeouts: i32,
    pub strikeouts_per_nine: f64,
    pub walks_per_nine: f64,
    pub fielding_independent_pitching: f64,
    pub expected_fielding_independent_pitching: f64,
    pub wins: i32,
    pub saves: i32,
    pub holds: i32,
    pub quality_starts: i32,
    pub rate_strikeouts: i32,
    pub walks: i32,
    pub batters_faced: i32,
}

/// Raw or blended Statcast metrics and their sample sizes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatcastData {
    pub average_exit_velocity: f64,
    pub barrel_percentage: f64,
    pub hard_hit_percentage: f64,
    pub expected_batting_average: f64,
    pub expected_slugging_percentage: f64,
    pub expected_weighted_on_base_average: f64,
    pub average_launch_angle: f64,
    pub sweet_spot_percentage: f64,
    pub sprint_speed: f64,
    pub fly_ball_percentage: f64,
    pub home_run_to_fly_ball_percentage: f64,
    pub fastball_velocity: f64,
    pub spin_rate: f64,
    pub whiff_percentage: f64,
    pub chase_percentage: f64,
    pub pitching_hard_hit_percentage: f64,
    pub ground_ball_percentage: f64,
    pub pitching_fly_ball_percentage: f64,
    pub expected_earned_run_average: f64,
    pub expected_fielding_independent_pitching: f64,
    pub plate_appearances: i32,
    pub batted_ball_events: i32,
}

/// One row in a player-card game log.
#[derive(Debug, Clone, PartialEq)]
pub struct GameLogRow {
    pub date: String,
    pub opponent_abbreviation: String,
    pub is_home: bool,
    pub team_result: String,
    pub batting_order: i32,
    pub hab: String,
    pub runs: i32,
    pub home_runs: i32,
    pub runs_batted_in: i32,
    pub stolen_bases: i32,
    pub batting_average: f64,
    pub innings_pitched_decimal: f64,
    pub wins: i32,
    pub saves: i32,
    pub strikeouts: i32,
    pub earned_run_average: f64,
    pub whip: f64,
}

/// A player assembled from fantasy, MLB, and Statcast sources.
#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub id: i64,
    pub yahoo_player_key: String,
    pub mlb_player_id: i64,
    pub name: String,
    pub team: String,
    pub positions: Vec<Position>,
    pub bat_side: String,
    pub pitch_hand: String,
    pub birth_date: String,
    pub jersey_number: String,
    pub roster_position: Position,
    pub injury_status: String,
    pub injury_note: String,
    pub mlbam_injury_note: String,
    pub ownership_percentage: f64,
    pub ownership_delta: f64,
    pub percentage_started: f64,
    pub yahoo_rank: i32,
    pub batting: Option<BattingStats>,
    pub pitching: Option<PitchingStats>,
    pub statcast_raw: Option<StatcastData>,
    pub statcast_blended: Option<StatcastData>,
    pub primary_type: String,
    pub player_quality_score: f64,
    pub is_closer: bool,
    pub spring_only: bool,
    pub projected_production: i32,
    pub is_recent_callup: bool,
    pub expert_consensus_rank: i32,
    pub fangraphs_war: f64,
    pub weighted_runs_created_plus: i32,
    pub owner: String,
}

impl Player {
    /// Returns whether batting statistics are present.
    #[must_use]
    pub const fn is_batter(&self) -> bool {
        self.batting.is_some()
    }

    /// Returns whether pitching statistics are present.
    #[must_use]
    pub const fn is_pitcher(&self) -> bool {
        self.pitching.is_some()
    }

    /// Returns whether the player is eligible at the exact position.
    #[must_use]
    pub fn eligible_at(&self, position: &Position) -> bool {
        self.positions.contains(position)
    }

    /// Returns age on a supplied Gregorian calendar date.
    #[must_use]
    pub fn age_on(&self, year: i32, month: u32, day: u32) -> Option<i32> {
        let birth = parse_iso_date(&self.birth_date)?;
        let as_of = valid_date(year, month, day)?;
        if birth > as_of {
            return None;
        }
        let before_birthday = (as_of.1, as_of.2) < (birth.1, birth.2);
        Some(as_of.0 - birth.0 - i32::from(before_birthday))
    }
}

/// A fantasy team's current roster.
#[derive(Debug, Clone, PartialEq)]
pub struct Roster {
    pub league_key: String,
    pub season: String,
    pub team_key: String,
    pub team_name: String,
    pub players: Vec<Player>,
}

impl Roster {
    /// Returns non-bench, non-injured-list players in roster order.
    #[must_use]
    pub fn active_players(&self) -> Vec<&Player> {
        self.players
            .iter()
            .filter(|player| {
                player.roster_position != Position::Bench
                    && player.roster_position != Position::InjuredList
            })
            .collect()
    }

    /// Returns batters in roster order.
    #[must_use]
    pub fn batters(&self) -> Vec<&Player> {
        self.players
            .iter()
            .filter(|player| player.is_batter())
            .collect()
    }

    /// Returns pitchers in roster order.
    #[must_use]
    pub fn pitchers(&self) -> Vec<&Player> {
        self.players
            .iter()
            .filter(|player| player.is_pitcher())
            .collect()
    }

    /// Returns the player whose Unicode-lowercased name exactly matches the query.
    #[must_use]
    pub fn player_by_name(&self, name: &str) -> Option<&Player> {
        let normalized = name.to_lowercase();
        self.players
            .iter()
            .find(|player| player.name.to_lowercase() == normalized)
    }
}

fn parse_iso_date(value: &str) -> Option<(i32, u32, u32)> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return None;
    }
    let year = value[0..4].parse().ok()?;
    let month = value[5..7].parse().ok()?;
    let day = value[8..10].parse().ok()?;
    valid_date(year, month, day)
}

fn valid_date(year: i32, month: u32, day: u32) -> Option<(i32, u32, u32)> {
    if year < 1 || !(1..=12).contains(&month) {
        return None;
    }
    let days = match month {
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 31,
    };
    (1..=days).contains(&day).then_some((year, month, day))
}

const fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
