//! VIEWFRAME (ADR-0041, #351): views placed on a sheet by what they hold,
//! projected views aligned with their parent, one undo, never crowded.

use cad_core::{EntityId, Point2};
use cad_geometry::composition::{Looking, Role};

use crate::compose::{ViewRecord, sheet_of, views_on};
use crate::testing::Harness;

/// An A3 landscape sheet at 1:1, third angle, its corner at the origin.
fn sheet() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&[
        "UNITS",
        "mm",
        "TITLEBLOCK",
        "A3",
        "Landscape",
        "1",
        "PART",
        "",
        "",
        "",
        "",
        "Third",
        "0,0",
    ]);
    h
}

/// A rectangle drawn at its view's own origin; its id.
fn drawn(h: &mut Harness, w: f64, ht: f64) -> EntityId {
    h.enter_all(&["RECTANG", "0,0", &format!("{w},{ht}")]);
    *h.ids().last().unwrap()
}

fn view(h: &Harness, id: &str) -> ViewRecord {
    let sheet = sheet_of(h.session.drawing()).unwrap();
    views_on(h.session.drawing(), &sheet.group)
        .into_iter()
        .find(|v| v.id == id)
        .unwrap_or_else(|| panic!("no view {id}: {:?}", h.log()))
}

#[test]
fn a_front_and_its_left_view_are_placed_in_line_as_one_undo() {
    let mut h = sheet();
    let front = drawn(&mut h, 100.0, 60.0);
    let left = drawn(&mut h, 40.0, 60.0);
    h.session.select_only(&[front]);
    h.enter_all(&["VIEWFRAME", "0,0", "front", "", ""]);
    assert_eq!(h.prompt(), "Another view? [Yes/No] <No>:");
    h.enter("Yes");
    h.session.select_only(&[left]);
    h.enter_all(&[
        "",
        "0,0",
        "left",
        "LEFT SIDE",
        "PRojected",
        "front",
        "Left",
        "No",
    ]);
    assert_eq!(
        h.log().last().unwrap(),
        "2 views placed on TITLE-1: front, left."
    );

    let (f, l) = (view(&h, "front"), view(&h, "left"));
    assert_eq!(f.role, Role::Principal);
    assert_eq!(
        l.role,
        Role::Projected {
            of: "front".into(),
            looking: Looking::Left
        }
    );
    // In line, by their origins; third angle: the left view on the left.
    assert_eq!(l.origin.y, f.origin.y);
    assert!(l.origin.x < f.origin.x);
    // The objects moved with their views, and the titles are drawn.
    let drawing = h.session.drawing();
    let corner = |id: EntityId| match drawing.entity(id) {
        Some(cad_document::Entity::Polyline(p)) => p
            .vertices
            .iter()
            .fold(Point2::new(f64::MAX, f64::MAX), |a, v| {
                Point2::new(a.x.min(v.x), a.y.min(v.y))
            }),
        other => panic!("{other:?}"),
    };
    assert_eq!(corner(front), f.origin);
    assert_eq!(corner(left), l.origin);
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, cad_document::Entity::Text(t) if t.content == "LEFT SIDE"))
    );
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, cad_document::Entity::Text(t) if t.content == "FRONT"))
    );
    // Inside the border, clear of the block.
    let frame = sheet_of(drawing).unwrap().frame;
    for v in [&f, &l] {
        let b = cad_core::Aabb::from_points(
            v.origin + (v.extent.min() - Point2::ORIGIN),
            v.origin + (v.extent.max() - Point2::ORIGIN),
        );
        assert!(b.min().x >= frame.border.min().x && b.max().x <= frame.border.max().x);
        assert!(b.min().y >= frame.border.min().y && b.max().y <= frame.border.max().y);
    }

    // One undo: back where they were drawn, no views.
    h.enter("U");
    assert_eq!(corner_of(&h, front), Point2::ORIGIN);
    assert!(views_on(h.session.drawing(), "TITLE-1").is_empty());
}

