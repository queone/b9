pub const PROJECTION_WEIGHT: f64 = 0.70;
pub const RECENT_WEIGHT: f64 = 0.30;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HitterWindow {
    pub pa: f64,
    pub r: f64,
    pub hr: f64,
    pub rbi: f64,
    pub sb: f64,
    pub avg: f64,
    pub obp: f64,
    pub ops: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PitcherWindow {
    pub ip: f64,
    pub qs: f64,
    pub w: f64,
    pub sv: f64,
    pub k: f64,
    pub era: f64,
    pub whip: f64,
}

/// Scale and blend a full hitter projection with a recent window.
#[must_use]
pub fn next_hitter(
    projection: Option<HitterWindow>,
    recent: Option<HitterWindow>,
    pa: f64,
) -> HitterWindow {
    if pa <= 0.0 {
        return HitterWindow::default();
    }
    let scaled = |window: HitterWindow| {
        let scale = pa / window.pa;
        HitterWindow {
            pa,
            r: window.r * scale,
            hr: window.hr * scale,
            rbi: window.rbi * scale,
            sb: window.sb * scale,
            avg: window.avg,
            obp: window.obp,
            ops: window.ops,
        }
    };
    match (
        projection.filter(|v| v.pa > 0.0).map(scaled),
        recent.filter(|v| v.pa > 0.0).map(scaled),
    ) {
        (Some(p), Some(r)) => HitterWindow {
            pa,
            r: blend(Some(p.r), Some(r.r)),
            hr: blend(Some(p.hr), Some(r.hr)),
            rbi: blend(Some(p.rbi), Some(r.rbi)),
            sb: blend(Some(p.sb), Some(r.sb)),
            avg: blend(Some(p.avg), Some(r.avg)),
            obp: blend(Some(p.obp), Some(r.obp)),
            ops: blend(Some(p.ops), Some(r.ops)),
        },
        (Some(v), None) | (None, Some(v)) => v,
        _ => HitterWindow::default(),
    }
}

/// Scale and blend a full pitcher projection; QS always comes from recent use.
#[must_use]
pub fn next_pitcher(
    projection: Option<PitcherWindow>,
    recent: Option<PitcherWindow>,
    ip: f64,
) -> PitcherWindow {
    if ip <= 0.0 {
        return PitcherWindow::default();
    }
    let scaled = |window: PitcherWindow| {
        let scale = ip / window.ip;
        PitcherWindow {
            ip,
            qs: window.qs * scale,
            w: window.w * scale,
            sv: window.sv * scale,
            k: window.k * scale,
            era: window.era,
            whip: window.whip,
        }
    };
    match (
        projection.filter(|v| v.ip > 0.0).map(scaled),
        recent.filter(|v| v.ip > 0.0).map(scaled),
    ) {
        (Some(p), Some(r)) => PitcherWindow {
            ip,
            qs: r.qs,
            w: blend(Some(p.w), Some(r.w)),
            sv: blend(Some(p.sv), Some(r.sv)),
            k: blend(Some(p.k), Some(r.k)),
            era: blend(Some(p.era), Some(r.era)),
            whip: blend(Some(p.whip), Some(r.whip)),
        },
        (Some(v), None) => PitcherWindow { qs: 0.0, ..v },
        (None, Some(v)) => v,
        _ => PitcherWindow::default(),
    }
}
#[must_use]
pub fn blend(projection: Option<f64>, recent: Option<f64>) -> f64 {
    match (projection, recent) {
        (Some(p), Some(r)) => p * PROJECTION_WEIGHT + r * RECENT_WEIGHT,
        (Some(p), None) => p,
        (None, Some(r)) => r,
        _ => 0.0,
    }
}
