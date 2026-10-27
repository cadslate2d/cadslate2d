//! A building section drawn (#338, ADR-0040): what `cad_layout::section`
//! cuts, placed in the drawing — the left end of the floor at a point,
//! scaled to the drawing's unit — the cut outlined and hatched, what lies
//! beyond outlined, the stair in section where the cut runs up its first
//! flight; and on the plan, the cut line with its arrows and letters.

use cad_core::{Point2, Vec2};
use cad_document::{
    Entity, HatchEntity, HatchStyle, Justification, LineEntity, PolylineEntity, TextEntity,
};
use cad_geometry::Path;
use cad_geometry::stair::Placement;
use cad_layout::{Cut, Layout, Section, Spec};

use crate::floorplan::plan_stair;

pub const CUT: &str = "SECT-CUT";
pub const HATCH: &str = "SECT-HATCH";
pub const BEYOND: &str = "SECT-BEYOND";
pub const GROUND: &str = "SECT-GROUND";
pub const STAIR: &str = "SECT-STAIR";
pub const TEXT: &str = "SECT-TEXT";
pub const MARK: &str = "SECT-MARK";
pub const LAYERS: [&str; 7] = [CUT, HATCH, BEYOND, GROUND, STAIR, TEXT, MARK];

/// The prefix of a section's group: `SECT-1`, `SECT-2`, …
pub const GROUP: &str = "SECT-";

/// Text heights, metres: the section's title, the plan's letters.
const TITLE_HEIGHT: f64 = 0.3;
const LETTER_HEIGHT: f64 = 0.4;
/// The cut hatch's spacing, metres: ANSI31's lines are 3.175 units apart.
const HATCH_SPACING: f64 = 0.08;

/// How a cut meets a stair the plan drew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Meets {
    /// Not at all.
    Clear,
    /// Up its first flight, where its profile is drawn.
    FirstFlight,
    /// Anywhere else: across a flight, up the second, along a landing.
    Elsewhere,
}

/// The stair `placed` as `spec` sizes it.
fn stair_of(spec: &Spec, placed: &Placement) -> cad_geometry::stair::Stair {
    cad_geometry::stair::Stair {
        kind: placed.kind,
        ..plan_stair(spec)
    }
}

/// The way `placed` climbs, in plan, along and across the cut, and its
/// foot's place along and across it. Its turn is a quarter turn, so the
/// climb is along the cut or across it, never between.
fn climb(placed: &Placement, cut: Cut) -> (f64, f64, f64, f64) {
    let (sin, cos) = placed.turn.sin_cos();
    let climb = Vec2::new(-sin, cos);
    if cut.along_x {
        (climb.x, climb.y, placed.foot.x, placed.foot.y)
    } else {
        (climb.y, climb.x, placed.foot.y, placed.foot.x)
    }
}

fn meets(spec: &Spec, placed: &Placement, cut: Cut) -> Meets {
    let stair = stair_of(spec, placed);
    let Ok(footprint) = stair.footprint() else {
        return Meets::Clear;
    };
    // The footprint across the cut, in plan.
    let (sin, cos) = placed.turn.sin_cos();
    let across = |q: Point2| {
        let p = placed.foot + Vec2::new(q.x * cos - q.y * sin, q.x * sin + q.y * cos);
        if cut.along_x { p.y } else { p.x }
    };
    let corners = footprint.corners().map(across);
    let (lo, hi) = corners
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &c| {
            (l.min(c), h.max(c))
        });
    if !(cut.at > lo && cut.at < hi) {
        return Meets::Clear;
    }
    let (along, _, _, foot_across) = climb(placed, cut);
    if along.abs() > 0.5 && (cut.at - foot_across).abs() < stair.width / 2.0 {
        Meets::FirstFlight
    } else {
        Meets::Elsewhere
    }
}

/// Whether the cut meets a stair where its section is not drawn: such a
/// cut is refused rather than drawn without the stair it passes through
/// (ADR-0040 §4).
pub fn meets_a_stair_elsewhere(spec: &Spec, stairs: &[(usize, Placement)], cut: Cut) -> bool {
    stairs
        .iter()
        .any(|(_, placed)| meets(spec, placed, cut) == Meets::Elsewhere)
}

/// The stair's profile on the section, where the cut runs up its first
/// flight: its treads and underside, in the section's metres. `stairs`
/// are where the plan's stairs were drawn.
fn stair_profile(
    spec: &Spec,
    stairs: &[(usize, Placement)],
    cut: Cut,
    section: &Section,
) -> Vec<Vec<Point2>> {
    let mut out = Vec::new();
    for (_, placed) in stairs {
        if meets(spec, placed, cut) != Meets::FirstFlight {
            continue;
        }
        let Ok(profile) = stair_of(spec, placed).cut_profile() else {
            continue;
        };
        let (along, _, foot_along, _) = climb(placed, cut);
        let u0 = section.u(foot_along);
        let sign = along.signum() * section.direction;
        out.extend(profile.into_iter().map(|line| {
            line.into_iter()
                .map(|q| Point2::new(u0 + sign * q.x, q.y))
                .collect()
        }));
    }
    out
}

