//! IMAGEATTACH, IMAGE, IMAGEADJUST, and IMAGEFRAME (ADR-0042 §6, #525),
//! with a picture as the host makes one: placed, moved, rotated, scaled,
//! mirrored, aligned, faded, framed, detached, reloaded, and purged — one
//! undo each.

use cad_core::{Point2, Vec2};
use cad_document::{Entity, ImageDefId, ImageEntity, LineEntity};
use cad_geometry::Transform;

use crate::testing::Harness;
use crate::{Dirty, HostRequest, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// The host's pictures: 16 × 12, four coloured quarters.
const QUARTERS: &[u8] = include_bytes!("../../../../test-data/images/quarters.png");
const PHOTO: &[u8] = include_bytes!("../../../../test-data/images/quarters.jpg");

/// A PNG's signature and header, `width` × `height` and in colour: all a
/// definition reads of it, and a picture of another size to reload with.
fn header_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn images(h: &Harness) -> Vec<ImageEntity> {
    h.entities()
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Image(image) => Some(image),
            _ => None,
        })
        .collect()
}

fn close(a: Vec2, b: Vec2) -> bool {
    (a - b).length() < 1e-9
}

/// IMAGEATTACH with `bytes` as `name`, placed by `answers`.
fn attach(h: &mut Harness, name: &str, bytes: &[u8], answers: &[&str]) {
    h.enter("IAT");
    assert_eq!(h.session.take_host_request(), Some(HostRequest::ImageFile));
    h.session.attach_picture(name, bytes.to_vec());
    h.enter_all(answers);
}

#[test]
fn imageattach_asks_the_host_then_places_the_picture_as_one_undo() {
    let mut h = Harness::new();
    h.enter("IMAGEATTACH");
    assert_eq!(h.session.take_host_request(), Some(HostRequest::ImageFile));
    assert_eq!(h.prompt(), "Choose the picture to attach:");
    let dirty = h
        .session
        .attach_picture("C:\\scans\\site plan.png", QUARTERS.to_vec());
    assert!(dirty.contains(Dirty::UI));
    assert_eq!(last(&h), "\"site plan\": 16 × 12 pixels, PNG.");
    assert_eq!(h.prompt(), "Specify insertion point:");
    h.enter("10,20");
    assert_eq!(h.prompt(), "Specify width <16.0000>:");
    h.enter("160");
    assert_eq!(h.prompt(), "Specify rotation angle <0>:");
    // The preview turns with the pointer and leaves the drawing alone.
    let revision = h.session.drawing().revision();
    h.move_to_world(p(10.0, 100.0));
    let preview = h.session.overlay();
    let shown = preview.into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::Image(image)) => Some(image),
        _ => None,
    });
    let shown = shown.expect("the image previewed");
    assert!(
        close(shown.across(), Vec2::new(0.0, 160.0)),
        "turned to the pointer"
    );
    assert_eq!(h.session.drawing().revision(), revision);

    h.enter("90");
    assert_eq!(
        last(&h),
        "Attached \"site plan\", 16 × 12 pixels, 160.0000 across."
    );
    let placed = &images(&h)[0];
    assert_eq!(placed.insertion, p(10.0, 20.0));
    assert!(close(placed.across(), Vec2::new(0.0, 160.0)));
    assert!(close(placed.up(), Vec2::new(-120.0, 0.0)), "square pixels");
    let defs = h.session.drawing().image_defs();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs.get(placed.def).unwrap().name, "site plan");
    h.undo();
    assert!(h.entities().is_empty());
    assert!(h.session.drawing().image_defs().is_empty(), "one undo");
}

#[test]
fn a_picture_attached_twice_is_held_once_and_previewed_as_itself() {
    let mut h = Harness::new();
    attach(&mut h, "plan.png", QUARTERS, &["0,0", "", ""]);
    assert!(
        close(images(&h)[0].across(), Vec2::new(16.0, 0.0)),
        "a unit a pixel"
    );
    h.enter("IAT");
    h.session.take_host_request();
    h.session
        .attach_picture("copy of plan.png", QUARTERS.to_vec());
    h.move_to_world(p(100.0, 0.0));
    let held = images(&h)[0].def;
    let previewed = h.session.overlay().into_iter().find_map(|item| match item {
        OverlayItem::PreviewEntity(Entity::Image(image)) => Some(image.def),
        _ => None,
    });
    assert_eq!(previewed, Some(held), "the picture the drawing holds");
    h.enter_all(&["100,0", "32", "0"]);
    assert_eq!(images(&h).len(), 2);
    assert_eq!(h.session.drawing().image_defs().len(), 1);
    assert_eq!(images(&h)[1].def, held);
    // Another picture under a taken name has a name of its own.
    attach(&mut h, "plan.jpg", PHOTO, &["200,0", "", ""]);
    let photo = images(&h)[2].def;
    assert_eq!(
        h.session.drawing().image_defs().get(photo).unwrap().name,
        "plan (2)"
    );
}

