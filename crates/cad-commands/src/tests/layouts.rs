//! The spaces of the drawing in the session (#193): LAYOUT, MODEL,
//! PSPACE, MSPACE; a view per space; what a command makes is in the
//! space the session is on; the model worked through a viewport.

use cad_core::Point2;
use cad_document::{Entity, LayoutId, ViewportEntity};

use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn viewport(h: &Harness, space: LayoutId) -> ViewportEntity {
    h.session
        .drawing()
        .shown_in(space)
        .find_map(|(_, entity)| match entity {
            Entity::Viewport(viewport) => Some(*viewport),
            _ => None,
        })
        .expect("a viewport")
}

#[test]
fn layout_new_makes_a_layout_and_goes_to_it_and_what_is_drawn_there_is_its() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", ""]);
    h.enter("LAYOUT");
    assert_eq!(
        h.prompt(),
        "Enter layout option or [?/New/Rename/Delete/Set] <Set>:"
    );
    h.enter("N");
    assert_eq!(h.prompt(), "Enter name for the new layout:");
    let dirty = h.enter("Sheet one");
    assert!(dirty.contains(Dirty::SCENE | Dirty::VIEW | Dirty::UI));
    assert_eq!(last(&h), "Layout \"Sheet one\" made.");
    let sheet = h.session.space();
    assert_ne!(sheet, LayoutId::MODEL);
    let state = h.session.ui_state();
    assert_eq!(state.space, "Sheet one");
    assert!(state.paper);
    assert_eq!(state.layouts, vec!["Sheet one"]);
    // A line drawn here is the layout's, not the model's.
    h.enter_all(&["L", "10,10", "50,10", ""]);
    assert_eq!(h.session.drawing().shown_in(sheet).count(), 1);
    assert_eq!(
        h.session.drawing().shown().count(),
        1,
        "the model's line alone"
    );
    // The layout's view is its sheet: A3 landscape, 420 by 297.
    let view = h.session.view();
    assert!((view.center().x - 210.0).abs() < 1e-9 && (view.center().y - 148.5).abs() < 1e-9);
    h.enter_all(&["LAYOUT", "?"]);
    assert_eq!(last(&h), "Layouts: Model, Sheet one (current).");
}

#[test]
fn each_space_keeps_its_view_and_model_goes_back() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", ""]);
    h.enter_all(&["ZOOM", "C", "50,0", "20"]);
    let model_view = h.session.view().clone();
    h.enter_all(&["LAYOUT", "N", "Sheet"]);
    assert_ne!(h.session.view().center(), model_view.center());
    h.enter_all(&["ZOOM", "C", "100,100", "50"]);
    let sheet_view = h.session.view().clone();
    let dirty = h.enter("MODEL");
    assert!(dirty.contains(Dirty::VIEW));
    assert_eq!(h.session.space(), LayoutId::MODEL);
    assert_eq!(h.session.view().center(), model_view.center());
    assert_eq!(h.session.view().scale(), model_view.scale());
    assert!(!h.session.ui_state().paper);
    h.enter_all(&["LAYOUT", "S", "sheet"]);
    assert_eq!(h.session.view().center(), sheet_view.center());
    assert_eq!(h.session.ui_state().space, "Sheet");
    h.enter_all(&["LAYOUT", "S", "sheet"]);
    assert_eq!(last(&h), "Already on Sheet.");
    h.enter_all(&["LAYOUT", "S", "nowhere"]);
    assert_eq!(last(&h), "No layout named \"nowhere\".");
    h.escape();
    // ZOOM Extents on a layout is the sheet.
    h.enter_all(&["ZOOM", "E"]);
    assert!((h.session.view().center().x - 210.0).abs() < 1e-9);
}

#[test]
fn layout_rename_and_delete_and_the_model_refuses_both() {
    let mut h = Harness::new();
    h.enter_all(&["LAYOUT", "N", "Sheet"]);
    let sheet = h.session.space();
    h.enter_all(&["L", "10,10", "50,10", ""]);
    h.enter_all(&["LAYOUT", "R", "", "Details"]);
    assert_eq!(last(&h), "Layout renamed to \"Details\".");
    assert_eq!(h.session.ui_state().space, "Details");
    h.enter_all(&["LAYOUT", "R", "Model"]);
    assert_eq!(last(&h), "The model keeps its name.");
    h.escape();
    h.enter_all(&["LAYOUT", "D", "Model"]);
    assert_eq!(last(&h), "The model cannot be deleted.");
    h.escape();
    h.enter_all(&["LAYOUT", "D", ""]);
    assert_eq!(
        last(&h),
        "Layout \"Details\" deleted with the 1 object on it."
    );
    assert_eq!(h.session.space(), LayoutId::MODEL);
    assert!(!h.session.drawing().layouts().contains(sheet));
    assert_eq!(h.session.ui_state().layouts, Vec::<String>::new());
    // One undo brings the layout and its line back.
    h.undo();
    assert!(h.session.drawing().layouts().contains(sheet));
    assert_eq!(h.session.drawing().entities_in(sheet).count(), 1);
}

