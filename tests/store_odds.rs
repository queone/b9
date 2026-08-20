use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use skout::store::{Clock, MoneylineQuote, Store};
use tempfile::tempdir;

struct FixedClock(SystemTime);

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

struct CountingClock(AtomicUsize);

impl Clock for CountingClock {
    fn now(&self) -> SystemTime {
        self.0.fetch_add(1, Ordering::SeqCst);
        UNIX_EPOCH + Duration::from_secs(1_800_000_000)
    }
}

fn quote(game_pk: i64, home: i64, away: i64, sportsbook: &str) -> MoneylineQuote {
    MoneylineQuote {
        game_pk,
        home_price: home,
        away_price: away,
        sportsbook: sportsbook.into(),
    }
}

#[test]
fn replacement_is_scoped_deduplicated_and_clocked_once() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("skout.db");
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock(now))).unwrap();
    store
        .replace_moneylines(&[20, 10, 20], &[quote(10, -140, 125, "Book")])
        .unwrap();
    let lines = store.moneylines_for_games(&[10, 20, 30]).unwrap();
    assert_eq!(lines.keys().copied().collect::<Vec<_>>(), vec![10]);
    assert_eq!(lines[&10].home_price, -140);
    assert_eq!(lines[&10].away_price, 125);
    assert_eq!(lines[&10].fetched_at, now);
    assert_eq!(store.latest_odds_fetch_time().unwrap(), Some(now));

    store
        .replace_moneylines(&[20], &[quote(20, -110, 100, "Other")])
        .unwrap();
    assert_eq!(store.moneylines_for_games(&[10, 20]).unwrap().len(), 2);
    store.replace_moneylines(&[10], &[]).unwrap();
    let lines = store.moneylines_for_games(&[10, 20]).unwrap();
    assert!(!lines.contains_key(&10));
    assert!(lines.contains_key(&20));
}

#[test]
fn replacement_preserves_non_moneyline_markets() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("skout.db");
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock(now))).unwrap();
    store
        .transaction(|transaction| {
            transaction.execute(
                "INSERT INTO mlb_odds (game_pk, market, side, line, price, player_mlbam_id, sportsbook, fetched_at)
                 VALUES (10, 'total', 'over', 8.5, -110, 0, 'Book', 1)",
                [],
            ).unwrap();
            Ok(())
        })
        .unwrap();
    store
        .replace_moneylines(&[10], &[quote(10, -140, 125, "Book")])
        .unwrap();
    assert_eq!(store.moneylines_for_games(&[10]).unwrap().len(), 1);
    store.close().unwrap();
    let connection = Connection::open(path).unwrap();
    let totals: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM mlb_odds WHERE market = 'total'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(totals, 1);
}

#[test]
fn invalid_replacements_leave_prior_rows_unchanged() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("skout.db");
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock(now))).unwrap();
    store
        .replace_moneylines(&[10], &[quote(10, -140, 125, "Book")])
        .unwrap();
    for (games, quotes) in [
        (vec![0], vec![]),
        (vec![10], vec![quote(11, -110, 100, "Book")]),
        (vec![10], vec![quote(10, 0, 100, "Book")]),
        (vec![10], vec![quote(10, -110, 100, &"x".repeat(129))]),
        (vec![10], vec![quote(10, -110, 100, "bad\nbook")]),
    ] {
        assert!(store.replace_moneylines(&games, &quotes).is_err());
        assert_eq!(
            store.moneylines_for_games(&[10]).unwrap()[&10].home_price,
            -140
        );
    }
}

#[test]
fn invalid_replacement_never_captures_the_clock() {
    let directory = tempdir().unwrap();
    let clock = Arc::new(CountingClock(AtomicUsize::new(0)));
    let mut store =
        Store::open_at_with_clock(directory.path().join("skout.db"), clock.clone()).unwrap();
    assert!(store.replace_moneylines(&[0], &[]).is_err());
    assert_eq!(clock.0.load(Ordering::SeqCst), 0);
}

#[test]
fn injected_insert_failure_rolls_back_delete() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("skout.db");
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock(now))).unwrap();
    store
        .replace_moneylines(&[10], &[quote(10, -140, 125, "Book")])
        .unwrap();
    store
        .transaction(|transaction| {
            transaction
                .execute_batch(
                    "CREATE TRIGGER fail_moneyline_insert BEFORE INSERT ON mlb_odds
                 BEGIN SELECT RAISE(ABORT, 'injected insert failure'); END;",
                )
                .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(
        store
            .replace_moneylines(&[10], &[quote(10, -120, 105, "Book")])
            .is_err()
    );
    assert_eq!(
        store.moneylines_for_games(&[10]).unwrap()[&10].home_price,
        -140
    );
}

#[test]
fn injected_delete_failure_preserves_prior_rows() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("skout.db");
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock(now))).unwrap();
    store
        .replace_moneylines(&[10], &[quote(10, -140, 125, "Book")])
        .unwrap();
    store
        .transaction(|transaction| {
            transaction
                .execute_batch(
                    "CREATE TRIGGER fail_moneyline_delete BEFORE DELETE ON mlb_odds
                     BEGIN SELECT RAISE(ABORT, 'injected delete failure'); END;",
                )
                .unwrap();
            Ok(())
        })
        .unwrap();
    assert!(store.replace_moneylines(&[10], &[]).is_err());
    assert_eq!(
        store.moneylines_for_games(&[10]).unwrap()[&10].home_price,
        -140
    );
}

#[test]
fn corrupt_stored_moneylines_are_contextual_errors() {
    for statement in [
        "UPDATE mlb_odds SET price = 0 WHERE side = 'home'",
        "UPDATE mlb_odds SET fetched_at = 0 WHERE side = 'home'",
        "UPDATE mlb_odds SET sportsbook = char(10) WHERE side = 'home'",
        "DELETE FROM mlb_odds WHERE side = 'away'",
        "PRAGMA ignore_check_constraints = ON; UPDATE mlb_odds SET market = 'bad' WHERE side = 'home'",
        "PRAGMA ignore_check_constraints = ON; UPDATE mlb_odds SET side = 'bad' WHERE side = 'home'",
    ] {
        let directory = tempdir().unwrap();
        let path = directory.path().join("skout.db");
        let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let mut store = Store::open_at_with_clock(&path, Arc::new(FixedClock(now))).unwrap();
        store
            .replace_moneylines(&[10], &[quote(10, -140, 125, "Book")])
            .unwrap();
        store.close().unwrap();
        Connection::open(&path)
            .unwrap()
            .execute_batch(statement)
            .unwrap();
        let store = Store::open_at(&path).unwrap();
        let error = store.moneylines_for_games(&[10]).unwrap_err();
        assert!(error.to_string().contains("moneyline"), "{error}");
    }
}