#[test]
fn imageattach_refuses_what_is_not_a_picture_and_a_width_of_nothing() {
    let mut h = Harness::new();
    h.enter("IAT");
    h.session.take_host_request();
    h.session.attach_picture("drawing.gif", b"GIF89a".to_vec());
    assert_eq!(
        h.log()[h.log().len() - 2..],
        ["That is not a PNG or JPEG image.", "*Cancel*"]
    );
    assert_eq!(h.prompt(), "Command:");
    // No picture: Enter gives up.
    h.enter("IAT");
    h.session.take_host_request();
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    // A width of nothing is asked again.
    h.enter("IAT");
    h.session.attach_picture("plan.png", QUARTERS.to_vec());
    h.enter_all(&["0,0", "0"]);
    assert_eq!(last(&h), "The width must be more than zero.");
    assert_eq!(h.prompt(), "Specify width <16.0000>:");
    h.enter_all(&["8", "0"]);
    assert_eq!(images(&h).len(), 1);
}

/// One image of the quarters, 160 across at the origin.
fn with_one() -> (Harness, ImageEntity) {
    let mut h = Harness::new();
    attach(&mut h, "plan.png", QUARTERS, &["0,0", "160", "0"]);
    h.enter_all(&["ZOOM", "E"]);
    let image = images(&h)[0].clone();
    (h, image)
}

#[test]
fn an_image_is_moved_rotated_scaled_mirrored_and_aligned_one_undo_each() {
    let (mut h, image) = with_one();
    let check = |h: &mut Harness, transform: Transform, what: &str| {
        let moved = &images(h)[0];
        let expected = image.transformed(&transform);
        assert!(
            moved.insertion.distance_to(expected.insertion) < 1e-9,
            "{what}"
        );
        assert!(close(moved.across(), expected.across()), "{what}");
        assert!(close(moved.up(), expected.up()), "{what}");
        h.undo();
        assert_eq!(images(h)[0], image, "{what}: one undo");
    };
    h.enter_all(&["MOVE", "ALL", "", "0,0", "30,-20"]);
    check(
        &mut h,
        Transform::translation(Vec2::new(30.0, -20.0)),
        "moved",
    );
    h.enter_all(&["ROTATE", "ALL", "", "0,0", "90"]);
    check(&mut h, Transform::rotation(p(0.0, 0.0), 90.0), "rotated");
    h.enter_all(&["SCALE", "ALL", "", "0,0", "2"]);
    check(
        &mut h,
        Transform::scaling(p(0.0, 0.0), 2.0).unwrap(),
        "scaled",
    );
    h.enter_all(&["MIRROR", "ALL", "", "0,0", "0,10", "Y"]);
    check(
        &mut h,
        Transform::mirror(p(0.0, 0.0), p(0.0, 10.0)).unwrap(),
        "mirrored",
    );
    // Aligned and scaled: its lower corners onto a diagonal twice as long.
    h.enter_all(&[
        "ALIGN", "ALL", "", "0,0", "100,100", "160,0", "100,420", "Y",
    ]);
    let aligned = &images(&h)[0];
    assert!(aligned.insertion.distance_to(p(100.0, 100.0)) < 1e-9);
    assert!(close(aligned.across(), Vec2::new(0.0, 320.0)), "aligned");
    assert!(
        close(aligned.up(), Vec2::new(-240.0, 0.0)),
        "square pixels kept"
    );
    h.undo();
    assert_eq!(images(&h)[0], image, "aligned: one undo");
}

