//! Advanced selection (#185): fence, window and crossing polygons,
//! Previous, Last, ALL, Remove and Add modes at every selection prompt;
//! SELECT at the idle prompt; QSELECT by kind and property.

use cad_core::{EntityId, Point2};
use cad_document::{
    CircleEntity, Color, Drawing, Entity, Layer, LayerId, LineEntity, Lineweight, PolylineEntity,
    Properties,
};

use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

/// A slot 200 long and 80 high with semicircular ends: the right end
/// bulges out to x = 240, the left to x = -40. Its vertices are all
/// between x = 0 and x = 200.
fn slot() -> Entity {
    PolylineEntity::with_bulges(
        vec![p(0.0, 0.0), p(200.0, 0.0), p(200.0, 80.0), p(0.0, 80.0)],
        vec![0.0, 1.0, 0.0, 1.0],
        true,
    )
    .into()
}

/// A line from (-100,0) to (100,0), a circle of radius 30 at (0,80), and
/// the slot.
fn scene() -> (Harness, EntityId, EntityId, EntityId) {
    let h = Harness::with_entities([
        line(-100.0, 0.0, 100.0, 0.0),
        CircleEntity::new(p(0.0, 80.0), 30.0).into(),
        slot(),
    ]);
    let ids = h.ids();
    (h, ids[0], ids[1], ids[2])
}

