use super::*;

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
    for (index, room) in layout.rooms.iter().enumerate() {
        let mut own: Vec<Face> = faces.iter().filter(|f| f.room == index).copied().collect();
        own.sort_by(|a, b| {
            (a.start.x, a.start.y)
                .partial_cmp(&(b.start.x, b.start.y))
                .unwrap()
        });
        if own.len() != 4 {
            faults.push(format!("the {} has {} faces", room.name, own.len()));
            continue;
        }
        // Closed: every end is another face's start.
        for f in &own {
            if !own
                .iter()
                .any(|g| near(g.start.x, f.end.x) && near(g.start.y, f.end.y))
            {
                faults.push(format!("the {}'s faces do not close", room.name));
            }
        }
        // Half a wall from the centreline: exterior or interior.
        let r = room.rect;
        let inner = room.inner;
        let o = layout.outline;
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
        let on_ext = w.exterior.iter().filter(|f| f.room == index).count();
        let expected = [
            near(r.x, o.x),
            near(r.right(), o.right()),
            near(r.y, o.y),
            near(r.top(), o.top()),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if on_ext != expected {
            faults.push(format!("the {}'s faces are on the wrong layer", room.name));
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
            if floors > 1 || (floors == 1) == wall_at(x, y) {
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
    // Each corner room has two faces on the outside, two inside.
    assert_eq!((w.exterior.len(), w.interior.len()), (8, 8));
    // A floor that forgets its wall is caught here too.
    let mut broken = layout.clone();
    broken.rooms[0].inner = broken.rooms[0].rect;
    assert!(!drafting_faults(&spec, &broken).is_empty());
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
            proptest::prop_assert_eq!(labels(&plans[0]).len(), plans[0].rooms.len());
        }
    }
}