#[test]
fn imageadjust_fades_the_images_selected_as_one_undo() {
    let (mut h, _) = with_one();
    attach(&mut h, "plan.png", QUARTERS, &["200,0", "160", "0"]);
    h.enter_all(&["LINE", "0,0", "400,120", ""]);
    h.enter_all(&["ZOOM", "E"]);
    h.select_box(p(-10.0, -10.0), p(410.0, 130.0));
    h.enter("IMAGEADJUST");
    assert_eq!(last(&h), "1 object is not an image.");
    assert_eq!(h.prompt(), "Enter fade value (0-100) <0>:");
    h.enter("150");
    assert_eq!(last(&h), "The fade is a whole number from 0 to 100.");
    h.enter("50.5");
    assert_eq!(h.prompt(), "Enter fade value (0-100) <0>:");
    let revision = h.session.drawing().revision();
    h.enter("50");
    assert_eq!(last(&h), "2 images faded to 50.");
    assert!(images(&h).iter().all(|image| image.fade == 50));
    h.undo();
    assert!(images(&h).iter().all(|image| image.fade == 0));
    assert_ne!(h.session.drawing().revision(), revision);
    // Selected first, and faded alike, the shared fade is the default.
    h.redo();
    h.select_box(p(-10.0, -10.0), p(410.0, 130.0));
    h.enter("IAD");
    assert_eq!(h.prompt(), "Enter fade value (0-100) <50>:");
    h.escape();
    // Nothing but a line: nothing to fade.
    h.escape();
    h.enter_all(&["IAD", "400,120", ""]);
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn imageframe_shows_or_hides_every_frame_as_one_undo() {
    let (mut h, _) = with_one();
    h.enter("IMAGEFRAME");
    assert_eq!(h.prompt(), "Enter image frame setting (0 or 1) <1>:");
    h.enter("2");
    assert_eq!(last(&h), "Enter 0 to hide image frames or 1 to show them.");
    let dirty = h.enter("0");
    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(last(&h), "Image frames are hidden.");
    assert!(!h.session.drawing().settings().image_frame);
    h.enter("IMAGEFRAME");
    assert_eq!(h.prompt(), "Enter image frame setting (0 or 1) <0>:");
    h.escape();
    h.undo();
    assert!(h.session.drawing().settings().image_frame);
}

#[test]
fn image_lists_detaches_and_reloads_a_picture_one_undo_each() {
    let (mut h, image) = with_one();
    attach(&mut h, "plan.png", QUARTERS, &["200,0", "", ""]);
    attach(&mut h, "site.jpg", PHOTO, &["0,200", "", ""]);
    h.enter("IMAGE");
    assert_eq!(h.prompt(), "Enter image option or [?/Detach/Reload] <?>:");
    h.enter("");
    let log = h.log();
    assert_eq!(
        log[log.len() - 2..],
        [
            "\"plan\": 16 × 12 pixels, PNG, 1 KB, shown by 2 images.",
            "\"site\": 16 × 12 pixels, JPEG, 1 KB, shown by 1 image.",
        ]
    );

    // Reloaded with a picture of another size: each image keeps its frame.
    h.enter_all(&["IMAGE", "R", "nothing"]);
    assert_eq!(last(&h), "There is no image \"nothing\".");
    h.enter("plan");
    assert_eq!(h.session.take_host_request(), Some(HostRequest::ImageFile));
    assert_eq!(h.prompt(), "Choose the picture to reload it with:");
    h.session.attach_picture("scan.png", header_png(32, 24));
    assert_eq!(last(&h), "Reloaded \"plan\": 32 × 24 pixels.");
    let def: ImageDefId = images(&h)[0].def;
    let reloaded = h.session.drawing().image_defs().get(def).unwrap();
    assert_eq!((reloaded.name.as_str(), reloaded.width), ("plan", 32));
    for (now, was) in images(&h)
        .iter()
        .zip([image.across(), Vec2::new(16.0, 0.0)])
    {
        assert_eq!(now.pixels, [32, 24]);
        assert!(close(now.across(), was), "its frame kept");
    }
    h.undo();
    let back = h.session.drawing().image_defs().get(def).unwrap();
    assert_eq!(back.width, 16);
    assert_eq!(images(&h)[0], image);

    // Detached: the picture and its two images.
    h.enter_all(&["IMAGE", "D", "PLAN"]);
    assert_eq!(last(&h), "Detached \"plan\" and 2 images.");
    assert_eq!(images(&h).len(), 1);
    assert_eq!(h.session.drawing().image_defs().len(), 1);
    h.undo();
    assert_eq!(images(&h).len(), 3);
    assert_eq!(h.session.drawing().image_defs().len(), 2);
}

#[test]
fn purge_sweeps_out_a_picture_no_image_shows() {
    let (mut h, _) = with_one();
    h.enter_all(&["ERASE", "ALL", ""]);
    h.enter_all(&["PURGE", "?"]);
    assert_eq!(last(&h), "Unused: 1 image (plan).");
    h.escape();
    h.enter_all(&["PURGE", "I"]);
    assert_eq!(last(&h), "Purged 1 image (plan).");
    assert!(h.session.drawing().image_defs().is_empty());
    h.undo();
    assert_eq!(h.session.drawing().image_defs().len(), 1);
}

#[test]
fn a_line_drawn_over_an_image_is_a_line_of_its_own() {
    let (mut h, _) = with_one();
    h.enter_all(&["LINE", "0,0", "160,120", ""]);
    assert!(
        h.entities()
            .iter()
            .any(|e| matches!(e, Entity::Line(LineEntity { .. })))
    );
    h.undo();
    assert_eq!(h.entities().len(), 1);
}