/// What `section` is drawn as, each entity with the index of its layer in
/// [`LAYERS`]: the floor's left end at `at`, `scale` drawing units to the
/// metre, titled with `letter`.
pub fn entities(
    spec: &Spec,
    stairs: &[(usize, Placement)],
    cut: Cut,
    section: &Section,
    letter: char,
    at: Point2,
    scale: f64,
) -> Vec<(usize, Entity)> {
    let place = |q: Point2| at + Vec2::new(q.x * scale, q.y * scale);
    let mut out: Vec<(usize, Entity)> = Vec::new();
    for ring in &section.cut {
        let points: Vec<Point2> = ring.iter().map(|q| place(*q)).collect();
        out.push((0, PolylineEntity::new(points.clone(), true).into()));
        let path = Path {
            bulges: vec![0.0; points.len()],
            vertices: points,
            closed: true,
        };
        let hatch = HatchEntity::solid(vec![path]).styled(HatchStyle::named(
            "ANSI31",
            0.0,
            HATCH_SPACING * scale / 3.175,
        ));
        out.push((1, hatch.into()));
    }
    for ring in &section.beyond {
        let points = ring.iter().map(|q| place(*q)).collect();
        out.push((2, PolylineEntity::new(points, true).into()));
    }
    for (a, b) in &section.lines {
        out.push((0, LineEntity::new(place(*a), place(*b)).into()));
    }
    let (a, b) = section.ground;
    out.push((3, LineEntity::new(place(a), place(b)).into()));
    for line in stair_profile(spec, stairs, cut, section) {
        let points = line.into_iter().map(place).collect();
        out.push((4, PolylineEntity::new(points, false).into()));
    }
    let mut title = TextEntity::new(
        place(Point2::new(
            section.width / 2.0,
            -2.5 * TITLE_HEIGHT - spec.building.slab,
        )),
        TITLE_HEIGHT * scale,
        0.0,
        format!("SECTION {letter}-{letter}"),
    );
    title.justification = Justification::Center;
    out.push((5, title.into()));
    out
}

/// The cut's mark on the plan, on [`MARK`]: its line across the building
/// and a metre past it, an arrow at each end towards where it looks, and
/// its letter by each arrow. `origin` and `scale` are the plan's.
pub fn mark(
    layout: &Layout,
    cut: Cut,
    letter: char,
    origin: Point2,
    scale: f64,
) -> Vec<(usize, Entity)> {
    let place = |x: f64, y: f64| Point2::new(origin.x + x * scale, origin.y + y * scale);
    let fp = layout.footprint;
    let past = 1.0;
    let (ends, look) = if cut.along_x {
        (
            [(fp.x - past, cut.at), (fp.right() + past, cut.at)],
            Vec2::new(0.0, if cut.looking_up { 1.0 } else { -1.0 }),
        )
    } else {
        (
            [(cut.at, fp.y - past), (cut.at, fp.top() + past)],
            Vec2::new(if cut.looking_up { 1.0 } else { -1.0 }, 0.0),
        )
    };
    let mut out: Vec<(usize, Entity)> = vec![(
        6,
        LineEntity::new(place(ends[0].0, ends[0].1), place(ends[1].0, ends[1].1)).into(),
    )];
    for (x, y) in ends {
        let tip = (x + look.x * 0.6, y + look.y * 0.6);
        out.push((6, LineEntity::new(place(x, y), place(tip.0, tip.1)).into()));
        let back = Vec2::new(-look.x * 0.2, -look.y * 0.2);
        let side = Vec2::new(-look.y * 0.1, look.x * 0.1);
        out.push((
            6,
            PolylineEntity::new(
                vec![
                    place(tip.0 + back.x + side.x, tip.1 + back.y + side.y),
                    place(tip.0, tip.1),
                    place(tip.0 + back.x - side.x, tip.1 + back.y - side.y),
                ],
                false,
            )
            .into(),
        ));
        let mut text = TextEntity::new(
            place(x + look.x * 1.0, y + look.y * 1.0),
            LETTER_HEIGHT * scale,
            0.0,
            letter.to_string(),
        );
        text.justification = Justification::Center;
        out.push((6, text.into()));
    }
    out
}

/// What a section's group says of it: the plan, the cut, its letter, and
/// where, as JSON.
pub fn description(plan: &str, cut: Cut, letter: char, at: Point2, scale: f64) -> String {
    serde_json::json!({
        "plan": plan,
        "cut": { "along_x": cut.along_x, "at": cut.at, "looking_up": cut.looking_up },
        "letter": letter.to_string(),
        "at": [at.x, at.y],
        "scale": scale,
    })
    .to_string()
}