fn corner_of(h: &Harness, id: EntityId) -> Point2 {
    match h.session.drawing().entity(id) {
        Some(cad_document::Entity::Polyline(p)) => p
            .vertices
            .iter()
            .fold(Point2::new(f64::MAX, f64::MAX), |a, v| {
                Point2::new(a.x.min(v.x), a.y.min(v.y))
            }),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_view_added_later_takes_its_slot_and_moves_nothing_placed() {
    let mut h = sheet();
    let front = drawn(&mut h, 100.0, 60.0);
    h.session.select_only(&[front]);
    h.enter_all(&["VIEWFRAME", "0,0", "front", "", "", "No"]);
    let before = view(&h, "front");
    let top = drawn(&mut h, 100.0, 40.0);
    h.session.select_only(&[top]);
    h.enter_all(&["VIEWFRAME", "0,0", "top", "", "", "", "Above", "No"]);
    let (f, t) = (view(&h, "front"), view(&h, "top"));
    assert_eq!(f, before, "the front did not move");
    assert_eq!(t.origin.x, f.origin.x, "a column shares widths");
    assert!(t.origin.y > f.origin.y);
}

#[test]
fn what_cannot_be_placed_is_refused_and_nothing_moves() {
    // No sheet.
    let mut h = Harness::new();
    let id = drawn(&mut h, 10.0, 10.0);
    h.session.select_only(&[id]);
    h.enter("VIEWFRAME");
    assert_eq!(
        h.log().last().unwrap(),
        "There is no sheet to place views on: draw one with TITLEBLOCK first."
    );

    // Too large for the sheet: said in paper millimetres; nothing moved.
    let mut h = sheet();
    let huge = drawn(&mut h, 900.0, 60.0);
    let revision = h.session.drawing().revision();
    h.session.select_only(&[huge]);
    h.enter_all(&["VIEWFRAME", "0,0", "front", "", "", "No"]);
    let said = h.log().last().unwrap().clone();
    assert!(said.starts_with("View front needs 900 ×"), "{said}");
    assert_eq!(h.session.drawing().revision(), revision);

    // A detail too large at its own scale is told that its ratio sets its
    // size, not the sheet's scale.
    let mut h = sheet();
    let detail = drawn(&mut h, 300.0, 100.0);
    h.session.select_only(&[detail]);
    h.enter_all(&["VIEWFRAME", "0,0", "detail", "", "Free", "2:1", "No"]);
    let said = h.log().last().unwrap().clone();
    assert!(
        said.starts_with("View detail, at its own scale 2:1, needs 600 ×")
            && said.contains("its ratio alone sets its size on paper"),
        "{said}"
    );

    // A second view projected into a taken slot.
    let mut h = sheet();
    let a = drawn(&mut h, 50.0, 50.0);
    let b = drawn(&mut h, 20.0, 50.0);
    let c = drawn(&mut h, 20.0, 50.0);
    h.session.select_only(&[a]);
    h.enter_all(&["VIEWFRAME", "0,0", "front", "", "", "Yes"]);
    h.session.select_only(&[b]);
    h.enter_all(&["", "0,0", "l1", "", "PRojected", "front", "Left", "Yes"]);
    h.session.select_only(&[c]);
    h.enter_all(&["", "0,0", "l2", "", "PRojected", "front", "Left", "No"]);
    assert!(
        h.log()
            .last()
            .unwrap()
            .contains("would take the place of another view of front from the left"),
        "{:?}",
        h.log().last()
    );
    assert!(views_on(h.session.drawing(), "TITLE-1").is_empty());
}

/// What a view's dimensions reach is measured with it (ADR-0041 §2): a
/// front view dimensioned well below its geometry is placed with its
/// dimension clear of the view beneath it, and the box it was placed by is
/// where its objects are.
#[test]
fn a_view_is_placed_by_what_it_draws_dimensions_and_all() {
    let mut h = sheet();
    h.enter_all(&["RECTANG", "0,0", "100,60"]);
    h.enter_all(&["DIMLINEAR", "0,0", "100,0", "50,-40"]);
    let front: Vec<EntityId> = h.ids();
    let front: Vec<EntityId> = front[front.len() - 2..].to_vec();
    let below = drawn(&mut h, 100.0, 30.0);
    h.session.select_only(&front);
    h.enter_all(&["VIEWFRAME", "0,0", "front", "", "", "Yes"]);
    h.session.select_only(&[below]);
    h.enter_all(&["", "0,0", "bottom", "", "PRojected", "front", "Below", "No"]);
    let (f, b) = (view(&h, "front"), view(&h, "bottom"));
    // The front's box reaches its dimension, 40 below its geometry and more.
    assert!(f.extent.min().y <= -40.0, "{:?}", f.extent);
    let drawing = h.session.drawing();
    let tables = drawing.tables();
    let dimension = drawing.entity(front[1]).unwrap().bounds(tables);
    let bottom_top = b.origin.y + b.extent.max().y;
    assert!(
        dimension.min().y >= bottom_top + 10.0 - 1e-9,
        "{dimension:?} over {bottom_top}"
    );
    // What was measured is where it is: the front's objects inside its box.
    let placed = cad_core::Aabb::from_points(
        f.origin + (f.extent.min() - Point2::ORIGIN),
        f.origin + (f.extent.max() - Point2::ORIGIN),
    );
    for id in &front {
        let b = drawing.entity(*id).unwrap().bounds(tables);
        assert!(b.min().x >= placed.min().x - 1e-9 && b.max().x <= placed.max().x + 1e-9);
        assert!(b.min().y >= placed.min().y - 1e-9 && b.max().y <= placed.max().y + 1e-9);
    }
}

/// A detail at its own scale (ADR-0041 §4, #352): a 2:1 detail on a 1:3
/// sheet is drawn k = 6 times true size; its dimension reads true through a
/// style made for it — linear factor 1/6, overall scale 3 — its text keeps
/// its height, and its title says its scale.
#[test]
fn a_two_to_one_detail_on_a_one_to_three_sheet_is_drawn_six_times_and_reads_true() {
    let mut h = Harness::new();
    h.enter_all(&[
        "UNITS",
        "mm",
        "TITLEBLOCK",
        "A3",
        "Landscape",
        "3",
        "SHAFT",
        "",
        "",
        "",
        "",
        "Third",
        "0,0",
    ]);
    h.enter_all(&["LINE", "0,0", "10,0", ""]);
    h.enter_all(&["DIMLINEAR", "0,0", "10,0", "5,-5"]);
    h.enter_all(&["TEXT", "0,5", "2.5", "0", "KEY"]);
    let ids = h.ids();
    let detail: Vec<EntityId> = ids[ids.len() - 3..].to_vec();
    h.session.select_only(&detail);
    h.enter_all(&[
        "VIEWFRAME",
        "0,0",
        "detail",
        "DETAIL A",
        "Free",
        "2:1",
        "No",
    ]);
    assert!(
        h.log().last().unwrap().starts_with("1 view placed"),
        "{:?}",
        h.log()
    );
    let v = view(&h, "detail");
    assert_eq!(v.ratio, Some(2.0));
    let drawing = h.session.drawing();
    let tables = drawing.tables();
    let (line, dimension, text) = (
        drawing.entity(detail[0]).unwrap(),
        drawing.entity(detail[1]).unwrap(),
        drawing.entity(detail[2]).unwrap(),
    );
    match line {
        cad_document::Entity::Line(l) => assert!((l.start.distance_to(l.end) - 60.0).abs() < 1e-9),
        other => panic!("{other:?}"),
    }
    let cad_document::Entity::Dimension(d) = dimension else {
        panic!("{dimension:?}")
    };
    let style = tables.dim_style(d.style);
    assert_eq!(style.name, "detail-Standard");
    assert!((style.linear_factor - 1.0 / 6.0).abs() < 1e-12);
    assert_eq!(style.scale, 3.0);
    assert!(
        d.layout(style).text.content.starts_with("10.00"),
        "{}",
        d.layout(style).text.content
    );
    match text {
        cad_document::Entity::Text(t) => assert_eq!(t.height, 2.5, "a text keeps its height"),
        other => panic!("{other:?}"),
    }
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, cad_document::Entity::Text(t) if t.content == "DETAIL A (2:1)"))
    );
    // Only a free view has a scale of its own.
    let mut h = sheet();
    let id = drawn(&mut h, 10.0, 10.0);
    h.session.select_only(&[id]);
    h.enter_all(&["VIEWFRAME", "0,0", "front", "", "Free", "nonsense"]);
    assert_eq!(
        h.log().last().unwrap(),
        "A scale is paper to real, as 2:1 or 1:5, greater than zero."
    );
}

