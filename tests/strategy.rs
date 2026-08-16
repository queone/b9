use b9::strategy::{is_punted, normalized_punts};

#[test]
fn strategy_punts_are_case_insensitive_and_normalized() {
    let punts = vec![" ERA ".into(), "era".into(), "".into(), "WHIP".into()];
    assert!(is_punted("era", &punts));
    assert!(is_punted("whip", &punts));
    assert_eq!(normalized_punts(&punts), vec!["era", "whip"]);
}
