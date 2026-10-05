//! What a layer being off, locked, or current means for drafting (#132).
//! The table itself — its changes, refusals, and history — is tested in
//! `cad-document/src/drawing.rs`.
//!
//! - **Off:** the layer's objects are not there for the pointer: not
//!   picked, not selected, not snapped to, not used as edges.
//! - **Locked:** they are there to see, to snap to, and to cut against, but
//!   no command changes them, and each says why.
//! - **Current:** new objects are put on it.

use cad_core::Point2;
use cad_document::{CircleEntity, Drawing, Entity, Layer, LayerId, LineEntity, Properties};
use cad_input::ExpectedInput;

use crate::testing::Harness;
use crate::{
    Command, CommandContext, CommandInput, CommandRegistry, Prompt, Session, Step, Toggle, Toggles,
};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

const HIDDEN: LayerId = LayerId::from_raw(1);
const LOCKED: LayerId = LayerId::from_raw(2);

fn layers() -> [Layer; 2] {
    [
        Layer {
            on: false,
            ..Layer::named("hidden")
        },
        Layer {
            locked: true,
            ..Layer::named("locked")
        },
    ]
}

/// A horizontal line on layer 0 from (0, 0) to (100, 0), a vertical line
/// crossing it at x = 40 on `layer`, and a second horizontal one above.
fn crossed_by(layer: LayerId) -> Harness {
    let drawing = Drawing::from_parts(
        layers(),
        [
            (line(0.0, 0.0, 100.0, 0.0), Properties::default()),
            (line(40.0, -50.0, 40.0, 50.0), Properties::on(layer)),
            (line(0.0, 30.0, 30.0, 30.0), Properties::default()),
        ],
    )
    .unwrap();
    Harness::with_drawing(drawing)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

// ── off ──

#[test]
fn an_object_on_a_layer_that_is_off_cannot_be_picked_or_boxed() {
    let mut h = crossed_by(HIDDEN);
    let ids = h.ids();
    h.click_world(p(40.0, 25.0)); // right on the hidden line
    h.escape(); // the click only opened a selection box
    assert!(h.session.selection().is_empty());

    h.select_box(p(200.0, 200.0), p(-200.0, -200.0)); // a crossing box over everything
    let selected: Vec<_> = h.session.selection().iter().collect();
    assert_eq!(selected, vec![ids[0], ids[2]]);
}

#[test]
fn an_object_on_a_layer_that_is_off_is_not_snapped_to_or_cut_against() {
    let mut h = crossed_by(HIDDEN);
    h.enter("L");
    h.move_to_world(p(40.0, 49.0)); // by the hidden line's end
    assert!(h.session.snap().is_none());
    // By where the two lines cross — and out of reach of the midpoint of
    // the visible one, at (50, 0).
    h.move_to_world(p(38.0, 1.0));
    assert!(
        h.session.snap().is_none(),
        "no intersection with what is not there"
    );
    h.move_to_world(p(99.0, 1.0));
    assert_eq!(h.session.snap().map(|snap| snap.point), Some(p(100.0, 0.0)));
    h.escape();

    // TRIM with every object as an edge: the hidden line is not one.
    let before = h.entities();
    h.enter_all(&["TR", ""]);
    h.click_world(p(80.0, 0.0));
    assert_eq!(last(&h), "The object does not cross a cutting edge.");
    h.enter("");
    assert_eq!(h.entities(), before);

    // Nor does a stretch window take hold of it.
    h.enter_all(&["S", "30,40", "50,60"]);
    assert_eq!(last(&h), "Nothing in the window to stretch.");
}

// ── locked ──

#[test]
fn an_object_on_a_locked_layer_is_not_selected_and_the_click_says_so() {
    let mut h = crossed_by(LOCKED);
    let ids = h.ids();
    h.click_world(p(40.0, 25.0));
    assert!(h.session.selection().is_empty());
    assert_eq!(last(&h), "1 was on a locked layer.");

    h.select_box(p(200.0, 200.0), p(-200.0, -200.0));
    let selected: Vec<_> = h.session.selection().iter().collect();
    assert_eq!(selected, vec![ids[0], ids[2]]);
    assert_eq!(last(&h), "1 was on a locked layer.");
    assert!(
        h.session
            .grips()
            .iter()
            .all(|grip| grip.id.entity != ids[1])
    );
}

/// Every command that acts on a selection: the locked object is not in it.
#[test]
fn commands_that_take_a_selection_leave_a_locked_object_alone() {
    for command in [
        &["M", "0,0", "5,5"][..],
        &["CO", "0,0", "5,5", ""],
        &["RO", "0,0", "90"],
        &["SC", "0,0", "2"],
        &["MI", "0,0", "0,10", "Y"],
        &["E"],
        &["J"],
    ] {
        let mut h = crossed_by(LOCKED);
        let locked = h.entities()[1].clone();
        let locked_id = h.ids()[1];
        // Verb first: the command asks, and a box is drawn over everything.
        h.enter(command[0]);
        h.select_box(p(200.0, 200.0), p(-200.0, -200.0));
        let log = h.log();
        assert!(
            log.contains(&"1 was on a locked layer.".to_owned()),
            "{command:?}: {log:?}"
        );
        assert!(
            log.contains(&"2 found, 2 total".to_owned()),
            "{command:?}: {log:?}"
        );
        h.enter("");
        h.enter_all(&command[1..]);
        assert_eq!(h.prompt(), "Command:", "{command:?}");
        assert_eq!(
            h.session.drawing().entity(locked_id),
            Some(&locked),
            "{command:?} left it as it was"
        );
        assert_eq!(
            h.session.drawing().properties(locked_id),
            Some(Properties::on(LOCKED))
        );
    }
}

/// Every command that picks the object it acts on refuses, naming the layer.
#[test]
fn commands_that_pick_an_object_refuse_a_locked_one_by_name() {
    let refusal = "That object is on the locked layer \"locked\".";
    for start in [
        &["O", "5"][..],
        &["TR", ""],
        &["EX", ""],
        &["F"],
        &["CHA"],
        &["BR"],
    ] {
        let mut h = crossed_by(LOCKED);
        let before = h.entities();
        let revision = h.session.drawing().revision();
        h.enter_all(start);
        let prompt = h.prompt();
        h.move_to_world(p(40.0, 25.0));
        assert!(
            h.session.hovered_object().is_none(),
            "{start:?}: nothing is offered"
        );
        h.click_world(p(40.0, 25.0));
        assert_eq!(last(&h), refusal, "{start:?}");
        assert_eq!(h.prompt(), prompt, "{start:?}: it asks again");
        h.escape();
        assert_eq!(h.entities(), before, "{start:?}");
        assert_eq!(h.session.drawing().revision(), revision);
    }
}

#[test]
fn a_locked_object_can_still_be_snapped_to_and_cut_against() {
    let mut h = crossed_by(LOCKED);
    h.enter("L");
    h.move_to_world(p(40.0, 49.0));
    assert_eq!(h.session.snap().map(|snap| snap.point), Some(p(40.0, 50.0)));
    h.escape();

    // The locked line is a cutting edge for the line on layer 0.
    h.enter_all(&["TR", ""]);
    h.click_world(p(80.0, 0.0));
    h.enter("");
    assert_eq!(h.entities()[0], line(0.0, 0.0, 40.0, 0.0));
    assert_eq!(h.entities()[1], line(40.0, -50.0, 40.0, 50.0));

    // And a boundary to extend to.
    h.enter_all(&["EX", ""]);
    h.click_world(p(25.0, 30.0));
    h.enter("");
    assert_eq!(h.entities()[2], line(0.0, 30.0, 40.0, 30.0));
}

#[test]
fn stretch_leaves_what_is_locked_and_says_how_many() {
    let mut h = crossed_by(LOCKED);
    // A window round the top of the locked line and the end of the short one.
    h.enter_all(&["S", "25,20", "50,60"]);
    let log = h.log();
    assert_eq!(
        &log[log.len() - 2..],
        ["1 was on a locked layer.", "1 found"]
    );
    h.enter_all(&["0,0", "0,10"]);
    assert_eq!(
        h.entities()[1],
        line(40.0, -50.0, 40.0, 50.0),
        "left as it was"
    );
    assert_eq!(h.entities()[2], line(0.0, 30.0, 30.0, 40.0));

    // Only locked objects in the window: nothing to do.
    h.enter_all(&["S", "35,-60", "45,-40"]);
    let log = h.log();
    assert_eq!(
        &log[log.len() - 2..],
        [
            "1 was on a locked layer.",
            "Nothing in the window to stretch."
        ]
    );
}

// ── current ──

#[test]
fn new_objects_are_put_on_the_current_layer_and_derived_ones_follow_their_source() {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let walls = tx.add_layer(Layer::named("walls")).unwrap();
    tx.set_current_layer(walls).unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);

    h.enter_all(&[
        "L", "0,0", "100,0", "", "C", "50,50", "10", "REC", "200,200", "220,210",
    ]);
    assert_eq!(h.properties(), vec![Properties::on(walls); 3]);

    // An offset of an object on layer 0 stays on layer 0, whatever is current.
    let drawing = Drawing::from_parts(
        [Layer::named("walls")],
        [(
            CircleEntity::new(p(0.0, 0.0), 20.0).into(),
            Properties::default(),
        )],
    )
    .unwrap();
    let mut drawing = drawing;
    let mut tx = drawing.transaction();
    tx.set_current_layer(LayerId::from_raw(1)).unwrap();
    tx.commit();
    let mut h = Harness::with_drawing(drawing);
    h.enter_all(&["O", "5"]);
    h.click_world(p(20.0, 0.0));
    h.click_world(p(60.0, 0.0));
    h.enter("");
    assert_eq!(h.properties(), vec![Properties::default(); 2]);
}