/// A section view (ADR-0041 §5, #353): its cut drawn across its parent —
/// the cutting line on SECTION-MARKS in CENTER, an arrow and the letter at
/// each end looking toward the side asked — titled to match, the next
/// letter for the next; a cut that crosses nothing is refused.
#[test]
fn a_section_is_marked_across_its_parent_and_titled_to_match() {
    let mut h = sheet();
    let front = drawn(&mut h, 100.0, 60.0);
    let section = drawn(&mut h, 60.0, 60.0);
    h.session.select_only(&[front]);
    h.enter_all(&["VIEWFRAME", "0,0", "front", "", "", "Yes"]);
    h.session.select_only(&[section]);
    h.enter_all(&[
        "", "0,0", "cut", "", "Section", "front", "50,-10", "50,70", "100,30", "", "No",
    ]);
    assert!(
        h.log().last().unwrap().starts_with("2 views placed"),
        "{:?}",
        h.log().last()
    );
    let (f, s) = (view(&h, "front"), view(&h, "cut"));
    assert_eq!(s.section, Some(('A', "front".to_owned())));
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, cad_document::Entity::Text(t) if t.content == "SECTION A-A"))
    );
    let drawing = h.session.drawing();
    let group = drawing
        .groups()
        .iter()
        .find(|(_, g)| g.name == "SECTION-A")
        .map(|(_, g)| g.members.clone())
        .expect("the marker's group");
    let layer = drawing
        .layers()
        .find(crate::compose::SECTION_LAYER)
        .unwrap();
    let marks: Vec<&cad_document::Entity> = group
        .iter()
        .map(|id| drawing.entity(*id).unwrap())
        .collect();
    assert_eq!(
        marks.len(),
        7,
        "a line, two arrows of a shaft and a head, two letters"
    );
    for id in &group {
        assert_eq!(drawing.properties(*id).unwrap().layer, layer);
    }
    let linetype = &drawing.layers().get(layer).unwrap().linetype;
    assert!(matches!(linetype, cad_document::Linetype::Named(_)));
    // The cut where it was asked, on the front as placed; the arrows look
    // toward +x, the side asked.
    let cad_document::Entity::Line(cut) = marks[0] else {
        panic!("{:?}", marks[0])
    };
    assert_eq!(cut.start, f.origin + cad_core::Vec2::new(50.0, -10.0));
    assert_eq!(cut.end, f.origin + cad_core::Vec2::new(50.0, 70.0));
    let cad_document::Entity::Line(shaft) = marks[1] else {
        panic!()
    };
    assert!(shaft.end.x > shaft.start.x);
    assert_eq!(
        marks
            .iter()
            .filter(|e| matches!(e, cad_document::Entity::Text(t) if t.content == "A"))
            .count(),
        2
    );

    // The next section takes B.
    let again = drawn(&mut h, 20.0, 20.0);
    h.session.select_only(&[again]);
    h.enter_all(&[
        "VIEWFRAME",
        "0,0",
        "cut2",
        "",
        "Section",
        "front",
        "-10,30",
        "110,30",
        "50,70",
        "",
        "No",
    ]);
    assert_eq!(view(&h, "cut2").section, Some(('B', "front".to_owned())));

    // A cut that crosses nothing the view draws.
    let stray = drawn(&mut h, 20.0, 20.0);
    let revision = h.session.drawing().revision();
    h.session.select_only(&[stray]);
    h.enter_all(&[
        "VIEWFRAME",
        "0,0",
        "cut3",
        "",
        "Section",
        "front",
        "200,-10",
        "200,70",
        "250,30",
        "",
        "No",
    ]);
    assert_eq!(
        h.log().last().unwrap(),
        "Section C's cut does not cross what view front draws."
    );
    assert_eq!(h.session.drawing().revision(), revision);
}

