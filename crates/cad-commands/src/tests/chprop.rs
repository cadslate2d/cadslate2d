//! CHPROP and the selection summary (#141) through the session.

use cad_core::Point2;
use cad_document::{
    Color, Entity, LayerId, LineEntity, Linetype, LinetypeId, Lineweight, Properties,
};

use crate::testing::Harness;

const OPTIONS: &str = "Enter property to change or [Color/LAyer/LType/LWeight]:";

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(y: f64) -> Entity {
    Entity::Line(LineEntity::new(p(0.0, y), p(100.0, y)))
}

/// Three lines at y = 0, 10, 20 and the layers `walls` and `locked`.
fn harness() -> Harness {
    let mut h = Harness::with_entities([line(0.0), line(10.0), line(20.0)]);
    h.enter_all(&[
        "LA",
        "N",
        "walls,locked",
        "L",
        "locked",
        "",
        "LT",
        "L",
        "Dashed",
        "",
    ]);
    h
}

#[test]
fn chprop_changes_every_selected_object_in_one_step_and_loops_until_enter() {
    let mut h = harness();
    h.select_box(p(-10.0, -5.0), p(110.0, 15.0));
    assert_eq!(h.session.selection().len(), 2);
    h.enter("CHPROP");
    assert_eq!(last(&h), "2 found");
    assert_eq!(h.prompt(), OPTIONS);
    h.enter_all(&["LA", "walls"]);
    assert_eq!(last(&h), "2 objects changed to layer \"walls\".");
    h.enter_all(&["C", "red"]);
    assert_eq!(last(&h), "2 objects changed to red.");
    h.enter_all(&["LT", "dashed"]);
    assert_eq!(last(&h), "2 objects changed to linetype Dashed.");
    h.enter_all(&["LW", "0.5"]);
    assert_eq!(last(&h), "2 objects changed to lineweight 0.50 mm.");
    assert_eq!(h.prompt(), OPTIONS);
    h.enter("");
    assert_eq!(h.prompt(), "Command:");

    let changed = Properties {
        layer: LayerId::from_raw(1),
        color: Color::Index(1),
        linetype: Linetype::Named(LinetypeId::from_raw(1)),
        lineweight: Lineweight::Hundredths(50),
    };
    assert_eq!(
        h.properties(),
        vec![changed, changed, Properties::default()]
    );

    // Four transactions, one command, one undo.
    h.undo();
    assert!(h.properties().iter().all(|p| *p == Properties::default()));
    h.redo();
    assert_eq!(h.properties()[0], changed);
}

#[test]
fn chprop_asks_for_objects_when_nothing_is_selected_and_refuses_bad_values() {
    let mut h = harness();
    h.enter("CH");
    assert_eq!(h.prompt(), "Select objects:");
    h.click_world(p(50.0, 20.0));
    h.enter("");
    assert_eq!(last(&h), "1 found");
    // A value that is not one is explained and asked for again; Enter
    // goes back to the options. Nothing changes.
    h.enter_all(&["C", "mauve"]);
    assert!(last(&h).starts_with("A colour is"));
    h.enter_all(&["", "LA", "roof"]);
    assert_eq!(last(&h), "There is no layer \"roof\".");
    h.enter_all(&["", "LT", "Hidden"]);
    assert_eq!(
        last(&h),
        "Linetype \"Hidden\" is not loaded: load it with LINETYPE first."
    );
    h.enter_all(&["", "LW", "0.26"]);
    assert!(last(&h).ends_with("Or Default, ByLayer, or ByBlock."));
    h.enter("");
    assert_eq!(h.prompt(), OPTIONS);
    h.enter_all(&["C", "ByBlock"]);
    assert_eq!(last(&h), "1 object changed to ByBlock.");
    h.enter("");
    assert_eq!(h.properties()[2].color, Color::ByBlock);
    assert_eq!(h.properties()[0], Properties::default());

    h.escape();
    h.enter_all(&["PROPERTIES", ""]);
    assert_eq!(last(&h), "No objects selected.");
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn objects_on_a_locked_layer_are_never_changed() {
    let mut h = harness();
    // Move the middle line to the locked layer first.
    h.click_world(p(50.0, 10.0));
    h.enter_all(&["CH", "LA", "locked", ""]);
    assert_eq!(h.properties()[1].layer, LayerId::from_raw(2));
    h.escape();
    // A box over all three selects the two that are not locked.
    h.select_box(p(-10.0, -5.0), p(110.0, 25.0));
    assert_eq!(h.session.selection().len(), 2);
    h.enter_all(&["CH", "C", "3", ""]);
    let colours: Vec<Color> = h.properties().iter().map(|p| p.color).collect();
    assert_eq!(
        colours,
        vec![Color::Index(3), Color::ByLayer, Color::Index(3)]
    );
}

#[test]
fn the_ui_state_summarises_the_selection_and_is_bounded() {
    // Twenty thousand objects: enough that anything quadratic would show,
    // and the summary is still four counts and four strings.
    let mut h = Harness::with_entities(
        (0..20_000)
            .map(|i| Entity::Line(LineEntity::new(p(f64::from(i), 0.0), p(f64::from(i), 10.0)))),
    );
    h.select_box(p(-10.0, -5.0), p(20_010.0, 15.0));
    let summary = h.session.ui_state().selection;
    assert_eq!(summary.count, 20_000);
    assert_eq!(summary.kinds, vec![("lines".to_owned(), 20_000)]);
    assert_eq!(summary.layer.as_deref(), Some("0"));
    assert!(summary.geometry.is_empty());

    // Pointer movement does not recompute it: the same answer comes back
    // from the cache, which the generation and revision keep honest.
    h.move_to(10.0, 10.0);
    assert_eq!(h.session.ui_state().selection, summary);

    // Changing them all is one linear transaction, not a scan per object.
    h.enter_all(&["CH", "C", "red", ""]);
    assert_eq!(h.session.ui_state().selection.color.as_deref(), Some("red"));
    h.escape();
    assert_eq!(h.session.ui_state().selection.count, 0);
    assert_eq!(h.session.ui_state().selection.headline(), "No selection");
}

#[test]
fn one_selected_object_shows_its_geometry_in_the_ui_state() {
    let mut h = Harness::with_entities([line(0.0)]);
    h.click_world(p(50.0, 0.0));
    let summary = h.session.ui_state().selection;
    assert_eq!(summary.headline(), "1 line");
    assert_eq!(
        summary.geometry[2],
        ("Length".to_owned(), "100.0000".to_owned())
    );
    assert_eq!(h.session.ui_state().linetypes, vec!["Continuous"]);
}
