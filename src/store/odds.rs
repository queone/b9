//! Typed persistence for provider moneyline rows.

use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use rusqlite::{OptionalExtension, params, params_from_iter};

use super::{Store, StoreError, required_time};

const OPERATION: &str = "replace moneyline odds";
const MAX_SPORTSBOOK_LEN: usize = 128;

/// One complete quoted moneyline for an MLB game.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoneylineQuote {
    pub game_pk: i64,
    pub home_price: i64,
    pub away_price: i64,
    pub sportsbook: String,
}

/// One stored complete moneyline and its fetch time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredMoneyline {
    pub game_pk: i64,
    pub home_price: i64,
    pub away_price: i64,
    pub sportsbook: String,
    pub fetched_at: SystemTime,
}

impl Store {
    /// Atomically replace moneylines for the supplied affected games.
    pub fn replace_moneylines(
        &mut self,
        affected_games: &[i64],
        quotes: &[MoneylineQuote],
    ) -> Result<(), StoreError> {
        let games = validate_replacement(affected_games, quotes)?;
        if games.is_empty() {
            return Ok(());
        }
        let (_, fetched_at) = self.captured_time(OPERATION)?;
        let path = self.path.clone();
        self.transaction(|transaction| {
            let placeholders = std::iter::repeat_n("?", games.len())
                .collect::<Vec<_>>()
                .join(",");
            let delete = format!(
                "DELETE FROM mlb_odds WHERE market = 'moneyline' AND game_pk IN ({placeholders})"
            );
            transaction
                .execute(&delete, params_from_iter(games.iter()))
                .map_err(|error| StoreError::operation("delete prior moneylines", &path, error))?;
            let mut statement = transaction
                .prepare(
                    "INSERT INTO mlb_odds
                     (game_pk, market, side, line, price, player_mlbam_id, sportsbook, fetched_at)
                     VALUES (?1, 'moneyline', ?2, NULL, ?3, 0, ?4, ?5)",
                )
                .map_err(|error| StoreError::operation("prepare moneyline insert", &path, error))?;
            for quote in quotes {
                for (side, price) in [("home", quote.home_price), ("away", quote.away_price)] {
                    statement
                        .execute(params![
                            quote.game_pk,
                            side,
                            price,
                            quote.sportsbook,
                            fetched_at
                        ])
                        .map_err(|error| StoreError::operation("insert moneyline", &path, error))?;
                }
            }
            Ok(())
        })
    }

    /// Read complete moneylines for the supplied games in game-key order.
    pub fn moneylines_for_games(
        &self,
        game_pks: &[i64],
    ) -> Result<BTreeMap<i64, StoredMoneyline>, StoreError> {
        let games = validate_games("read moneylines", game_pks)?;
        if games.is_empty() {
            return Ok(BTreeMap::new());
        }
        let placeholders = std::iter::repeat_n("?", games.len())
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "SELECT game_pk, market, side, line, price, player_mlbam_id, sportsbook, fetched_at
             FROM mlb_odds WHERE game_pk IN ({placeholders}) ORDER BY game_pk, side, sportsbook"
        );
        let mut statement = self
            .connection()
            .prepare(&query)
            .map_err(|error| StoreError::operation("prepare moneyline read", &self.path, error))?;
        let mut rows = statement
            .query(params_from_iter(games.iter()))
            .map_err(|error| StoreError::operation("query moneylines", &self.path, error))?;
        let mut partial: BTreeMap<i64, PartialLine> = BTreeMap::new();
        while let Some(row) = rows
            .next()
            .map_err(|error| StoreError::operation("iterate moneylines", &self.path, error))?
        {
            let game_pk: i64 = row
                .get(0)
                .map_err(|error| StoreError::operation("read moneyline game", &self.path, error))?;
            let market: String = row.get(1).map_err(|error| {
                StoreError::operation("read moneyline market", &self.path, error)
            })?;
            let side: String = row
                .get(2)
                .map_err(|error| StoreError::operation("read moneyline side", &self.path, error))?;
            let line: Option<f64> = row
                .get(3)
                .map_err(|error| StoreError::operation("read moneyline line", &self.path, error))?;
            let price: i64 = row.get(4).map_err(|error| {
                StoreError::operation("read moneyline price", &self.path, error)
            })?;
            let player_id: i64 = row.get(5).map_err(|error| {
                StoreError::operation("read moneyline player", &self.path, error)
            })?;
            let sportsbook: String = row.get(6).map_err(|error| {
                StoreError::operation("read moneyline sportsbook", &self.path, error)
            })?;
            let fetched_at: i64 = row
                .get(7)
                .map_err(|error| StoreError::operation("read moneyline time", &self.path, error))?;
            if market != "moneyline" {
                if matches!(market.as_str(), "total" | "pitcher_strikeouts") {
                    continue;
                }
                return Err(StoreError::invalid(
                    "read moneylines",
                    format!("game {game_pk} has invalid stored market {market}"),
                ));
            }
            validate_stored(game_pk, &market, &side, line, price, player_id, &sportsbook)?;
            let fetched_at = required_time("read moneylines", "fetched_at", fetched_at)?;
            let entry = partial.entry(game_pk).or_insert_with(|| PartialLine {
                sportsbook: sportsbook.clone(),
                fetched_at,
                home: None,
                away: None,
            });
            if entry.sportsbook != sportsbook || entry.fetched_at != fetched_at {
                return Err(StoreError::invalid(
                    "read moneylines",
                    format!("game {game_pk} has inconsistent moneyline rows"),
                ));
            }
            let target = if side == "home" {
                &mut entry.home
            } else {
                &mut entry.away
            };
            if target.replace(price).is_some() {
                return Err(StoreError::invalid(
                    "read moneylines",
                    format!("game {game_pk} has duplicate {side} moneylines"),
                ));
            }
        }
        partial
            .into_iter()
            .map(|(game_pk, line)| {
                let home_price = line.home.ok_or_else(|| {
                    StoreError::invalid(
                        "read moneylines",
                        format!("game {game_pk} has no home moneyline"),
                    )
                })?;
                let away_price = line.away.ok_or_else(|| {
                    StoreError::invalid(
                        "read moneylines",
                        format!("game {game_pk} has no away moneyline"),
                    )
                })?;
                Ok((
                    game_pk,
                    StoredMoneyline {
                        game_pk,
                        home_price,
                        away_price,
                        sportsbook: line.sportsbook,
                        fetched_at: line.fetched_at,
                    },
                ))
            })
            .collect()
    }

    /// Return the latest odds fetch time, if any odds row exists.
    pub fn latest_odds_fetch_time(&self) -> Result<Option<SystemTime>, StoreError> {
        let timestamp: Option<i64> = self
            .connection()
            .query_row("SELECT MAX(fetched_at) FROM mlb_odds", [], |row| row.get(0))
            .optional()
            .map_err(|error| StoreError::operation("read latest odds time", &self.path, error))?
            .flatten();
        timestamp
            .map(|value| required_time("read latest odds time", "fetched_at", value))
            .transpose()
    }
}

