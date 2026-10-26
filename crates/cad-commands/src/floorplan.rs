//! A floor plan drawn (#279, #280, ADR-0039): a layout's walls, doors,
//! and labels as ordinary entities, in one transaction, on their own
//! layers — the building's outer face polylines, open at the way in and
//! at each window, and each floor's sides lines on `WALL-EXT` where they
//! face an exterior wall and `WALL-INT` otherwise, cut where a door or a
//! window goes through and closed by its jambs; each door's leaf and swing
//! on `DOOR`; each window's glass on `WINDOW` (#324); each room's name and
//! area centred in it on `ROOM-TEXT`. The layers are made if the drawing
//! has none of those names. Nothing is a wall or door object: the plan is
//! edited with the normal tools (DESIGN §4).

use cad_core::{EntityId, Point2};
use cad_document::{
    ArcEntity, Drawing, Entity, Justification, Layer, LayoutId, LineEntity, PolylineEntity,
    Properties, TextEntity, TransactionError,
};
use cad_layout::{Layout, Spec, doors, labels, walls, windows};

/// The layers a plan is drawn on.
pub const EXTERIOR_WALLS: &str = "WALL-EXT";
pub const INTERIOR_WALLS: &str = "WALL-INT";
pub const ROOM_TEXT: &str = "ROOM-TEXT";
pub const DOORS: &str = "DOOR";
pub const WINDOWS: &str = "WINDOW";
pub const LAYERS: [&str; 5] = [EXTERIOR_WALLS, INTERIOR_WALLS, ROOM_TEXT, DOORS, WINDOWS];

/// The labels' cap height, in metres.
const LABEL_HEIGHT: f64 = 0.25;

/// A room's area as its label says it: `12.50 m²`.
pub fn area_text(area: f64) -> String {
    format!("{area:.2} m²")
}

/// What a plan is drawn as, each entity with the index of its layer in
/// [`LAYERS`]: the terrain's origin at `origin`, `scale` drawing units to
/// the metre. The preview shows these; [`draw`] commits them.
pub fn entities(spec: &Spec, layout: &Layout, origin: Point2, scale: f64) -> Vec<(usize, Entity)> {
    let at = |p: Point2| Point2::new(origin.x + p.x * scale, origin.y + p.y * scale);
    let w = walls(spec, layout);
    // The outer face: a ring, or its pieces between the openings.
    let mut out: Vec<(usize, Entity)> = w
        .outside
        .iter()
        .map(|piece| {
            (
                0,
                PolylineEntity::new(piece.iter().map(|p| at(*p)).collect(), w.outside_closed)
                    .into(),
            )
        })
        .collect();
    for (faces, i) in [(&w.exterior, 0), (&w.interior, 1)] {
        for face in faces {
            out.push((i, LineEntity::new(at(face.start), at(face.end)).into()));
        }
    }
    for (jambs, i) in [(&w.exterior_jambs, 0), (&w.interior_jambs, 1)] {
        for (a, b) in jambs {
            out.push((i, LineEntity::new(at(*a), at(*b)).into()));
        }
    }
    for door in doors(spec, layout) {
        // The leaf, a rectangle 5 cm thick (the maintainer's ask).
        out.push((
            3,
            PolylineEntity::new(door.leaf_outline().iter().map(|p| at(*p)).collect(), true).into(),
        ));
        out.push((
            3,
            ArcEntity::new(
                at(door.hinge),
                spec.door * scale,
                door.swing.0,
                door.swing.1,
            )
            .into(),
        ));
    }
    // Each window's glass; its jambs are the exterior wall's (#324).
    for window in windows(spec, layout) {
        for (a, b) in window.glass() {
            out.push((4, LineEntity::new(at(a), at(b)).into()));
        }
    }
    let height = LABEL_HEIGHT * scale;
    for label in labels(layout) {
        let middle = at(label.at);
        for (text, below) in [(label.name.clone(), -0.25), (area_text(label.area), 1.25)] {
            let mut entity = TextEntity::new(
                Point2::new(middle.x, middle.y - below * height),
                height,
                0.0,
                text,
            );
            entity.justification = Justification::Center;
            out.push((2, entity.into()));
        }
    }
    out
}