fn selected(h: &Harness) -> Vec<EntityId> {
    h.session.selection().iter().collect()
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

const METHODS: &str = "Fence/WPolygon/CPolygon/Previous/Last/ALL";

#[test]
fn every_selection_prompt_offers_the_methods() {
    let (mut h, ..) = scene();
    h.enter("E");
    assert_eq!(h.prompt(), format!("Select objects or [{METHODS}/Remove]:"));
    h.escape();
    h.enter("CHPROP");
    assert_eq!(h.prompt(), format!("Select objects or [{METHODS}/Remove]:"));
    h.escape();
    // A command that asks for a point does not.
    h.enter("L");
    assert_eq!(h.prompt(), "Specify first point:");
}

// ── fence ──

#[test]
fn a_fence_through_a_polylines_arc_segment_selects_it() {
    let (mut h, _, _, slot) = scene();
    h.enter("E");
    h.enter("F");
    assert_eq!(h.prompt(), "First fence point or [Undo]:");
    // x = 220 crosses the right semicircle only: no vertex and no chord
    // between vertices is anywhere near it.
    h.enter("220,-10");
    assert_eq!(h.prompt(), "Next fence point or [Undo]:");
    h.enter("220,90");
    h.enter("");
    assert_eq!(last(&h), "1 found, 1 total");
    assert_eq!(selected(&h), vec![slot]);
    assert_eq!(h.session.selection_summary().method, "fence");
    h.enter("");
    assert_eq!(h.entities().len(), 2);
}

#[test]
fn a_fence_takes_what_it_crosses_and_passes_the_rest() {
    let (mut h, line, circle, slot) = scene();
    h.enter("E");
    // x = -20 crosses the line, the circle, and the slot's left end.
    h.enter_all(&["F", "-20,-50", "-20,120", ""]);
    assert_eq!(last(&h), "3 found, 3 total");
    assert_eq!(selected(&h), vec![line, circle, slot]);
    h.escape();
    h.escape(); // the selection too, or ERASE takes it at once

    h.enter("E");
    h.enter_all(&["F", "300,-50", "300,120", ""]);
    assert_eq!(last(&h), "0 found, 0 total");
    assert_eq!(h.prompt(), format!("Select objects or [{METHODS}/Remove]:"));
}

#[test]
fn fence_points_may_be_clicked_and_the_overlay_shows_the_fence() {
    let (mut h, _, _, slot) = scene();
    h.enter("E");
    h.enter("F");
    h.click_world(p(220.0, -10.0));
    h.move_to_world(p(220.0, 90.0));
    let items = h.session.overlay();
    assert!(
        items.contains(&OverlayItem::SelectionPath {
            points: vec![p(220.0, -10.0)],
            to: p(220.0, 90.0),
            closed: false,
            crossing: true,
        }),
        "{items:?}"
    );
    h.click_world(p(220.0, 90.0));
    h.enter("");
    assert_eq!(selected(&h), vec![slot]);
}

#[test]
fn undo_takes_the_last_fence_point_back() {
    let (mut h, _, _, slot) = scene();
    h.enter("E");
    h.enter_all(&["F", "220,-10", "-20,-10", "U", "220,90", ""]);
    assert_eq!(selected(&h), vec![slot]);
}

#[test]
fn escape_cancels_the_fence_and_not_the_command() {
    let (mut h, ..) = scene();
    h.enter("E");
    h.enter_all(&["F", "0,0"]);
    let dirty = h.escape();
    assert!(dirty.contains(Dirty::OVERLAY | Dirty::UI));
    assert_eq!(h.prompt(), format!("Select objects or [{METHODS}/Remove]:"));
    h.escape();
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn a_fence_needs_two_points_and_a_polygon_three() {
    let (mut h, ..) = scene();
    h.enter("E");
    h.enter_all(&["F", "0,0", ""]);
    assert_eq!(last(&h), "A fence needs at least 2 points.");
    assert_eq!(h.prompt(), format!("Select objects or [{METHODS}/Remove]:"));
    h.enter_all(&["WP", "0,0", "1,1", ""]);
    assert_eq!(last(&h), "A polygon needs at least 3 points.");
    assert!(selected(&h).is_empty());
}

// ── polygons ──

#[test]
fn a_window_polygon_takes_only_what_lies_wholly_inside() {
    let (mut h, _, circle, _) = scene();
    h.enter("E");
    assert_eq!(h.prompt(), format!("Select objects or [{METHODS}/Remove]:"));
    h.enter("WP");
    assert_eq!(h.prompt(), "First polygon point or [Undo]:");
    // Around the circle; the line passes beneath, the slot to the right.
    h.enter_all(&["-50,40", "50,40", "50,120", "-50,120", ""]);
    assert_eq!(last(&h), "1 found, 1 total");
    assert_eq!(selected(&h), vec![circle]);
    assert_eq!(h.session.selection_summary().method, "window polygon");
}

#[test]
fn a_window_polygon_need_not_be_convex() {
    let (mut h, line, circle, _) = scene();
    // An L shape holding the line's whole length and the circle, with a
    // notch at the top right that the circle would cross if it were a box.
    h.enter("E");
    h.enter_all(&[
        "WP", "-120,-20", "120,-20", "120,20", "40,20", "40,130", "-120,130", "",
    ]);
    assert_eq!(selected(&h), vec![line, circle]);
    h.escape();
    h.escape();
    h.enter("E");
    // The same L with the notch cutting through the circle.
    h.enter_all(&[
        "WP", "-120,-20", "120,-20", "120,20", "10,20", "10,130", "-120,130", "",
    ]);
    assert_eq!(selected(&h), vec![line]);
}

#[test]
fn a_crossing_polygon_takes_what_it_touches_or_holds() {
    let (mut h, line, circle, slot) = scene();
    h.enter("E");
    h.enter_all(&["CP", "-50,-20", "50,-20", "50,120", "-50,120", ""]);
    assert_eq!(last(&h), "3 found, 3 total");
    assert_eq!(selected(&h), vec![line, circle, slot]);
    assert_eq!(h.session.selection_summary().method, "crossing polygon");
    h.escape();
    h.escape();

    h.enter("E");
    h.enter_all(&["CP", "100,20", "150,20", "150,60", "100,60", ""]);
    assert_eq!(last(&h), "0 found, 0 total");
}

#[test]
fn a_polygon_inside_a_text_frame_takes_the_text() {
    let mut h = Harness::new();
    h.enter_all(&["DT", "0,0", "50", "0", "Wide"]);
    let text = h.ids()[0];
    h.enter("E");
    h.enter_all(&["CP", "20,10", "30,10", "30,20", ""]);
    assert_eq!(selected(&h), vec![text]);
}

// ── previous, last, all ──

#[test]
fn previous_brings_back_what_the_last_command_took() {
    let (mut h, line, circle, slot) = scene();
    h.select_box(p(-150.0, -40.0), p(120.0, 130.0));
    assert_eq!(selected(&h), vec![line, circle]);
    h.enter_all(&["M", "0,0", "0,10"]);
    assert!(selected(&h).is_empty());

    h.enter("E");
    h.enter("P");
    assert_eq!(last(&h), "2 found, 2 total");
    assert_eq!(selected(&h), vec![line, circle]);
    assert_eq!(h.session.selection_summary().method, "previous");
    h.enter("");
    assert_eq!(h.ids(), vec![slot]);
}

#[test]
fn previous_is_also_what_a_command_collected_itself() {
    let (mut h, line, ..) = scene();
    h.enter("E");
    h.enter_all(&["F", "-50,-10", "-50,10", ""]);
    assert_eq!(selected(&h), vec![line]);
    h.escape();
    h.enter_all(&["CHPROP", "P"]);
    assert_eq!(selected(&h), vec![line]);
}

#[test]
fn without_a_previous_selection_the_keyword_says_so() {
    let (mut h, ..) = scene();
    h.enter_all(&["E", "P"]);
    assert_eq!(last(&h), "There is no previous selection.");
    assert_eq!(h.prompt(), format!("Select objects or [{METHODS}/Remove]:"));
}

#[test]
fn last_takes_the_newest_object() {
    let (mut h, _, _, slot) = scene();
    h.enter_all(&["E", "L"]);
    assert_eq!(last(&h), "1 found, 1 total");
    assert_eq!(selected(&h), vec![slot]);
    assert_eq!(h.session.selection_summary().method, "last");
}

/// Layer 1 is off, layer 2 locked.
fn layered() -> Harness {
    let drawing = Drawing::from_parts(
        [
            Layer {
                on: false,
                ..Layer::named("hidden")
            },
            Layer {
                locked: true,
                ..Layer::named("locked")
            },
        ],
        [],
        [
            (line(0.0, 0.0, 100.0, 0.0), Properties::default()),
            (
                line(0.0, 10.0, 100.0, 10.0),
                Properties::on(LayerId::from_raw(1)),
            ),
            (
                line(0.0, 20.0, 100.0, 20.0),
                Properties::on(LayerId::from_raw(2)),
            ),
            (line(0.0, 30.0, 100.0, 30.0), Properties::default()),
        ],
    )
    .unwrap();
    Harness::with_drawing(drawing)
}

#[test]
fn all_takes_everything_shown_and_counts_what_is_locked() {
    let mut h = layered();
    let ids = h.ids();
    h.enter_all(&["E", "ALL"]);
    assert_eq!(selected(&h), vec![ids[0], ids[3]]);
    let log = h.log();
    assert_eq!(log[log.len() - 2], "2 found, 2 total");
    assert_eq!(log[log.len() - 1], "1 was on a locked layer.");
    assert_eq!(h.session.selection_summary().method, "all");
}

#[test]
fn last_and_a_fence_pass_over_hidden_and_locked_objects() {
    let mut h = layered();
    let ids = h.ids();
    h.enter_all(&["E", "L"]);
    assert_eq!(selected(&h), vec![ids[3]]);
    h.escape();
    h.escape();
    h.enter_all(&["E", "F", "50,-5", "50,35", ""]);
    assert_eq!(selected(&h), vec![ids[0], ids[3]]);
    assert_eq!(last(&h), "1 was on a locked layer.");
}

// ── remove and add ──

#[test]
fn remove_mode_takes_objects_out_until_add() {
    let (mut h, line, circle, slot) = scene();
    h.enter_all(&["E", "ALL"]);
    assert_eq!(selected(&h), vec![line, circle, slot]);
    h.enter("R");
    assert_eq!(h.prompt(), format!("Remove objects or [{METHODS}/Add]:"));
    h.click_world(p(0.0, 0.0));
    assert_eq!(last(&h), "1 removed, 2 total");
    assert_eq!(selected(&h), vec![circle, slot]);
    // A fence in Remove mode removes.
    h.enter_all(&["F", "220,-10", "220,90", ""]);
    assert_eq!(last(&h), "1 removed, 1 total");
    assert_eq!(selected(&h), vec![circle]);
    h.enter("A");
    assert_eq!(h.prompt(), format!("Select objects or [{METHODS}/Remove]:"));
    h.click_world(p(0.0, 0.0));
    assert_eq!(last(&h), "1 found, 2 total");
    h.enter("");
    assert_eq!(h.ids(), vec![slot]);
}

#[test]
fn remove_mode_ends_with_the_prompt() {
    let (mut h, line, ..) = scene();
    h.enter_all(&["E", "R"]);
    h.escape();
    h.enter("E");
    assert_eq!(h.prompt(), format!("Select objects or [{METHODS}/Remove]:"));
    h.click_world(p(0.0, 0.0));
    assert_eq!(selected(&h), vec![line]);
}

// ── SELECT ──

#[test]
fn select_builds_a_selection_the_next_command_takes() {
    let (mut h, line, circle, slot) = scene();
    h.enter_all(&["SELECT", "ALL", "R"]);
    h.click_world(p(0.0, 80.0 + 30.0));
    h.enter("");
    assert_eq!(last(&h), "2 selected.");
    assert_eq!(selected(&h), vec![line, slot]);
    h.enter("E");
    assert_eq!(h.ids(), vec![circle]);
}

// ── the summary ──

#[test]
fn the_summary_names_how_the_selection_was_made() {
    let (mut h, ..) = scene();
    let method = |h: &Harness| h.session.selection_summary().method;
    assert_eq!(method(&h), "");
    h.click_world(p(0.0, 0.0));
    assert_eq!(method(&h), "click");
    h.select_box(p(-150.0, -40.0), p(120.0, 130.0));
    assert_eq!(method(&h), "window");
    h.escape();
    h.select_box(p(120.0, 130.0), p(-150.0, -40.0));
    assert_eq!(method(&h), "crossing");
    h.escape();
    assert_eq!(method(&h), "");
}

// ── QSELECT ──

/// Lines of colours 1, 3, and 5, the last on layer `walls` with a
/// lineweight, and a circle; one hidden, one locked.
fn styled() -> Harness {
    let colored = |color: u8| Properties {
        color: Color::Index(color),
        ..Properties::default()
    };
    let drawing = Drawing::from_parts(
        [
            Layer::named("walls"),
            Layer {
                on: false,
                ..Layer::named("hidden")
            },
            Layer {
                locked: true,
                ..Layer::named("locked")
            },
        ],
        [],
        [
            (line(0.0, 0.0, 100.0, 0.0), colored(1)),
            (line(0.0, 10.0, 100.0, 10.0), colored(3)),
            (
                line(0.0, 20.0, 100.0, 20.0),
                Properties {
                    lineweight: Lineweight::Hundredths(50),
                    ..Properties::on(LayerId::from_raw(1))
                },
            ),
            (
                CircleEntity::new(p(0.0, 0.0), 10.0).into(),
                Properties::on(LayerId::from_raw(1)),
            ),
            (
                line(0.0, 30.0, 100.0, 30.0),
                Properties::on(LayerId::from_raw(2)),
            ),
            (
                line(0.0, 40.0, 100.0, 40.0),
                Properties::on(LayerId::from_raw(3)),
            ),
        ],
    )
    .unwrap();
    Harness::with_drawing(drawing)
}

#[test]
fn qselect_by_kind_alone() {
    let mut h = styled();
    let ids = h.ids();
    h.enter("QS");
    assert_eq!(h.prompt(), "Object kind (line, circle, text, …) <all>:");
    h.enter("line");
    assert_eq!(
        h.prompt(),
        "Property to test or [Layer/Color/LType/LWeight] <none>:"
    );
    h.enter("");
    assert_eq!(last(&h), "3 found.");
    assert_eq!(selected(&h), vec![ids[0], ids[1], ids[2]]);
    assert_eq!(h.session.selection_summary().method, "quick select");
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn qselect_every_kind_shown_and_unlocked() {
    let mut h = styled();
    h.enter_all(&["QS", "", ""]);
    assert_eq!(last(&h), "4 found.");
    h.enter_all(&["QS", "circles", ""]);
    assert_eq!(last(&h), "1 found.");
}

#[test]
fn qselect_by_layer_equal_and_not_equal() {
    let mut h = styled();
    let ids = h.ids();
    h.enter_all(&["QS", "", "L"]);
    assert_eq!(h.prompt(), "Layer name, after = or <>:");
    h.enter("= walls");
    assert_eq!(selected(&h), vec![ids[2], ids[3]]);
    h.enter_all(&["QS", "line", "L", "<> WALLS"]);
    assert_eq!(selected(&h), vec![ids[0], ids[1]]);
}

#[test]
fn qselect_compares_colour_numbers_and_lineweights() {
    let mut h = styled();
    let ids = h.ids();
    h.enter_all(&["QS", "", "C", "< 3"]);
    assert_eq!(selected(&h), vec![ids[0]]);
    h.enter_all(&["QS", "", "C", "> 1"]);
    assert_eq!(selected(&h), vec![ids[1]]);
    h.enter_all(&["QS", "", "C", "red"]);
    assert_eq!(selected(&h), vec![ids[0]]);
    h.enter_all(&["QS", "", "C", "<> bylayer"]);
    assert_eq!(selected(&h), vec![ids[0], ids[1]]);
    h.enter_all(&["QS", "", "LW", "> 0.3"]);
    assert_eq!(selected(&h), vec![ids[2]]);
    h.enter_all(&["QS", "", "LW", "= 0.5"]);
    assert_eq!(selected(&h), vec![ids[2]]);
    h.enter_all(&["QS", "", "LT", "= continuous"]);
    assert_eq!(last(&h), "0 found.");
    h.enter_all(&["QS", "", "LT", "<> continuous"]);
    assert_eq!(last(&h), "4 found.");
}

#[test]
fn qselect_refuses_what_makes_no_sense_and_asks_again() {
    let mut h = styled();
    h.enter_all(&["QS", "bogus"]);
    assert_eq!(
        last(&h),
        "No kind named \"bogus\". The kinds are line, circle, arc, ellipse, spline, polyline, \
         text, multiline text, attribute definition, block reference, dimension, hatch."
    );
    assert_eq!(h.prompt(), "Object kind (line, circle, text, …) <all>:");
    h.enter_all(&["", "L", "< walls"]);
    assert_eq!(last(&h), "A layer is tested with = or <>.");
    h.enter("nowhere");
    assert_eq!(last(&h), "No layer named \"nowhere\".");
    h.enter("= walls");
    assert_eq!(last(&h), "2 found.");
    h.enter_all(&["QS", "", "C", "< bylayer"]);
    assert_eq!(last(&h), "< and > compare colour numbers.");
    h.enter("purple");
    assert_eq!(last(&h), "A colour is 1-255, a name, ByLayer, or ByBlock.");
    h.escape();
    h.enter_all(&["QS", "", "LW", "< default"]);
    assert_eq!(last(&h), "< and > compare widths in millimetres.");
    h.enter("0.37");
    assert_eq!(
        last(&h),
        "A lineweight is a standard width in millimetres, Default, ByLayer, or ByBlock."
    );
    h.escape();
    h.enter_all(&["QS", "", "LT", "< continuous"]);
    assert_eq!(last(&h), "A linetype is tested with = or <>.");
}