/// The review knows a composed sheet's structure (ADR-0041 §7, #357):
/// nothing to say of one as composed; then each fault made by hand found.
#[test]
fn the_review_finds_what_breaks_a_composed_sheet() {
    let structural = |h: &Harness| -> Vec<String> {
        crate::review::review(h.session.drawing())
            .into_iter()
            .filter(|f| !f.contains("no dimensions") && !f.contains("fills only"))
            .collect()
    };
    let mut h = sheet();
    let front = drawn(&mut h, 100.0, 60.0);
    let left = drawn(&mut h, 40.0, 60.0);
    let cut = drawn(&mut h, 60.0, 60.0);
    h.session.select_only(&[front]);
    h.enter_all(&["VIEWFRAME", "0,0", "front", "", "", "Yes"]);
    h.session.select_only(&[left]);
    h.enter_all(&["", "0,0", "left", "", "PRojected", "front", "Left", "Yes"]);
    h.session.select_only(&[cut]);
    h.enter_all(&[
        "", "0,0", "aa", "", "Section", "front", "50,-10", "50,70", "100,30", "", "No",
    ]);
    h.enter_all(&["BOMTABLE", "1, Block, 1", "", ""]);
    let f = view(&h, "front");
    let corner = f.origin + cad_core::Vec2::new(100.0, 60.0);
    h.enter_all(&[
        "BALLOON",
        "1",
        &format!("{},{}", corner.x, corner.y),
        &format!("{},{}", corner.x + 20.0, corner.y + 15.0),
        "",
    ]);
    assert_eq!(structural(&h), Vec::<String>::new());

    // A side view drawn shorter than the front: they disagree on the
    // object's height. A centre line past the outline is not the outline.
    let mut short = sheet();
    let front_s = drawn(&mut short, 100.0, 60.0);
    short.enter_all(&["LINE", "50,-5", "50,65", ""]);
    let centre = *short.ids().last().unwrap();
    short.enter_all(&["LINETYPE", "Load", "CENTER", ""]);
    short.session.select_only(&[centre]);
    short.enter_all(&["CHPROP", "LType", "CENTER", ""]);
    let left_s = drawn(&mut short, 40.0, 55.0);
    short.session.select_only(&[front_s, centre]);
    short.enter_all(&["VIEWFRAME", "0,0", "front", "", "", "Yes"]);
    short.session.select_only(&[left_s]);
    short.enter_all(&["", "0,0", "left", "", "PRojected", "front", "Left", "No"]);
    assert_eq!(
        structural(&short),
        [
            "Views left and front disagree on the object's height: 55 in left and 60 in front. A projected view shows the whole object, as tall and as wide as the view it is projected from: draw both from the same positions."
        ],
        "{:?}",
        short.log()
    );

    // The left view moved by hand, out of line.
    let members = |h: &Harness, name: &str| {
        h.session
            .drawing()
            .groups()
            .iter()
            .find(|(_, g)| g.name == name)
            .map(|(_, g)| g.members.clone())
            .unwrap()
    };
    let left_view = members(&h, "VIEW-left");
    h.session.select_only(&left_view);
    h.enter_all(&["MOVE", "0,0", "0,20"]);
    let found = structural(&h).join(" | ");
    assert!(
        found.contains("View left is out of line with view front"),
        "{found}"
    );
    h.enter("U");

    // One object of the front moved out of its frame.
    h.session.select_only(&[front]);
    h.enter_all(&["MOVE", "0,0", "0,-300"]);
    let found = structural(&h).join(" | ");
    assert!(
        found.contains("View front has objects outside its frame"),
        "{found}"
    );
    h.enter("U");

    // The section's marker erased.
    let marker = members(&h, "SECTION-A");
    h.session.select_only(&marker);
    h.enter("ERASE");
    let found = structural(&h).join(" | ");
    assert!(
        found.contains("Section A-A has no cutting line marked"),
        "{found}"
    );
    h.enter("U");

    // The bill erased under its balloon.
    let bill = members(&h, "VIEW-bom");
    h.session.select_only(&bill);
    h.enter("ERASE");
    let found = structural(&h).join(" | ");
    assert!(found.contains("A balloon names part 1"), "{found}");
    h.enter("U");

    // Two views pushed onto each other.
    let section = members(&h, "VIEW-aa");
    let (a, f) = (view(&h, "aa"), view(&h, "front"));
    h.session.select_only(&section);
    h.enter_all(&[
        "MOVE",
        &format!("{},{}", a.origin.x, a.origin.y),
        &format!("{},{}", f.origin.x + 10.0, f.origin.y + 10.0),
    ]);
    let found = structural(&h).join(" | ");
    assert!(found.contains("overlap"), "{found}");
}