struct PartialLine {
    sportsbook: String,
    fetched_at: SystemTime,
    home: Option<i64>,
    away: Option<i64>,
}

fn validate_replacement(
    affected_games: &[i64],
    quotes: &[MoneylineQuote],
) -> Result<Vec<i64>, StoreError> {
    let games = validate_games(OPERATION, affected_games)?;
    let game_set: BTreeSet<_> = games.iter().copied().collect();
    let mut quoted = BTreeSet::new();
    for quote in quotes {
        if !game_set.contains(&quote.game_pk) {
            return Err(StoreError::invalid(
                OPERATION,
                format!("quote game {} is not in the affected set", quote.game_pk),
            ));
        }
        if !quoted.insert(quote.game_pk) {
            return Err(StoreError::invalid(
                OPERATION,
                format!("game {} has more than one quote", quote.game_pk),
            ));
        }
        if quote.home_price == 0 || quote.away_price == 0 {
            return Err(StoreError::invalid(
                OPERATION,
                format!("game {} moneyline prices must be nonzero", quote.game_pk),
            ));
        }
        validate_sportsbook(OPERATION, &quote.sportsbook)?;
    }
    Ok(games)
}

fn validate_games(operation: &'static str, values: &[i64]) -> Result<Vec<i64>, StoreError> {
    let mut games = BTreeSet::new();
    for value in values {
        if *value <= 0 {
            return Err(StoreError::invalid(
                operation,
                "game identifiers must be positive",
            ));
        }
        games.insert(*value);
    }
    Ok(games.into_iter().collect())
}

fn validate_sportsbook(operation: &'static str, value: &str) -> Result<(), StoreError> {
    if value.len() > MAX_SPORTSBOOK_LEN {
        return Err(StoreError::invalid(
            operation,
            format!("sportsbook exceeds {MAX_SPORTSBOOK_LEN} bytes"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(StoreError::invalid(
            operation,
            "sportsbook contains control characters",
        ));
    }
    Ok(())
}

fn validate_stored(
    game_pk: i64,
    market: &str,
    side: &str,
    line: Option<f64>,
    price: i64,
    player_id: i64,
    sportsbook: &str,
) -> Result<(), StoreError> {
    if game_pk <= 0 || market != "moneyline" || !matches!(side, "home" | "away") {
        return Err(StoreError::invalid(
            "read moneylines",
            "stored moneyline identity is invalid",
        ));
    }
    if line.is_some() || price == 0 || player_id != 0 {
        return Err(StoreError::invalid(
            "read moneylines",
            "stored moneyline shape is invalid",
        ));
    }
    validate_sportsbook("read moneylines", sportsbook)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, UNIX_EPOCH};

    use rusqlite::Connection;

    use super::*;
    use crate::store::{Clock, SCHEMA};

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> SystemTime {
            UNIX_EPOCH + Duration::from_secs(1_800_000_000)
        }
    }

    #[test]
    fn real_commit_failure_preserves_prior_moneylines() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection
            .execute("INSERT INTO schema_version (version) VALUES (1)", [])
            .unwrap();
        let mut store = Store {
            connection: Some(connection),
            path: PathBuf::from("odds-test.db"),
            clock: Arc::new(FixedClock),
        };
        store
            .replace_moneylines(
                &[10],
                &[MoneylineQuote {
                    game_pk: 10,
                    home_price: -140,
                    away_price: 125,
                    sportsbook: "Book".into(),
                }],
            )
            .unwrap();
        store
            .connection()
            .execute_batch(
                "CREATE TABLE commit_parent (id INTEGER PRIMARY KEY);
                 CREATE TABLE commit_child (
                    parent_id INTEGER,
                    FOREIGN KEY (parent_id) REFERENCES commit_parent(id)
                        DEFERRABLE INITIALLY DEFERRED
                 );
                 CREATE TRIGGER fail_moneyline_commit AFTER INSERT ON mlb_odds
                 BEGIN INSERT INTO commit_child (parent_id) VALUES (999); END;",
            )
            .unwrap();
        let error = store
            .replace_moneylines(
                &[10],
                &[MoneylineQuote {
                    game_pk: 10,
                    home_price: -120,
                    away_price: 105,
                    sportsbook: "Book".into(),
                }],
            )
            .unwrap_err();
        assert!(error.to_string().contains("commit transaction"));
        assert_eq!(
            store.moneylines_for_games(&[10]).unwrap()[&10].home_price,
            -140
        );
    }
}
