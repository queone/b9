use b9::glossary::{
    GlossaryEntry, LookupResult, embedded_entries, lookup, parse_glossary, render_entry,
    render_full, suggest_keys,
};

const EXPECTED_KEYS: &str = "ab abandon active age atc available avg avg162g babip barrel_pct batters_faced bb bb_pct bench blend_window category_strategy cfip ch_pct close closer confirmed confirmed_sp cs dtd ecr empirical_bayes era exit_velo expected faab fastball_velo fb_pct fip flippable fwar g game-log gb_pct gs h2h hard_hit_pct hbp holds hr hr_fb il injured ip k k9 k_bb_pct k_pct launch_angle lineup_candidates lineup_status lost na no_game not_scheduled obp opportunity_damping ops out own_pct p_slot pa pitcher_day_state pool pos pp ppd pqs probable probable_sp protect punt push qs r rbi replacement_level roster_moves roster_moves_note roster_slot rp_available rp_slot savant sb slg sp_slot spin_rate sprint_speed stabilization_ramp steamer streaming sv sweet_spot_pct tied w waiver_wire whiff_pct whip wrc_plus xba xera xfip xobp xslg xwoba yp yr z_score zips";

fn entry(key: &str, term: &str, aliases: &[&str]) -> GlossaryEntry {
    GlossaryEntry {
        key: key.to_owned(),
        term: term.to_owned(),
        class: "test".to_owned(),
        aliases: aliases.iter().map(|alias| (*alias).to_owned()).collect(),
        definition: format!("Definition for {key}."),
    }
}

#[test]
fn embedded_glossary_has_the_exact_baseline_entries() {
    let entries = embedded_entries().expect("parse embedded glossary");
    let mut actual: Vec<_> = entries.iter().map(|entry| entry.key.as_str()).collect();
    actual.sort_unstable();
    let expected: Vec<_> = EXPECTED_KEYS.split_whitespace().collect();
    assert_eq!(actual, expected);
    assert_eq!(entries.len(), 113);

    let pa = entries.iter().find(|entry| entry.key == "pa").unwrap();
    assert_eq!(pa.aliases, ["PA"]);
    assert!(pa.definition.contains("turn at bat — includes"));
    assert!(entries.iter().any(|entry| entry.key == "game-log"));
}

#[test]
fn parser_accepts_recorded_checklist_drift_and_rejects_bad_entries() {
    let source = "# Glossary\n\n## Coverage Checklist\n\n`missing`\n\n### Term (`key`) [class]\n\nDefinition.\n";
    assert_eq!(parse_glossary(source).unwrap().len(), 1);

    for (source, message) in [
        (
            "### Broken heading\nDefinition.\n",
            "malformed entry heading",
        ),
        ("### Term (``) [class]\nDefinition.\n", "must be non-empty"),
        ("### Term (`key`) [class]\n", "empty definition"),
        (
            "### One (`key`) [class]\nOne.\n### Two (`key`) [class]\nTwo.\n",
            "duplicate entry key",
        ),
    ] {
        let error = parse_glossary(source).expect_err("reject malformed glossary");
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn lookup_uses_exact_precedence_then_source_order_substrings() {
    let entries = vec![
        entry("alpha", "Shared", &["first"]),
        entry("shared", "Second", &["alias"]),
        entry("third", "Alias", &["shared"]),
    ];
    assert!(
        matches!(lookup(&entries, "shared"), LookupResult::Match(value) if value.key == "shared")
    );
    assert!(
        matches!(lookup(&entries, "second"), LookupResult::Match(value) if value.key == "shared")
    );
    assert!(
        matches!(lookup(&entries, "alias"), LookupResult::Match(value) if value.key == "third")
    );
    match lookup(&entries, "s") {
        LookupResult::Ambiguous(values) => {
            assert_eq!(
                values
                    .iter()
                    .map(|value| value.key.as_str())
                    .collect::<Vec<_>>(),
                ["alpha", "shared", "third"]
            );
        }
        result => panic!("expected ambiguity, got {result:?}"),
    }
}

#[test]
fn suggestions_use_unicode_scalar_distance_and_lexicographic_ties() {
    let entries = vec![
        entry("café", "Cafe", &[]),
        entry("caff", "Caff", &[]),
        entry("case", "Case", &[]),
        entry("zzzz", "Zed", &[]),
    ];
    assert_eq!(suggest_keys(&entries, "cafe", 3), ["caff", "café", "case"]);
}

#[test]
fn rendering_is_plain_ordered_and_has_no_trailing_blank_line() {
    let plain = entry("b", "Beta", &["B"]);
    assert_eq!(
        render_entry(&plain),
        "Beta (b) [test]\nAliases: B\nDefinition for b."
    );
    let entries = vec![
        entry("z", "Zulu", &[]),
        GlossaryEntry {
            class: "baseball".into(),
            ..entry("b", "Beta", &[])
        },
        GlossaryEntry {
            class: "baseball".into(),
            ..entry("a", "Alpha", &[])
        },
    ];
    let output = render_full(&entries);
    assert!(output.starts_with("BASEBALL\n\nAlpha (a)"));
    assert!(output.contains("\n\nBeta (b)"));
    assert!(output.contains("\n\nTEST\n\nZulu (z)"));
    assert!(!output.ends_with("\n\n"));
    assert!(!output.contains("\u{1b}["));
}
