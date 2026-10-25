use super::*;

fn brief(width: f64, depth: f64, rooms: &[(&str, f64)]) -> Spec {
    Spec {
        terrain: Terrain { width, depth },
        setbacks: Setbacks::default(),
        rooms: rooms.iter().map(|(n, a)| Room::new(*n, *a)).collect(),
        adjacent: Vec::new(),
        exterior_wall: 0.25,
        interior_wall: 0.15,
        door: 0.9,
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
fn what_cannot_be_built_is_refused_with_the_reason() {
    let spec = brief(8.0, 8.0, &[("living", 40.0), ("bedroom", 30.0)]);
    let error = solve(&spec).unwrap_err();
    assert!(
        error.reasons[0].starts_with("The rooms need 70 m²"),
        "{error}"
    );

    let mut spec = brief(20.0, 20.0, &[("bath", 2.0)]);
    spec.rooms[0].min_width = 1.6;
    assert_eq!(
        solve(&spec).unwrap_err().reasons,
        ["The bath's 2 m² cannot be 1.6 m wide both ways."]
    );
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
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(24))]

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
        match solve(&spec) {
            Ok(plan) => {
                proptest::prop_assert_eq!(verify(&spec, &plan), Vec::<String>::new());
                proptest::prop_assert_eq!(solve(&spec).unwrap(), plan);
            }
            Err(refused) => proptest::prop_assert!(!refused.reasons.is_empty()),
        }
    }
}
