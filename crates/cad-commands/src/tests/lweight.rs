//! LWEIGHT and the LWT toggle (#136) through the session. Reading a
//! lineweight as typed is tested in `cad-document/src/lineweight.rs`; how a
//! weight reaches the screen, in `cad-render` and the browser tests.

use cad_core::Point2;
use cad_document::{Entity, LineEntity, Lineweight};

use crate::testing::Harness;
use crate::{Dirty, KeyAction, Toggle};

const PROMPT: &str = "Enter lineweight for new objects (mm, Default, ByLayer, or ByBlock)";
const HINT: &str = "Lineweights are not being shown: switch LWT on to see them.";

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn weights(h: &Harness) -> Vec<Lineweight> {
    h.properties().into_iter().map(|p| p.lineweight).collect()
}

#[test]
fn new_objects_take_the_current_lineweight_and_old_ones_keep_theirs() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "10,0", ""]);
    h.enter("LWEIGHT");
    assert_eq!(h.prompt(), format!("{PROMPT} <ByLayer>:"));
    h.enter("0.5");
    let log = h.log();
    assert_eq!(
        &log[log.len() - 2..],
        ["New objects have a lineweight of 0.50 mm.", HINT]
    );
    h.enter_all(&["L", "0,5", "10,5", "", "C", "50,50", "5"]);
    let heavy = Lineweight::Hundredths(50);
    assert_eq!(weights(&h), vec![Lineweight::ByLayer, heavy, heavy]);

    h.enter("LW");
    assert_eq!(h.prompt(), format!("{PROMPT} <0.50 mm>:"));
    h.enter("default");
    assert_eq!(
        h.session.drawing().current_lineweight(),
        Lineweight::Default
    );
    h.enter_all(&["LINEWEIGHT", "ByLayer"]);
    assert_eq!(last(&h), "New objects take their layer's lineweight.");

    // An offset of a heavy line is heavy, whatever is current.
    h.enter_all(&["O", "2"]);
    h.click_world(Point2::new(5.0, 5.0));
    h.click_world(Point2::new(5.0, 20.0));
    h.enter("");
    assert_eq!(weights(&h).last(), Some(&heavy));
}

#[test]
fn a_width_that_is_not_standard_is_explained_and_asked_for_again() {
    let mut h = Harness::new();
    h.enter("LW");
    for bad in ["0.26", "3", "thick", "-0.25"] {
        h.enter(bad);
        assert!(
            last(&h).starts_with("A lineweight is one of the standard widths"),
            "{bad}"
        );
        assert!(last(&h).ends_with("Or Default, ByLayer, or ByBlock."));
        assert_eq!(h.prompt(), format!("{PROMPT} <ByLayer>:"));
    }
    h.enter("2.11mm");
    assert_eq!(
        h.session.drawing().current_lineweight(),
        Lineweight::Hundredths(211)
    );
    h.undo();
    assert_eq!(
        h.session.drawing().current_lineweight(),
        Lineweight::ByLayer
    );
}

/// LWT is a session toggle: it changes what is shown and nothing in the
/// drawing, so it is not undone and makes no scene.
#[test]
fn the_lwt_toggle_belongs_to_the_session_not_the_drawing() {
    let mut h = Harness::new();
    h.enter_all(&["LW", "1", "L", "0,0", "10,0", ""]);
    let revision = h.session.drawing().revision();
    assert!(!h.session.toggles().is_on(Toggle::Lineweight));

    let dirty = h.session.key(KeyAction::Toggle(Toggle::Lineweight));
    assert!(h.session.toggles().is_on(Toggle::Lineweight));
    assert!(dirty.contains(Dirty::UI), "the status bar shows it");
    assert!(!dirty.contains(Dirty::SCENE), "nothing committed changed");
    assert_eq!(h.session.drawing().revision(), revision);
    assert_ne!(h.session.ui_state().toggles & Toggle::Lineweight.bit(), 0);

    // With lineweights showing, LWEIGHT has nothing to add.
    h.enter_all(&["LW", "0.25"]);
    assert_eq!(last(&h), "New objects have a lineweight of 0.25 mm.");

    h.undo();
    h.undo();
    assert!(
        h.session.toggles().is_on(Toggle::Lineweight),
        "undo leaves the toggle alone"
    );
}

/// The issue's rule: pick and snap apertures do not change with lineweight.
/// A line 2.11 mm heavy is drawn eight pixels wide; it is still picked and
/// snapped to by its centre line, at the usual reach.
#[test]
fn a_heavy_line_is_picked_and_snapped_to_like_a_thin_one() {
    let picked_at = |weight: &str, offset: f64| {
        let mut h = Harness::new();
        h.session.key(KeyAction::Toggle(Toggle::Lineweight));
        h.enter_all(&["LW", weight, "L", "0,0", "100,0", ""]);
        h.click_world(Point2::new(30.0, offset));
        let selected = h.session.selection().len();
        h.escape();
        h.escape();
        h.enter("L");
        h.move_to_world(Point2::new(100.0 + offset, offset));
        (selected, h.session.snap().is_some())
    };
    for offset in [2.0, 4.0, 6.0, 9.0, 12.0] {
        assert_eq!(
            picked_at("2.11", offset),
            picked_at("0", offset),
            "{offset} px from the line"
        );
    }
    let line: Entity = LineEntity::new(Point2::new(0.0, 0.0), Point2::new(100.0, 0.0)).into();
    let mut h = Harness::new();
    h.enter_all(&["LW", "2.11", "L", "0,0", "100,0", ""]);
    assert_eq!(h.entities(), vec![line], "a lineweight changes no geometry");
}
