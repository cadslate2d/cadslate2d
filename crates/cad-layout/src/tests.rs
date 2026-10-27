use super::*;
use crate::search::DOOR_CLEARANCE;
use cad_core::Point2;

/// `default` cases, or `PROPTEST_CASES` when it is set: the search is too
/// slow for proptest's 256 in the check, and the stress run asks for more.
fn cases(default: u32) -> proptest::prelude::ProptestConfig {
    let wanted = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(default);
    proptest::prelude::ProptestConfig::with_cases(wanted)
}

fn brief(width: f64, depth: f64, rooms: &[(&str, f64)]) -> Spec {
    Spec {
        terrain: Terrain { width, depth },
        setbacks: Setbacks::default(),
        rooms: rooms.iter().map(|(n, a)| Room::new(*n, *a)).collect(),
        adjacent: Vec::new(),
        exterior_wall: 0.25,
        interior_wall: 0.15,
        door: 0.9,
        max_circulation: 0.15,
        windows: None,
        storey_height: 2.8,
        building: crate::Building::default(),
        seed: 0,
    }
}

#[test]
fn a_small_house_is_laid_out_and_keeps_every_hard_constraint() {
    let mut spec = brief(
        15.0,
        20.0,
        &[
            ("living", 24.0),
            ("kitchen", 10.0),
            ("bedroom", 12.0),
            ("bath", 4.5),
        ],
    );
    spec.adjacent = vec![
        ["living".into(), "kitchen".into()],
        ["bedroom".into(), "bath".into()],
    ];
    let layout = solve(&spec).unwrap();
    assert_eq!(verify(&spec, &layout), Vec::<String>::new());
    // The same brief and seed, the same plan.
    assert_eq!(solve(&spec).unwrap(), layout);
}

