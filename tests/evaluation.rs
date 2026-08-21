use skout::analysis::{blend, statcast_blend, window_proj, wire_threshold};

#[test]
fn blend_and_shrink_follow_contract() {
    assert_eq!(blend::weights(10, true), (0.25, 0.75));
    assert!((statcast_blend::shrink(10.0, 10.0, 0.0, 10.0) - 5.0).abs() < 1e-9);
}

#[test]
fn percentile_and_windows_cover_fallbacks() {
    assert_eq!(
        wire_threshold::percentile(vec![1.0, 2.0, 3.0], 0.5),
        Some(2.0)
    );
    assert_eq!(window_proj::blend(Some(10.0), Some(0.0)), 7.0);
    assert_eq!(window_proj::blend(Some(10.0), None), 10.0);
    assert_eq!(window_proj::blend(None, None), 0.0);
    let projected = window_proj::PitcherWindow {
        ip: 100.0,
        qs: 0.0,
        w: 10.0,
        ..Default::default()
    };
    let recent = window_proj::PitcherWindow {
        ip: 10.0,
        qs: 2.0,
        w: 2.0,
        ..Default::default()
    };
    let next = window_proj::next_pitcher(Some(projected), Some(recent), 10.0);
    assert!((next.w - 1.3).abs() < 1e-9);
    assert_eq!(next.qs, 2.0);
    assert_eq!(
        window_proj::next_pitcher(None, None, 10.0),
        Default::default()
    );
}