/// Draws `layout` with the terrain's origin at `origin` and `scale`
/// drawing units to the metre, in `space`, as one transaction, its layers
/// made where the drawing has none of their names. Returns what it made.
pub fn draw(
    drawing: &mut Drawing,
    space: LayoutId,
    spec: &Spec,
    layout: &Layout,
    origin: Point2,
    scale: f64,
) -> Result<Vec<EntityId>, TransactionError> {
    let found = LAYERS.map(|name| drawing.layers().find(name));
    let mut transaction = drawing.transaction().in_space(space);
    let mut layer = [None; LAYERS.len()];
    for (i, name) in LAYERS.iter().enumerate() {
        layer[i] = Some(match found[i] {
            Some(id) => id,
            None => transaction.add_layer(Layer::named(*name))?,
        });
    }
    let mut made = Vec::new();
    for (i, entity) in entities(spec, layout, origin, scale) {
        let properties = Properties {
            layer: layer[i].expect("found or made"),
            ..Properties::default()
        };
        made.push(transaction.insert_with(entity, properties)?);
    }
    transaction.commit();
    Ok(made)
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use cad_document::{Drawing, Entity, LayoutId};
    use cad_layout::{Room, Setbacks, Spec, Terrain, solve};

    use super::*;

    fn house() -> Spec {
        Spec {
            terrain: Terrain {
                width: 15.0,
                depth: 20.0,
            },
            setbacks: Setbacks::default(),
            rooms: vec![
                Room::new("living", 24.0),
                Room::new("kitchen", 10.0),
                Room::new("bedroom", 12.0),
                Room::new("bath", 4.5),
            ],
            adjacent: vec![["living".into(), "kitchen".into()]],
            exterior_wall: 0.25,
            interior_wall: 0.15,
            door: 0.9,
            max_circulation: 0.15,
            windows: None,
            seed: 0,
        }
    }

    fn on(drawing: &Drawing, name: &str) -> Vec<Entity> {
        let layer = drawing.layers().find(name).expect("the layer");
        drawing
            .entities()
            .filter(|(id, _)| drawing.properties(*id).is_some_and(|p| p.layer == layer))
            .map(|(_, e)| e.clone())
            .collect()
    }

    #[test]
    fn a_plan_is_walls_on_their_layers_and_a_label_in_each_room_as_one_undo() {
        let spec = house();
        let layout = solve(&spec).unwrap();
        let mut drawing = Drawing::new();
        let made = draw(
            &mut drawing,
            LayoutId::MODEL,
            &spec,
            &layout,
            Point2::ORIGIN,
            1.0,
        )
        .unwrap();
        let w = walls(&spec, &layout);
        let all_doors = doors(&spec, &layout);
        let rooms = layout.rooms.len();
        assert_eq!(
            made.len(),
            1 + w.exterior.len()
                + w.interior.len()
                + w.exterior_jambs.len()
                + w.interior_jambs.len()
                + 2 * all_doors.len()
                + 2 * rooms
        );
        let outside = on(&drawing, EXTERIOR_WALLS);
        assert_eq!(outside.len(), 1 + w.exterior.len() + w.exterior_jambs.len());
        // Open at the way in: four corners and the two jambs' ends.
        assert!(matches!(&outside[0], Entity::Polyline(p) if !p.closed && p.vertices.len() == 6));
        assert_eq!(
            on(&drawing, INTERIOR_WALLS).len(),
            w.interior.len() + w.interior_jambs.len()
        );
        // A leaf and a swing for every door, the way in among them.
        let door_layer = on(&drawing, DOORS);
        assert_eq!(door_layer.len(), 2 * all_doors.len());
        assert_eq!(
            door_layer
                .iter()
                .filter(|e| matches!(e, Entity::Arc(_)))
                .count(),
            all_doors.len()
        );
        assert!(all_doors.iter().any(|d| d.rooms.1.is_none()));
        let texts: Vec<String> = on(&drawing, ROOM_TEXT)
            .into_iter()
            .filter_map(|e| match e {
                Entity::Text(t) => Some(t.content),
                _ => None,
            })
            .collect();
        for room in &layout.rooms {
            assert!(texts.contains(&room.name));
            assert!(texts.contains(&area_text(room.area())));
        }
        // One undo takes the plan away, its layers too.
        assert!(drawing.undo().is_some());
        assert_eq!(drawing.len(), 0);
        assert!(drawing.layers().find(EXTERIOR_WALLS).is_none());
    }

    #[test]
    fn a_plan_is_placed_and_scaled_and_keeps_the_drawings_own_layers() {
        let spec = house();
        let layout = solve(&spec).unwrap();
        let mut drawing = Drawing::new();
        let mut transaction = drawing.transaction();
        transaction.add_layer(Layer::named(INTERIOR_WALLS)).unwrap();
        transaction.commit();
        let layers = drawing.layers().len();
        draw(
            &mut drawing,
            LayoutId::MODEL,
            &spec,
            &layout,
            Point2::new(100.0, 50.0),
            1000.0,
        )
        .unwrap();
        // Only the four missing layers were made.
        assert_eq!(drawing.layers().len(), layers + 4);
        let outline = on(&drawing, EXTERIOR_WALLS)
            .into_iter()
            .find_map(|e| match e {
                Entity::Polyline(p) => Some(p),
                _ => None,
            })
            .unwrap();
        let f = layout.footprint;
        assert!(
            outline
                .vertices
                .contains(&Point2::new(100.0 + f.x * 1000.0, 50.0 + f.y * 1000.0))
        );
    }

    /// #324: a window in each room's outside walls when the brief gives
    /// their width — the outer face cut at each, its jambs on `WALL-EXT`,
    /// its glass on `WINDOW` — and every door leaf a rectangle 5 cm thick.
    #[test]
    fn windows_cut_the_outside_walls_and_door_leaves_are_rectangles() {
        let mut spec = house();
        spec.windows = Some(1.5);
        let layout = solve(&spec).unwrap();
        let windows = cad_layout::windows(&spec, &layout);
        assert!(!windows.is_empty());
        let mut drawing = Drawing::new();
        draw(
            &mut drawing,
            LayoutId::MODEL,
            &spec,
            &layout,
            Point2::ORIGIN,
            1.0,
        )
        .unwrap();
        let glass = on(&drawing, WINDOWS);
        assert_eq!(glass.len(), 2 * windows.len());
        for window in &windows {
            assert!(window.from.distance_to(window.to) <= 1.5 + 1e-9);
        }
        // The outer face in a piece between each pair of openings: the
        // windows and the way in.
        let pieces = on(&drawing, EXTERIOR_WALLS)
            .into_iter()
            .filter(|e| matches!(e, Entity::Polyline(p) if !p.closed))
            .count();
        assert_eq!(pieces, windows.len() + 1);
        for leaf in on(&drawing, DOORS).into_iter().filter_map(|e| match e {
            Entity::Polyline(p) => Some(p),
            _ => None,
        }) {
            assert!(leaf.closed && leaf.vertices.len() == 4);
            let v = &leaf.vertices;
            let short = v[1].distance_to(v[2]).min(v[0].distance_to(v[1]));
            assert!((short - cad_layout::LEAF_THICKNESS).abs() < 1e-9, "{short}");
        }
        // Without a width, no room asks: no windows, as before.
        assert!(cad_layout::windows(&house(), &solve(&house()).unwrap()).is_empty());
    }
}