// ── the selection follows the layers ──

/// Switches the layer named `walls` off, or locks it: stands in for the
/// LAYER command, which has its own issue.
#[derive(Debug)]
struct SwitchWalls {
    off: bool,
}

impl Command for SwitchWalls {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        let id = ctx
            .drawing
            .layers()
            .find("walls")
            .expect("the layer exists");
        let mut layer = ctx.drawing.layers().get(id).cloned().expect("just found");
        if self.off {
            layer.on = false;
        } else {
            layer.locked = true;
        }
        let mut transaction = ctx.drawing.transaction();
        transaction
            .replace_layer(id, layer)
            .expect("a valid change");
        transaction.commit();
        Step::Finished
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point()
    }

    fn step(&mut self, _input: CommandInput, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Finished
    }
}

#[test]
fn a_selected_object_whose_layer_is_switched_off_or_locked_leaves_the_selection() {
    for command in ["WALLSOFF", "WALLSLOCK"] {
        let drawing = Drawing::from_parts(
            [Layer::named("walls")],
            [
                (line(0.0, 0.0, 100.0, 0.0), Properties::default()),
                (
                    line(0.0, 20.0, 100.0, 20.0),
                    Properties::on(LayerId::from_raw(1)),
                ),
            ],
        )
        .unwrap();
        let mut registry = CommandRegistry::standard();
        registry
            .register("WALLSOFF", &[], || Box::new(SwitchWalls { off: true }))
            .unwrap();
        registry
            .register("WALLSLOCK", &[], || Box::new(SwitchWalls { off: false }))
            .unwrap();
        let mut session =
            Session::with_registry(drawing, registry).with_toggles(Toggles::only(&[Toggle::Osnap]));
        session.resize(800.0, 600.0);
        let mut h = Harness { session };
        let ids = h.ids();

        h.select_box(p(200.0, 200.0), p(-200.0, -200.0));
        assert_eq!(h.session.selection().len(), 2);
        h.enter(command);
        let selected: Vec<_> = h.session.selection().iter().collect();
        assert_eq!(selected, vec![ids[0]], "{command}");

        // Undo brings the layer back; the object is selectable again.
        h.undo();
        h.escape();
        h.click_world(p(50.0, 20.0));
        assert_eq!(
            h.session.selection().iter().collect::<Vec<_>>(),
            vec![ids[1]]
        );
    }
}