#[test]
fn mspace_works_the_model_through_a_viewport_and_pspace_hands_the_view_back() {
    let mut h = Harness::new();
    h.enter_all(&["REC", "0,0", "1000,500"]);
    h.enter_all(&["LAYOUT", "N", "Sheet"]);
    let sheet = h.session.space();
    h.enter_all(&["MVIEW", "10,10", "210,110"]);
    let window = viewport(&h, sheet);
    assert!((window.scale - 0.2).abs() < 1e-12);
    let paper_view = h.session.view().clone();
    h.enter("MSPACE");
    assert_eq!(
        h.session.space(),
        LayoutId::MODEL,
        "the model, through the viewport"
    );
    let state = h.session.ui_state();
    assert!(state.through && !state.paper);
    assert_eq!(state.space, "Sheet", "the layout the viewport is on");
    // The viewport's frame on screen shows the same model points.
    let frame_centre = paper_view.world_to_screen(window.center);
    let seen = h.session.view().screen_to_world(frame_centre);
    assert!(seen.distance_to(window.view_center) < 1e-6, "{seen:?}");
    assert!((h.session.view().scale() - paper_view.scale() * window.scale).abs() < 1e-9);
    // The frame is outlined in the model's coordinates.
    h.move_to(400.0, 300.0);
    let corners = h
        .session
        .overlay()
        .into_iter()
        .find_map(|item| match item {
            OverlayItem::ViewportFrame { corners } => Some(corners),
            _ => None,
        })
        .expect("the viewport's frame");
    assert!(corners[0].distance_to(window.model_point(window.corners()[0])) < 1e-9);
    // A line drawn here is the model's, at the viewport's scale: typed
    // coordinates are model coordinates.
    h.enter_all(&["L", "0,600", "100,600", ""]);
    assert_eq!(h.session.drawing().shown().count(), 2);
    assert_eq!(
        h.session.drawing().shown_in(sheet).count(),
        1,
        "the viewport alone"
    );
    // Zoomed in the model, PSPACE gives the viewport the view: its scale
    // is the model's pixels per unit over the paper's, and its centre
    // the model point its frame's centre showed.
    h.enter_all(&["ZOOM", "C", "500,250", "250"]);
    let model_view = h.session.view().clone();
    let expected_scale = model_view.scale() / paper_view.scale();
    let expected_center = model_view.screen_to_world(paper_view.world_to_screen(window.center));
    let dirty = h.enter("PSPACE");
    assert!(dirty.contains(Dirty::VIEW));
    assert_eq!(h.session.space(), sheet);
    assert_eq!(h.session.view().center(), paper_view.center());
    let updated = viewport(&h, sheet);
    assert!(
        (updated.scale - expected_scale).abs() < 1e-9 && updated.scale > window.scale,
        "closer in: {}",
        updated.scale
    );
    assert!(updated.view_center.distance_to(expected_center) < 1e-6);
    assert_eq!(last(&h), "The viewport shows the model as you left it.");
    assert!(!h.session.ui_state().through);
    // Undone as one step, and PSPACE without MSPACE says so.
    h.undo();
    assert!((viewport(&h, sheet).scale - 0.2).abs() < 1e-12);
    h.enter("PSPACE");
    assert_eq!(last(&h), "Not in a viewport: PSPACE needs MSPACE first.");
}

#[test]
fn mspace_needs_a_layout_with_a_viewport_and_picks_among_several() {
    let mut h = Harness::new();
    h.enter("MSPACE");
    assert_eq!(
        last(&h),
        "MSPACE works through a viewport of a layout: go to a layout first."
    );
    h.enter_all(&["LAYOUT", "N", "Sheet"]);
    h.enter("MS");
    assert_eq!(last(&h), "This layout has no viewport: MVIEW makes one.");
    h.enter_all(&["MVIEW", "10,10", "110,60"]);
    h.enter_all(&["MVIEW", "200,10", "300,60"]);
    h.enter("MSPACE");
    assert_eq!(h.prompt(), "Select viewport:");
    h.click_world(p(250.0, 60.0));
    assert_eq!(h.session.space(), LayoutId::MODEL);
    assert!(h.session.viewport_through().is_some());
    // MODEL from the model through a viewport: the viewport takes the
    // view, and the session stays in the model with it.
    h.enter_all(&["ZOOM", "C", "500,250", "100"]);
    h.enter("MODEL");
    assert_eq!(h.session.space(), LayoutId::MODEL);
    assert!(h.session.viewport_through().is_none());
    assert_eq!(last(&h), "The viewport shows the model as you left it.");
}