#[test]
fn what_has_no_plan_is_refused_as_infeasible_with_the_reason() {
    let spec = brief(8.0, 8.0, &[("living", 40.0), ("bedroom", 30.0)]);
    match solve(&spec).unwrap_err() {
        Refusal::Infeasible { reasons } => {
            assert!(
                reasons[0].starts_with("The rooms need at least 66.5 m²"),
                "{reasons:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    let mut spec = brief(20.0, 20.0, &[("bath", 2.0)]);
    spec.rooms[0].min_width = 1.6;
    assert_eq!(
        solve(&spec).unwrap_err(),
        Refusal::Infeasible {
            reasons: vec!["The bath's 2 m² cannot be 1.6 m wide both ways.".into()]
        }
    );
    // An unreadable brief is neither.
    let spec = brief(20.0, 20.0, &[("a", 10.0), ("a", 10.0)]);
    assert_eq!(
        solve(&spec).unwrap_err(),
        Refusal::Invalid("Two rooms are called a.".into())
    );
}

/// Whether `spec` is refused as infeasible for a reason containing `what`.
fn refused_for(spec: &Spec, what: &str) -> bool {
    matches!(solve(spec), Err(Refusal::Infeasible { reasons }) if reasons.iter().any(|r| r.contains(what)))
}

/// Each necessary condition holds a brief just past its bound, and lets one
/// just inside it go on to the search: they are bounds, not guesses.
#[test]
fn each_necessary_condition_refuses_just_past_its_bound_and_not_inside_it() {
    let (inside, past) = (1.0 - 1e-6, 1.0 + 1e-6);
    let rooms = |n: usize, area: f64| -> Vec<(String, f64)> {
        (1..=n).map(|i| (format!("room {i}"), area)).collect()
    };
    let spec_of = |w: f64, d: f64, rooms: &[(String, f64)]| {
        let named: Vec<(&str, f64)> = rooms.iter().map(|(n, a)| (n.as_str(), *a)).collect();
        brief(w, d, &named)
    };

    // 1. The setbacks leave room inside the exterior walls: 0.5 m across.
    let mut spec = spec_of(10.0, 10.0, &rooms(1, 4.0));
    spec.setbacks.left = 9.5 * inside;
    assert!(!refused_for(&spec, "The setbacks leave no room"));
    spec.setbacks.left = 9.5;
    assert!(refused_for(&spec, "The setbacks leave no room"));

    // 2. 95% of the floor inside the walls: 9.5 × 9.5 = 90.25 m².
    let spec = spec_of(10.0, 10.0, &rooms(1, 95.0 * inside));
    assert!(!refused_for(&spec, "The rooms need at least"));
    let spec = spec_of(10.0, 10.0, &rooms(1, 95.0 * past));
    assert!(refused_for(&spec, "The rooms need at least"));

    // 3. Twelve rooms with a wall round each in a 9.75 m square outline.
    let bound = (((9.75f64 * 9.75 / 12.0).sqrt() - 0.15).powi(2)) / 0.95;
    assert!(!refused_for(
        &spec_of(10.0, 10.0, &rooms(12, bound * inside)),
        "With their walls"
    ));
    assert!(refused_for(
        &spec_of(10.0, 10.0, &rooms(12, bound * past)),
        "With their walls"
    ));

    // 4. 105% of the area is at least the minimum width squared.
    let mut spec = spec_of(20.0, 20.0, &rooms(1, 2.56 / 1.05 * past));
    spec.rooms[0].min_width = 1.6;
    assert!(!refused_for(&spec, "wide both ways"));
    spec.rooms[0].area = 2.56 / 1.05 * inside;
    assert!(refused_for(&spec, "wide both ways"));

    // 5. The minimum width across the buildable area: 6 − 0.5 = 5.5 m.
    let mut spec = spec_of(20.0, 6.0, &rooms(1, 40.0));
    spec.rooms[0].min_width = 5.5 * inside;
    assert!(!refused_for(&spec, "the buildable area is 5.5 m across"));
    spec.rooms[0].min_width = 5.5 * past;
    assert!(refused_for(&spec, "the buildable area is 5.5 m across"));

    // 6. 95% of the area within its aspect ratio: 5.5 × (2.5 × 5.5).
    let bound = 5.5 * 2.5 * 5.5 / 0.95;
    assert!(!refused_for(
        &spec_of(30.0, 6.0, &rooms(1, bound * inside)),
        "within its aspect ratio"
    ));
    assert!(refused_for(
        &spec_of(30.0, 6.0, &rooms(1, bound * past)),
        "within its aspect ratio"
    ));

    // 7. Six doors of 1.45 m round a hub at least 1.2 m wide: its longest
    // perimeter, 2 × (1.2 + 1.05·A / 1.2 + 0.5), is 8.7 m at A = 2.65 × 1.2 / 1.05.
    let hub_with = |area: f64| {
        let mut spec = spec_of(20.0, 20.0, &rooms(5, 10.0));
        let mut hub = Room::new("hub", area);
        hub.min_width = 1.2;
        hub.entrance = true;
        spec.rooms.insert(0, hub);
        spec.adjacent = (1..=5)
            .map(|i| ["hub".to_owned(), format!("room {i}")])
            .collect();
        spec
    };
    let bound = 2.65 * 1.2 / 1.05;
    assert!(!refused_for(&hub_with(bound * past), "too little wall"));
    assert!(refused_for(&hub_with(bound * inside), "too little wall"));

    // 8. A minimum length (#331): 105% of the area at least the width by
    // the length, 1.5 × 4.
    let long_room = |w: f64, d: f64, area: f64, length: f64| {
        let mut spec = spec_of(w, d, &rooms(1, area));
        spec.rooms[0].min_length = length;
        spec
    };
    let bound = 1.5 * 4.0 / 1.05;
    assert!(!refused_for(
        &long_room(20.0, 20.0, bound * past, 4.0),
        "m wide and"
    ));
    assert!(refused_for(
        &long_room(20.0, 20.0, bound * inside, 4.0),
        "m wide and"
    ));
    // 9. The length along the buildable area: 20 − 0.5 = 19.5 m.
    let what = "m long: the buildable area is 19.5 m long";
    assert!(!refused_for(
        &long_room(20.0, 6.0, 40.0, 19.5 * inside),
        what
    ));
    assert!(refused_for(&long_room(20.0, 6.0, 40.0, 19.5 * past), what));
    // 10. The length within the aspect ratio: 5² ≤ 2.5 × 105% of the area.
    let bound = 25.0 / (2.5 * 1.05);
    let what = "m long within its aspect ratio";
    assert!(!refused_for(
        &long_room(20.0, 20.0, bound * past, 5.0),
        what
    ));
    assert!(refused_for(
        &long_room(20.0, 20.0, bound * inside, 5.0),
        what
    ));
}

#[test]
fn a_room_given_a_minimum_length_is_laid_out_at_least_that_long() {
    let mut spec = brief(
        12.0,
        15.0,
        &[("living", 20.0), ("stair", 8.0), ("bath", 4.5)],
    );
    spec.rooms[0].entrance = true;
    spec.rooms[1].min_width = 1.8;
    spec.rooms[1].min_length = 3.8;
    spec.adjacent = vec![
        ["living".into(), "stair".into()],
        ["living".into(), "bath".into()],
    ];
    let layout = solve(&spec).unwrap();
    assert_eq!(verify(&spec, &layout), Vec::<String>::new());
    let floor = layout.rooms[1].inner;
    assert!(floor.w.max(floor.h) >= 3.8 - 1e-9, "{floor:?}");
    assert!(floor.w.min(floor.h) >= 1.8 - 1e-9, "{floor:?}");
}

#[test]
fn walls_that_cannot_fit_are_proved_and_a_search_that_finds_nothing_is_exhausted() {
    // Twelve rooms whose floor fits the land: with a wall round each, they
    // do not, and that is proved before any search.
    let rooms: Vec<(String, f64)> = (1..=12).map(|i| (format!("room {i}"), 7.5)).collect();
    let named: Vec<(&str, f64)> = rooms.iter().map(|(n, a)| (n.as_str(), *a)).collect();
    match solve(&brief(10.0, 10.0, &named)).unwrap_err() {
        Refusal::Infeasible { reasons } => {
            assert!(reasons[0].starts_with("With their walls the rooms cover at least"));
        }
        other => panic!("{other:?}"),
    }
    // Twenty-four that pass every bound, and that no search lays out.
    let rooms: Vec<(String, f64)> = (1..=24).map(|i| (format!("room {i}"), 7.0)).collect();
    let named: Vec<(&str, f64)> = rooms.iter().map(|(n, a)| (n.as_str(), *a)).collect();
    match alternatives_within(&brief(14.0, 14.0, &named), 1, 2).unwrap_err() {
        Refusal::Exhausted { reasons, searches } => {
            assert_eq!(searches, 2);
            assert!(!reasons.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_budget_is_forty_searches_to_six_rooms_then_fewer() {
    assert_eq!(searches_for(1), 40);
    assert_eq!(searches_for(6), 40);
    assert_eq!(searches_for(12), 20);
    assert_eq!(searches_for(24), 10);
    assert_eq!(searches_for(100), 8);
}

#[test]
fn circulation_stays_within_the_budget_and_not_at_all_when_forbidden() {
    let mut spec = brief(10.0, 12.0, &[("living", 25.0), ("bath", 4.0)]);
    spec.rooms[0].entrance = true;
    spec.adjacent = vec![["living".into(), "bath".into()]];
    let plan = solve(&spec).unwrap();
    assert!(verify(&spec, &plan).is_empty());
    if let Some(c) = plan.circulation {
        assert!(plan.rooms[c].area() <= 0.15 * 29.0 + 1e-9);
    }
    spec.max_circulation = 0.0;
    match solve(&spec) {
        Ok(plan) => assert_eq!(plan.circulation, None),
        Err(refusal) => assert!(matches!(refusal, Refusal::Exhausted { .. }), "{refusal:?}"),
    }
}

#[test]
fn alternatives_differ_and_come_best_first() {
    let spec = brief(
        14.0,
        14.0,
        &[
            ("living", 25.0),
            ("kitchen", 10.0),
            ("bed 1", 12.0),
            ("bed 2", 11.0),
            ("bath", 5.0),
        ],
    );
    let plans = alternatives(&spec, 3).unwrap();
    assert!(plans.len() >= 2, "{}", plans.len());
    for pair in plans.windows(2) {
        assert!(pair[0].energy <= pair[1].energy);
        assert_ne!(pair[0].rooms, pair[1].rooms);
    }
    for plan in &plans {
        assert!(verify(&spec, plan).is_empty());
    }
}

proptest::proptest! {
    #![proptest_config(cases(24))]

    /// Whatever the brief, a plan keeps every hard constraint, and the same
    /// seed makes the same plan; a brief with no plan says why.
    #[test]
    fn every_plan_keeps_the_hard_constraints(
        width in 8.0..30.0f64,
        depth in 8.0..30.0f64,
        areas in proptest::collection::vec(3.0..30.0f64, 1..7),
        seed in 0..1000u64,
    ) {
        let rooms: Vec<(String, f64)> =
            areas.iter().enumerate().map(|(i, a)| (format!("room {i}"), *a)).collect();
        let named: Vec<(&str, f64)> = rooms.iter().map(|(n, a)| (n.as_str(), *a)).collect();
        let mut spec = brief(width, depth, &named);
        spec.seed = seed;
        match alternatives_within(&spec, 1, 2) {
            Ok(plans) => {
                proptest::prop_assert_eq!(verify(&spec, &plans[0]), Vec::<String>::new());
                proptest::prop_assert_eq!(&alternatives_within(&spec, 1, 2).unwrap(), &plans);
            }
            Err(refused) => proptest::prop_assert!(!refused.reasons().is_empty()),
        }
    }
}

proptest::proptest! {
    #![proptest_config(cases(32))]

    /// Briefs with setbacks, minimum widths, aspect ratios, adjacencies, an
    /// entrance, and a circulation budget: every alternative keeps every
    /// hard constraint, the alternatives differ and come best first —
    /// without circulation before with — and a refusal is never empty.
    #[test]
    fn every_alternative_keeps_the_hard_constraints_and_they_are_ranked(
        width in 8.0..30.0f64,
        depth in 8.0..30.0f64,
        setbacks in proptest::collection::vec(0.0..3.0f64, 4),
        rooms in proptest::collection::vec((3.0..30.0f64, 1.0..2.5f64, 1.5..4.0f64, proptest::bool::ANY), 1..8),
        pairs in proptest::collection::vec((0..8usize, 0..8usize), 0..5),
        entrance in 0..8usize,
        max_circulation in prop_oneof_circulation(),
        seed in 0..1000u64,
    ) {
        let mut spec = brief(width, depth, &[]);
        spec.setbacks = Setbacks { front: setbacks[0], back: setbacks[1], left: setbacks[2], right: setbacks[3] };
        spec.rooms = rooms
            .iter()
            .enumerate()
            .map(|(i, (area, min_width, max_aspect, exterior))| Room {
                min_width: *min_width,
                max_aspect: *max_aspect,
                exterior: *exterior,
                entrance: i == entrance % rooms.len(),
                ..Room::new(format!("room {i}"), *area)
            })
            .collect();
        let n = spec.rooms.len();
        spec.adjacent = pairs
            .iter()
            .map(|(a, b)| (a % n, b % n))
            .filter(|(a, b)| a != b)
            .map(|(a, b)| [format!("room {a}"), format!("room {b}")])
            .collect();
        spec.max_circulation = max_circulation;
        spec.seed = seed;
        match alternatives_within(&spec, 3, 6) {
            Ok(plans) => {
                proptest::prop_assert!(!plans.is_empty() && plans.len() <= 3);
                for plan in &plans {
                    proptest::prop_assert_eq!(verify(&spec, plan), Vec::<String>::new());
                }
                for pair in plans.windows(2) {
                    proptest::prop_assert!(pair[0].rooms != pair[1].rooms);
                    let rank = |p: &Layout| (p.circulation.is_some(), p.energy);
                    proptest::prop_assert!(rank(&pair[0]) <= rank(&pair[1]));
                }
            }
            Err(refusal) => proptest::prop_assert!(!refusal.reasons().is_empty()),
        }
    }
}

fn prop_oneof_circulation() -> impl proptest::strategy::Strategy<Value = f64> {
    use proptest::strategy::Strategy;
    (0..3u8).prop_map(|k| [0.0, 0.15, 0.3][k as usize])
}

// ── The verifier, against plans made by hand to break it ──

/// The outline's lower left corner, with a 0.25 exterior wall from (0, 0).
const O: f64 = 0.125;

/// A plan of these centreline rectangles, each room's floor worked out
/// here and taken as its area, every room `min_width` 1 and `max_aspect`
/// 10, so that only what a test breaks is broken.
fn by_hand(rects: &[Rect], outline: Rect, doors: &[(usize, usize)]) -> (Spec, Layout) {
    let (te, ti) = (0.25, 0.15);
    let near = |a: f64, b: f64| (a - b).abs() < 1e-12;
    let floors: Vec<Rect> = rects
        .iter()
        .map(|r| {
            let half = |out: bool| if out { te / 2.0 } else { ti / 2.0 };
            r.inset(
                half(near(r.x, outline.x)),
                half(near(r.right(), outline.right())),
                half(near(r.y, outline.y)),
                half(near(r.top(), outline.top())),
            )
        })
        .collect();
    let mut spec = brief(20.0, 20.0, &[]);
    spec.rooms = floors
        .iter()
        .enumerate()
        .map(|(i, f)| Room {
            min_width: 1.0,
            max_aspect: 10.0,
            ..Room::new(format!("r{i}"), f.area())
        })
        .collect();
    let layout = Layout {
        footprint: Rect::new(
            outline.x - te / 2.0,
            outline.y - te / 2.0,
            outline.w + te,
            outline.h + te,
        ),
        outline,
        rooms: rects
            .iter()
            .zip(&floors)
            .enumerate()
            .map(|(i, (r, f))| PlacedRoom {
                name: format!("r{i}"),
                rect: *r,
                inner: *f,
            })
            .collect(),
        circulation: None,
        entrance: 0,
        doors: doors.to_vec(),
        energy: 0.0,
        seed: 0,
    };
    (spec, layout)
}

/// A T-junction: r0 the whole left half, r1 below and r2 above on the
/// right, r2 `top` tall.
fn tee(top: f64) -> (Spec, Layout) {
    let outline = Rect::new(O, O, 8.0, 4.0);
    by_hand(
        &[
            Rect::new(O, O, 4.0, 4.0),
            Rect::new(O + 4.0, O, 4.0, 4.0 - top),
            Rect::new(O + 4.0, O + 4.0 - top, 4.0, top),
        ],
        outline,
        &[(0, 1), (0, 2)],
    )
}

fn breaks(spec: &Spec, layout: &Layout, what: &str) -> bool {
    let broken = verify(spec, layout);
    assert!(broken.iter().any(|b| b.contains(what)), "{broken:?}");
    true
}

#[test]
fn a_plan_made_by_hand_passes_and_its_door_at_a_t_junction_is_measured() {
    // A door needs 0.9 + 2 × 0.15 + 0.25 = 1.45 m of shared wall.
    let (spec, layout) = tee(1.45);
    assert_eq!(verify(&spec, &layout), Vec::<String>::new());
    let (spec, layout) = tee(1.45 - 1e-6);
    assert!(breaks(&spec, &layout, "the door between the r0 and the r2"));
}

#[test]
fn a_door_through_a_corner_or_to_nowhere_is_caught() {
    // A 2 × 2 grid: r0 and r3 meet only at the middle point.
    let outline = Rect::new(O, O, 8.0, 8.0);
    let (spec, layout) = by_hand(
        &[
            Rect::new(O, O, 4.0, 4.0),
            Rect::new(O + 4.0, O, 4.0, 4.0),
            Rect::new(O, O + 4.0, 4.0, 4.0),
            Rect::new(O + 4.0, O + 4.0, 4.0, 4.0),
        ],
        outline,
        &[(0, 1), (0, 2), (0, 3)],
    );
    assert!(breaks(&spec, &layout, "the door between the r0 and the r3"));
    let mut layout = layout;
    layout.doors = vec![(0, 1), (0, 2), (1, 7)];
    assert!(breaks(&spec, &layout, "a door names a space"));
    assert!(breaks(&spec, &layout, "the r3 cannot be reached"));
}

#[test]
fn areas_are_held_to_five_percent_either_side() {
    let (mut spec, layout) = tee(2.0);
    let floor = layout.rooms[0].inner.area();
    spec.rooms[0].area = floor / 0.951;
    assert_eq!(verify(&spec, &layout), Vec::<String>::new());
    spec.rooms[0].area = floor / 0.949;
    assert!(breaks(&spec, &layout, "the r0's area"));
    spec.rooms[0].area = floor / 1.049;
    assert_eq!(verify(&spec, &layout), Vec::<String>::new());
    spec.rooms[0].area = floor / 1.051;
    assert!(breaks(&spec, &layout, "the r0's area"));
}

#[test]
fn a_floor_that_forgets_a_wall_is_caught() {
    // r0 is on the outline on three sides: half the exterior wall there.
    let (spec, mut layout) = tee(2.0);
    layout.rooms[0].inner = layout.rooms[0].rect.inset(0.075, 0.075, 0.075, 0.075);
    assert!(breaks(
        &spec,
        &layout,
        "the r0's floor does not match its walls"
    ));
    let (spec, mut layout) = tee(2.0);
    layout.rooms[1].inner = layout.rooms[1].rect;
    assert!(breaks(
        &spec,
        &layout,
        "the r1's floor does not match its walls"
    ));
}

#[test]
fn overlaps_gaps_and_setbacks_by_a_millimetre_are_caught() {
    let (spec, mut layout) = tee(2.0);
    layout.rooms[1].rect.w -= 0.001;
    assert!(breaks(&spec, &layout, "gap"));
    let (spec, mut layout) = tee(2.0);
    layout.rooms[1].rect.x -= 0.001;
    layout.rooms[1].rect.w += 0.001;
    assert!(breaks(&spec, &layout, "the r0 and the r1 overlap"));
    let (mut spec, layout) = tee(2.0);
    spec.setbacks.left = 0.001;
    assert!(breaks(&spec, &layout, "outside the buildable area"));
}

#[test]
fn widths_aspects_entrances_and_circulation_are_held_at_their_bounds() {
    let (mut spec, layout) = tee(2.0);
    let narrow = layout.rooms[2].inner.h;
    spec.rooms[2].min_width = narrow;
    assert_eq!(verify(&spec, &layout), Vec::<String>::new());
    spec.rooms[2].min_width = narrow + 0.001;
    assert!(breaks(&spec, &layout, "the r2 is narrower"));
    let (mut spec, layout) = tee(2.0);
    let floor = layout.rooms[2].inner;
    spec.rooms[2].min_length = floor.w.max(floor.h);
    assert_eq!(verify(&spec, &layout), Vec::<String>::new());
    spec.rooms[2].min_length += 0.001;
    assert!(breaks(&spec, &layout, "the r2 is shorter than"));

    let (mut spec, layout) = tee(1.2);
    let floor = layout.rooms[2].inner;
    spec.rooms[2].max_aspect = floor.aspect() - 0.001;
    assert!(breaks(
        &spec,
        &layout,
        "the r2 is longer than its aspect ratio"
    ));

    // The entrance's door needs one side on the outline 1.45 long: r2,
    // between r1 and r3 on the right, has only its right side, 1.2.
    let outline = Rect::new(O, O, 8.0, 4.8);
    let (mut spec, mut layout) = by_hand(
        &[
            Rect::new(O, O, 4.0, 4.8),
            Rect::new(O + 4.0, O, 4.0, 1.8),
            Rect::new(O + 4.0, O + 1.8, 4.0, 1.2),
            Rect::new(O + 4.0, O + 3.0, 4.0, 1.8),
        ],
        outline,
        &[(0, 1), (1, 2), (2, 3)],
    );
    assert_eq!(verify(&spec, &layout), Vec::<String>::new());
    layout.entrance = 2;
    spec.rooms[2].entrance = true;
    assert!(breaks(&spec, &layout, "the entrance has no outside wall"));

    // The last space as circulation: within its budget, or not at all.
    let (mut spec, mut layout) = tee(2.0);
    spec.rooms.pop();
    layout.circulation = Some(2);
    let total: f64 = spec.rooms.iter().map(|r| r.area).sum();
    spec.max_circulation = layout.rooms[2].inner.area() / total;
    assert_eq!(verify(&spec, &layout), Vec::<String>::new());
    spec.max_circulation *= 0.99;
    assert!(breaks(&spec, &layout, "more than the brief allows"));
    spec.max_circulation = 0.0;
    assert!(breaks(&spec, &layout, "more than the brief allows"));
}

// ── Walls and labels (#279) ──

/// Whatever is wrong with `layout`'s walls as drafting: faces that do not
/// close round their floor, a face not half a wall from its centreline,
/// faces of two rooms that cross or overlap, and — sampled every 7 cm —
/// a point that is both floor and wall, or neither: an overlap or a gap
/// where walls meet.
fn drafting_faults(spec: &Spec, layout: &Layout) -> Vec<String> {
    let (te, ti) = (spec.exterior_wall, spec.interior_wall);
    let w = walls(spec, layout);
    let mut faults = Vec::new();
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let faces: Vec<Face> = w.exterior.iter().chain(&w.interior).copied().collect();
    let all_doors = doors(spec, layout);
    let all_windows = crate::windows(spec, layout);
    for (index, room) in layout.rooms.iter().enumerate() {
        let own: Vec<Face> = faces.iter().filter(|f| f.room == index).copied().collect();
        // Closed but for its doors and windows: its faces and the openings in them make
        // up the floor's perimeter, every face on it.
        let inner = room.inner;
        let length = |f: &Face| f.start.distance_to(f.end);
        let openings: f64 = all_doors
            .iter()
            .filter(|d| d.rooms.0 == index || d.rooms.1 == Some(index))
            .map(|d| d.from.distance_to(d.to))
            .sum::<f64>()
            + all_windows
                .iter()
                .filter(|w| w.room == index)
                .map(|w| w.from.distance_to(w.to))
                .sum::<f64>();
        let perimeter = 2.0 * (inner.w + inner.h);
        if (own.iter().map(length).sum::<f64>() + openings - perimeter).abs() > 1e-9 {
            faults.push(format!("the {}'s faces and doors do not close", room.name));
        }
        let o = layout.outline;
        let r = room.rect;
        for f in &own {
            let side = if near(f.start.x, f.end.x) {
                if near(f.start.x, inner.x) {
                    Some(near(r.x, o.x))
                } else if near(f.start.x, inner.right()) {
                    Some(near(r.right(), o.right()))
                } else {
                    None
                }
            } else if near(f.start.y, inner.y) {
                Some(near(r.y, o.y))
            } else if near(f.start.y, inner.top()) {
                Some(near(r.top(), o.top()))
            } else {
                None
            };
            match side {
                None => faults.push(format!("a face of the {} is off its floor", room.name)),
                Some(outside) => {
                    if outside != w.exterior.contains(f) {
                        faults.push(format!("a face of the {} is on the wrong layer", room.name));
                    }
                }
            }
        }
        // Half a wall from the centreline: exterior or interior.
        for (face_at, line_at, on_outline) in [
            (inner.x, r.x, near(r.x, o.x)),
            (inner.right(), r.right(), near(r.right(), o.right())),
            (inner.y, r.y, near(r.y, o.y)),
            (inner.top(), r.top(), near(r.top(), o.top())),
        ] {
            let half = if on_outline { te / 2.0 } else { ti / 2.0 };
            if !near((face_at - line_at).abs(), half) {
                faults.push(format!(
                    "a face of the {} is not half its wall from it",
                    room.name
                ));
            }
        }
    }
    // The outer face is the outline's, half an exterior wall out.
    let f = layout.footprint;
    let o = layout.outline;
    if !(near(o.x - f.x, te / 2.0)
        && near(f.right() - o.right(), te / 2.0)
        && near(o.y - f.y, te / 2.0)
        && near(f.top() - o.top(), te / 2.0))
    {
        faults.push("the outer face is not half an exterior wall out".into());
    }
    // Faces of different rooms neither cross nor overlap.
    for (i, a) in faces.iter().enumerate() {
        for b in &faces[i + 1..] {
            if a.room == b.room {
                continue;
            }
            let (ax0, ax1) = (a.start.x.min(a.end.x), a.start.x.max(a.end.x));
            let (ay0, ay1) = (a.start.y.min(a.end.y), a.start.y.max(a.end.y));
            let (bx0, bx1) = (b.start.x.min(b.end.x), b.start.x.max(b.end.x));
            let (by0, by1) = (b.start.y.min(b.end.y), b.start.y.max(b.end.y));
            if ax0 <= bx1 + 1e-9 && bx0 <= ax1 + 1e-9 && ay0 <= by1 + 1e-9 && by0 <= ay1 + 1e-9 {
                faults.push(format!(
                    "a face of the {} touches one of the {}",
                    layout.rooms[a.room].name, layout.rooms[b.room].name
                ));
            }
        }
    }
    // Every point is floor or wall, never both, never neither.
    let wall_at = |x: f64, y: f64| {
        let d = (x - o.x)
            .abs()
            .min((x - o.right()).abs())
            .min((y - o.y).abs())
            .min((y - o.top()).abs());
        let exterior = d <= te / 2.0;
        let interior = layout.rooms.iter().any(|room| {
            let r = room.rect;
            let within = |a0: f64, a1: f64, v: f64| v >= a0 - ti / 2.0 && v <= a1 + ti / 2.0;
            let sides = [
                (
                    !near(r.x, o.x),
                    (x - r.x).abs() <= ti / 2.0 && within(r.y, r.top(), y),
                ),
                (
                    !near(r.right(), o.right()),
                    (x - r.right()).abs() <= ti / 2.0 && within(r.y, r.top(), y),
                ),
                (
                    !near(r.y, o.y),
                    (y - r.y).abs() <= ti / 2.0 && within(r.x, r.right(), x),
                ),
                (
                    !near(r.top(), o.top()),
                    (y - r.top()).abs() <= ti / 2.0 && within(r.x, r.right(), x),
                ),
            ];
            sides.iter().any(|(interior, here)| *interior && *here)
        });
        exterior || interior
    };
    let step = 0.07;
    let mut y = f.y + 0.0313;
    'sample: while y < f.top() {
        let mut x = f.x + 0.0217;
        while x < f.right() {
            let floors = layout
                .rooms
                .iter()
                .filter(|r| {
                    x > r.inner.x && x < r.inner.right() && y > r.inner.y && y < r.inner.top()
                })
                .count();
            let opening = all_doors.iter().any(|d| {
                let (x0, x1) = (d.from.x.min(d.to.x), d.from.x.max(d.to.x));
                let (y0, y1) = (d.from.y.min(d.to.y), d.from.y.max(d.to.y));
                let t = d.thickness / 2.0;
                if near(x0, x1) {
                    (x - x0).abs() < t && y > y0 && y < y1
                } else {
                    (y - y0).abs() < t && x > x0 && x < x1
                }
            });
            if opening {
                if floors > 0 {
                    faults.push(format!("at ({x:.3}, {y:.3}) a door's opening is floor"));
                    break 'sample;
                }
            } else if floors > 1 || (floors == 1) == wall_at(x, y) {
                faults.push(format!(
                    "at ({x:.3}, {y:.3}) floor {floors}, wall {}",
                    wall_at(x, y)
                ));
                break 'sample;
            }
            x += step;
        }
        y += step;
    }
    faults
}

#[test]
fn walls_are_their_thickness_and_meet_cleanly_at_l_t_and_x() {
    // L at every outer corner, T where r1 and r2 meet r0, X in the grid.
    let (spec, layout) = tee(1.6);
    assert_eq!(drafting_faults(&spec, &layout), Vec::<String>::new());
    let outline = Rect::new(O, O, 8.0, 8.0);
    let (spec, layout) = by_hand(
        &[
            Rect::new(O, O, 4.0, 4.0),
            Rect::new(O + 4.0, O, 4.0, 4.0),
            Rect::new(O, O + 4.0, 4.0, 4.0),
            Rect::new(O + 4.0, O + 4.0, 4.0, 4.0),
        ],
        outline,
        &[(0, 1), (0, 2), (1, 3)],
    );
    assert_eq!(drafting_faults(&spec, &layout), Vec::<String>::new());
    let w = walls(&spec, &layout);
    // Each corner room has two sides on the outside and two inside; the
    // doors cut three interior walls in two faces each and the way in one.
    assert_eq!((w.exterior.len(), w.interior.len()), (9, 14));
    // A floor that forgets its wall is caught here too.
    let mut broken = layout.clone();
    broken.rooms[0].inner = broken.rooms[0].rect;
    assert!(!drafting_faults(&spec, &broken).is_empty());
}

/// Whatever is wrong with `layout`'s doors: a room no door reaches, an
/// opening off its wall's centreline, too near a junction, or the wrong
/// width, a face left across an opening, a missing jamb, a leaf or swing
/// that is not a door's, or an outer face not open at the way in.
fn door_faults(spec: &Spec, layout: &Layout) -> Vec<String> {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let all = doors(spec, layout);
    let w = walls(spec, layout);
    let mut faults = Vec::new();
    let ways_in: Vec<&Door> = all.iter().filter(|d| d.rooms.1.is_none()).collect();
    if ways_in.len() != 1 || ways_in[0].rooms.0 != layout.entrance {
        faults.push(format!("{} ways in", ways_in.len()));
    }
    // Every room reached through the doors.
    let mut reached = vec![false; layout.rooms.len()];
    reached[layout.entrance] = true;
    for _ in 0..layout.rooms.len() {
        for d in &all {
            if let (a, Some(b)) = d.rooms
                && reached[a] != reached[b]
            {
                reached[a] = true;
                reached[b] = true;
            }
        }
    }
    for (i, r) in reached.iter().enumerate() {
        if !r {
            faults.push(format!("no door reaches the {}", layout.rooms[i].name));
        }
    }
    let o = layout.outline;
    let keep_clear = DOOR_CLEARANCE + spec.exterior_wall / 2.0;
    for d in &all {
        let vertical = near(d.from.x, d.to.x);
        let along = |p: Point2| if vertical { p.y } else { p.x };
        let across = |p: Point2| if vertical { p.x } else { p.y };
        let (lo, hi) = (along(d.from), along(d.to));
        let line = across(d.from);
        if !near(hi - lo, spec.door) {
            faults.push(format!("an opening {:.3} wide", hi - lo));
        }
        // The stretch of wall it may be in: shared by its rooms, or the
        // way in's side on the outline.
        let a = layout.rooms[d.rooms.0].rect;
        let stretch = match d.rooms.1 {
            Some(b) => {
                let b = layout.rooms[b].rect;
                if vertical {
                    (near(a.right(), line) || near(a.x, line))
                        && (near(b.right(), line) || near(b.x, line))
                } else {
                    (near(a.top(), line) || near(a.y, line))
                        && (near(b.top(), line) || near(b.y, line))
                }
                .then(|| {
                    if vertical {
                        (a.y.max(b.y), a.top().min(b.top()))
                    } else {
                        (a.x.max(b.x), a.right().min(b.right()))
                    }
                })
            }
            None => {
                let on = if vertical {
                    near(line, o.x) || near(line, o.right())
                } else {
                    near(line, o.y) || near(line, o.top())
                };
                let mine = if vertical {
                    near(a.x, line) || near(a.right(), line)
                } else {
                    near(a.y, line) || near(a.top(), line)
                };
                (on && mine).then(|| {
                    if vertical {
                        (a.y, a.top())
                    } else {
                        (a.x, a.right())
                    }
                })
            }
        };
        match stretch {
            None => faults.push("an opening off its wall".into()),
            Some((s0, s1)) => {
                if lo < s0 + keep_clear - 1e-9 || hi > s1 - keep_clear + 1e-9 {
                    faults.push("an opening too near a junction".into());
                }
            }
        }
        // No face across the opening, on either face of the wall.
        for f in w.exterior.iter().chain(&w.interior) {
            if near(f.start.x, f.end.x) == vertical
                && (across(f.start) - line).abs() <= d.thickness / 2.0 + 1e-9
            {
                let (f0, f1) = (
                    along(f.start).min(along(f.end)),
                    along(f.start).max(along(f.end)),
                );
                if f0 < hi - 1e-9 && f1 > lo + 1e-9 {
                    faults.push(format!(
                        "a face of the {} crosses a door",
                        layout.rooms[f.room].name
                    ));
                }
            }
        }
        // A jamb across the wall at each end.
        let jambs = if d.rooms.1.is_none() {
            &w.exterior_jambs
        } else {
            &w.interior_jambs
        };
        for end in [lo, hi] {
            let found = jambs.iter().any(|(p, q)| {
                near(along(*p), end) && near(along(*q), end) && near(p.distance_to(*q), d.thickness)
            });
            if !found {
                faults.push("a door without its jamb".into());
            }
        }
        // The leaf a door's width long, into its room; a quarter swing.
        if !near(d.hinge.distance_to(d.leaf), spec.door) {
            faults.push("a leaf not a door's width".into());
        }
        let room = layout.rooms[d.into].inner;
        if room.w.min(room.h) >= spec.door
            && !(d.leaf.x >= room.x - 1e-9
                && d.leaf.x <= room.right() + 1e-9
                && d.leaf.y >= room.y - 1e-9
                && d.leaf.y <= room.top() + 1e-9)
        {
            faults.push(format!("a leaf outside the {}", layout.rooms[d.into].name));
        }
        if !near(
            (d.swing.1 - d.swing.0).rem_euclid(std::f64::consts::TAU),
            std::f64::consts::FRAC_PI_2,
        ) {
            faults.push("a swing that is not a quarter turn".into());
        }
    }
    // The outer face open at the way in and each window, between their
    // jambs: every piece ends where an exterior jamb does.
    if let Some(way_in) = ways_in.first() {
        let jamb_ends: Vec<Point2> = w
            .exterior_jambs
            .iter()
            .flat_map(|(p, q)| [*p, *q])
            .collect();
        let ends = w
            .outside
            .iter()
            .flat_map(|piece| [piece[0], piece[piece.len() - 1]]);
        if w.outside_closed
            || !ends
                .clone()
                .all(|e| jamb_ends.iter().any(|j| j.distance_to(e) < 1e-9))
        {
            faults.push("the outer face is not open at the way in".into());
        }
        let _ = way_in;
    }
    faults
}

#[test]
fn doors_reach_every_room_through_their_walls_clear_of_junctions() {
    let (spec, layout) = tee(1.6);
    assert_eq!(door_faults(&spec, &layout), Vec::<String>::new());
    let outline = Rect::new(O, O, 8.0, 8.0);
    let (spec, layout) = by_hand(
        &[
            Rect::new(O, O, 4.0, 4.0),
            Rect::new(O + 4.0, O, 4.0, 4.0),
            Rect::new(O, O + 4.0, 4.0, 4.0),
            Rect::new(O + 4.0, O + 4.0, 4.0, 4.0),
        ],
        outline,
        &[(0, 1), (0, 2), (1, 3)],
    );
    assert_eq!(door_faults(&spec, &layout), Vec::<String>::new());
    let all = doors(&spec, &layout);
    assert_eq!(all.len(), 4, "three walls and the way in");
    // The way in is on the entrance's front, at its middle.
    let way_in = all.iter().find(|d| d.rooms.1.is_none()).unwrap();
    assert!((way_in.from.y - O).abs() < 1e-12);
    assert!(((way_in.from.x + way_in.to.x) / 2.0 - (O + 2.0)).abs() < 1e-12);
    // r3 is two doors in: its door opens into it, not into r1.
    let to_r3 = all.iter().find(|d| d.rooms == (1, Some(3))).unwrap();
    assert_eq!(to_r3.into, 3);
}

#[test]
fn every_room_is_labelled_with_its_name_and_area_at_its_middle() {
    let (_, layout) = tee(1.6);
    let labels = labels(&layout);
    assert_eq!(labels.len(), 3);
    for (label, room) in labels.iter().zip(&layout.rooms) {
        assert_eq!(label.name, room.name);
        assert!((label.area - room.inner.area()).abs() < 1e-12);
        assert!(label.at.x > room.inner.x && label.at.x < room.inner.right());
        assert!(label.at.y > room.inner.y && label.at.y < room.inner.top());
    }
}

proptest::proptest! {
    #![proptest_config(cases(16))]

    /// The walls of any plan the search makes are drafted cleanly.
    #[test]
    fn the_walls_of_every_plan_are_drafted_cleanly(
        width in 8.0..25.0f64,
        depth in 8.0..25.0f64,
        areas in proptest::collection::vec(4.0..25.0f64, 1..7),
        max_circulation in prop_oneof_circulation(),
        seed in 0..1000u64,
    ) {
        let rooms: Vec<(String, f64)> =
            areas.iter().enumerate().map(|(i, a)| (format!("room {i}"), *a)).collect();
        let named: Vec<(&str, f64)> = rooms.iter().map(|(n, a)| (n.as_str(), *a)).collect();
        let mut spec = brief(width, depth, &named);
        spec.max_circulation = max_circulation;
        spec.seed = seed;
        if let Ok(plans) = alternatives_within(&spec, 1, 4) {
            proptest::prop_assert_eq!(drafting_faults(&spec, &plans[0]), Vec::<String>::new());
            proptest::prop_assert_eq!(door_faults(&spec, &plans[0]), Vec::<String>::new());
            proptest::prop_assert_eq!(labels(&plans[0]).len(), plans[0].rooms.len());
        }
    }
}

/// #324: every window is on the outline, along its room's floor, clear of
/// the room's corners and of the way in, no wider than asked and no
/// narrower than the least; and a room asking for an outside wall gets
/// one when no width is given.
#[test]
fn windows_are_on_outside_walls_clear_of_corners_and_the_way_in() {
    let briefs = [
        brief(
            12.0,
            20.0,
            &[
                ("living", 25.0),
                ("kitchen", 10.0),
                ("bed 1", 12.0),
                ("bed 2", 10.0),
                ("bath", 5.0),
            ],
        ),
        brief(
            10.0,
            25.0,
            &[
                ("living", 30.0),
                ("kitchen", 12.0),
                ("suite", 16.0),
                ("bath", 5.0),
                ("bed", 11.0),
                ("office", 8.0),
            ],
        ),
        brief(8.0, 12.0, &[("studio", 25.0), ("bath", 5.0)]),
    ];
    for mut spec in briefs {
        spec.windows = Some(1.5);
        let layout = crate::solve(&spec).unwrap();
        let tolerance = cad_core::tolerance::LinearTolerance::DEFAULT.at_magnitude(30.0);
        let doors = crate::doors(&spec, &layout);
        let way_in = doors.iter().find(|d| d.rooms.1.is_none()).unwrap();
        let windows = crate::windows(&spec, &layout);
        assert!(!windows.is_empty());
        for w in &windows {
            let width = w.from.distance_to(w.to);
            assert!(
                (crate::WINDOW_MIN - 1e-9..=1.5 + 1e-9).contains(&width),
                "{width}"
            );
            let o = layout.outline;
            let on_outline = |p: Point2| {
                tolerance.eq(p.x, o.x)
                    || tolerance.eq(p.x, o.right())
                    || tolerance.eq(p.y, o.y)
                    || tolerance.eq(p.y, o.top())
            };
            assert!(on_outline(w.from) && on_outline(w.to), "on an outside wall");
            let floor = layout.rooms[w.room].inner;
            for p in [w.from, w.to] {
                let along_ok = if tolerance.eq(w.from.x, w.to.x) {
                    p.y >= floor.y + crate::WINDOW_CLEARANCE - 1e-9
                        && p.y <= floor.top() - crate::WINDOW_CLEARANCE + 1e-9
                } else {
                    p.x >= floor.x + crate::WINDOW_CLEARANCE - 1e-9
                        && p.x <= floor.right() - crate::WINDOW_CLEARANCE + 1e-9
                };
                assert!(along_ok, "clear of {}'s corners", layout.rooms[w.room].name);
            }
            // Clear of the way in, where it is on the same wall.
            let same_line = (tolerance.eq(w.from.x, w.to.x)
                && tolerance.eq(way_in.from.x, way_in.to.x)
                && tolerance.eq(w.from.x, way_in.from.x))
                || (tolerance.eq(w.from.y, w.to.y)
                    && tolerance.eq(way_in.from.y, way_in.to.y)
                    && tolerance.eq(w.from.y, way_in.from.y));
            if same_line {
                let span = |a: Point2, b: Point2| {
                    if tolerance.eq(a.x, b.x) {
                        (a.y.min(b.y), a.y.max(b.y))
                    } else {
                        (a.x.min(b.x), a.x.max(b.x))
                    }
                };
                let (wl, wh) = span(w.from, w.to);
                let (dl, dh) = span(way_in.from, way_in.to);
                assert!(
                    wh <= dl - crate::WINDOW_CLEARANCE + 1e-9
                        || wl >= dh + crate::WINDOW_CLEARANCE - 1e-9,
                    "clear of the way in"
                );
            }
        }
        // Every room with an outside wall long enough has a window.
        for (i, room) in layout.rooms.iter().enumerate() {
            let longest = room.rect.longest_outside(&layout.outline, tolerance);
            let floor = room.inner;
            if longest > 0.0
                && floor.w.min(floor.h)
                    >= crate::WINDOW_MIN + 2.0 * crate::WINDOW_CLEARANCE + spec.door + 1.0
            {
                assert!(
                    windows.iter().any(|w| w.room == i),
                    "{} has none",
                    room.name
                );
            }
        }
        // The walls still check out with the openings cut.
        let mut faults = drafting_faults(&spec, &layout);
        faults.extend(door_faults(&spec, &layout));
        assert!(faults.is_empty(), "{faults:?}");
    }
    // No width given: only the rooms asking for an outside wall, one each.
    let mut spec = brief(
        12.0,
        20.0,
        &[("living", 25.0), ("kitchen", 10.0), ("bed 1", 12.0)],
    );
    spec.rooms[2].exterior = true;
    let layout = crate::solve(&spec).unwrap();
    let windows = crate::windows(&spec, &layout);
    assert!(windows.iter().all(|w| w.room == 2) && windows.len() <= 1);
}
