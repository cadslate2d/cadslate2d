//! A floor plan drawn (#279, #280, ADR-0039): a layout's walls, doors,
//! and labels as ordinary entities, in one transaction, on their own
//! layers — the building's outer face polylines, open at the way in and
//! at each window, and each floor's sides lines on `WALL-EXT` where they
//! face an exterior wall and `WALL-INT` otherwise, cut where a door or a
//! window goes through and closed by its jambs; each door's leaf and swing
//! on `DOOR`; each window's glass on `WINDOW` (#324); each room's name and
//! area centred in it on `ROOM-TEXT`; in a room the brief gives the stair
//! to the floor above, the stair on `STAIR`, sized for the storey by
//! Blondel's rule and put where it fits (#331), the room's label moved
//! beside it. The layers — `STAIR` only for a stair — are made if the drawing
//! has none of those names. Nothing is a wall or door object: the plan is
//! edited with the normal tools (DESIGN §4).

use cad_core::{Aabb, EntityId, Point2, Vec2};
use cad_document::{
    ArcEntity, Drawing, Entity, Group, Justification, Layer, LayoutId, LineEntity, PolylineEntity,
    Properties, TextEntity, TransactionError,
};
use cad_geometry::stair::{self, Kind, Placement, Rule, Stair};
use cad_layout::{Layout, Spec, doors, labels, walls, windows};

use crate::stair::STAIRS;

/// The layers a plan is drawn on.
pub const EXTERIOR_WALLS: &str = "WALL-EXT";
pub const INTERIOR_WALLS: &str = "WALL-INT";
pub const ROOM_TEXT: &str = "ROOM-TEXT";
pub const DOORS: &str = "DOOR";
pub const WINDOWS: &str = "WINDOW";
pub const LAYERS: [&str; 6] = [
    EXTERIOR_WALLS,
    INTERIOR_WALLS,
    ROOM_TEXT,
    DOORS,
    WINDOWS,
    STAIRS,
];

/// A plan's stair: this wide, straight where the room is long enough,
/// else turning back on itself or a quarter.
pub const STAIR_WIDTH: f64 = 0.9;
const STAIR_KINDS: [Kind; 3] = [Kind::Straight, Kind::U, Kind::L];

/// The stair a plan draws for its storey.
pub fn plan_stair(spec: &Spec) -> Stair {
    Stair {
        kind: Kind::Straight,
        height: spec.storey_height,
        width: STAIR_WIDTH,
        newel: 0.15,
        rule: Rule::default(),
    }
}

/// What a room needs, across and along, to hold the plan's stair: a stair
/// turning back on itself and a door's width of passage beside it, for a
/// doorway the stair keeps clear of; the stair and the floor at its foot.
pub fn stair_room(spec: &Spec) -> Option<(f64, f64)> {
    let u = Stair {
        kind: Kind::U,
        ..plan_stair(spec)
    };
    let b = u.footprint().ok()?;
    Some((
        b.max().x - b.min().x + spec.door,
        b.max().y - b.min().y + stair::APPROACH,
    ))
}

/// The brief as the search takes it: each room that holds the stair at
/// least as wide and as long as [`stair_room`].
pub fn searched(spec: &Spec) -> Spec {
    let mut spec = spec.clone();
    if let Some((across, along)) = stair_room(&spec) {
        for room in spec.rooms.iter_mut().filter(|room| room.stair) {
            room.min_width = room.min_width.max(across);
            room.min_length = room.min_length.max(along);
        }
    }
    spec
}

/// Each room the brief gives a stair, by index, and where its stair goes:
/// against a corner, clear of the floor its doors swing over; `None` when
/// none fits.
pub fn stairs(spec: &Spec, layout: &Layout) -> Vec<(usize, Option<Placement>)> {
    let doors = doors(spec, layout);
    let stair = plan_stair(spec);
    spec.rooms
        .iter()
        .enumerate()
        .filter(|(_, room)| room.stair)
        .map(|(i, _)| {
            let inner = layout.rooms[i].inner;
            let room = Aabb::from_points(
                Point2::new(inner.x, inner.y),
                Point2::new(inner.right(), inner.top()),
            );
            let clear: Vec<Aabb> = doors
                .iter()
                .filter(|d| d.rooms.0 == i || d.rooms.1 == Some(i))
                .map(|d| {
                    // The doorway and a door's width either side of it.
                    let along = d.to - d.from;
                    let across = if along.x.abs() >= along.y.abs() {
                        Vec2::new(0.0, spec.door)
                    } else {
                        Vec2::new(spec.door, 0.0)
                    };
                    Aabb::from_points(d.from - across, d.to + across)
                })
                .collect();
            (i, stair::fit(&stair, &STAIR_KINDS, room, &clear))
        })
        .collect()
}

/// What a stair that fits nowhere would need, said: the rooms it leaves
/// out and the floor a straight stair and a U take.
pub fn stair_misfits(spec: &Spec, layout: &Layout) -> Option<String> {
    let names: Vec<&str> = stairs(spec, layout)
        .iter()
        .filter(|(_, placed)| placed.is_none())
        .map(|(i, _)| spec.rooms[*i].name.as_str())
        .collect();
    if names.is_empty() {
        return None;
    }
    let needs = |kind: Kind| {
        Stair {
            kind,
            ..plan_stair(spec)
        }
        .footprint()
        .map(|b| {
            format!(
                "{:.2} × {:.2} m",
                b.max().x - b.min().x,
                b.max().y - b.min().y + stair::APPROACH
            )
        })
        .unwrap_or_default()
    };
    Some(format!(
        "No stair fits in {}: a straight stair needs {} of floor, a U {}.",
        names.join(", "),
        needs(Kind::Straight),
        needs(Kind::U)
    ))
}

/// The largest part of `room` beside `taken`: where a room's label goes
/// clear of its stair.
fn beside(room: Aabb, taken: Aabb) -> Aabb {
    let (r, t) = (room, taken);
    let parts = [
        Aabb::from_points(r.min(), Point2::new(t.min().x, r.max().y)),
        Aabb::from_points(Point2::new(t.max().x, r.min().y), r.max()),
        Aabb::from_points(r.min(), Point2::new(r.max().x, t.min().y)),
        Aabb::from_points(Point2::new(r.min().x, t.max().y), r.max()),
    ];
    // The stair is inside the room, so each part is.
    let area = |b: &Aabb| {
        let d = b.max() - b.min();
        d.x * d.y
    };
    parts
        .iter()
        .copied()
        .max_by(|a, b| area(a).total_cmp(&area(b)))
        .unwrap_or(room)
}

/// The labels' cap height, in metres.
const LABEL_HEIGHT: f64 = 0.25;
/// About how far a label's letter advances, in cap heights: what tells a
/// label too wide for the floor it is on.
const LABEL_ADVANCE: f64 = 0.85;
/// The height of what a stair says of its risers and treads, in metres.
const STAIR_TEXT: f64 = 0.1;

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
    let mut labels = labels(layout);
    // A label turned a quarter, reading up, where it is too wide for the
    // floor beside a stair.
    let mut upright = vec![false; labels.len()];
    let stair_drawn = plan_stair(spec);
    for (i, placed) in stairs(spec, layout) {
        let Some(placed) = placed else { continue };
        let stair = Stair {
            kind: placed.kind,
            ..stair_drawn
        };
        let (Ok(mut laid), Ok(footprint), Ok(sizing)) =
            (stair.plan(), stair.footprint(), stair.sizing())
        else {
            continue;
        };
        let (sin, cos) = placed.turn.sin_cos();
        let turn =
            |q: Point2| placed.foot + Vec2::new(q.x * cos - q.y * sin, q.x * sin + q.y * cos);
        // The risers and treads written up the first flight, beside the
        // walking line, rather than beside the stair.
        laid.label.clear();
        for entity in crate::stair::entities(&laid, at(placed.foot), placed.turn, scale) {
            out.push((5, entity));
        }
        let along = turn(Point2::new(STAIR_WIDTH / 2.0 - 0.04, 0.15));
        out.push((
            5,
            TextEntity::new(
                at(along),
                STAIR_TEXT * scale,
                placed.turn + std::f64::consts::FRAC_PI_2,
                sizing.words(),
            )
            .into(),
        ));
        let inner = layout.rooms[i].inner;
        let room = Aabb::from_points(
            Point2::new(inner.x, inner.y),
            Point2::new(inner.right(), inner.top()),
        );
        let corners = footprint.corners().map(turn);
        let part = beside(room, Aabb::from_points(corners[0], corners[2]));
        labels[i].at = part.center();
        let wide = labels[i]
            .name
            .chars()
            .count()
            .max(area_text(labels[i].area).chars().count()) as f64
            * LABEL_ADVANCE
            * LABEL_HEIGHT;
        let (w, h) = (part.max().x - part.min().x, part.max().y - part.min().y);
        upright[i] = w < wide && h > w;
    }
    for (label, upright) in labels.into_iter().zip(upright) {
        let middle = at(label.at);
        // Down the label: its lines one under another.
        let (down, rotation) = if upright {
            (Vec2::new(height, 0.0), std::f64::consts::FRAC_PI_2)
        } else {
            (Vec2::new(0.0, -height), 0.0)
        };
        for (text, below) in [(label.name.clone(), -0.25), (area_text(label.area), 1.25)] {
            let mut entity = TextEntity::new(middle + down * below, height, rotation, text);
            entity.justification = Justification::Center;
            out.push((2, entity.into()));
        }
    }
    out
}

/// The prefix of the group a drawn plan is kept in: `PLAN-1`, `PLAN-2`, …
pub const PLAN_GROUP: &str = "PLAN-";

/// What a plan's group says of it: the brief and where and how it was
/// drawn, as JSON, so a later request can draw it again revised (#325).
pub fn plan_description(spec: &Spec, origin: Point2, scale: f64, alternative: u32) -> String {
    format!(
        "{{\"brief\":{},\"at\":[{},{}],\"scale\":{},\"alternative\":{alternative}}}",
        spec.to_json(),
        origin.x,
        origin.y,
        scale
    )
}

/// Draws `layout` with the terrain's origin at `origin` and `scale`
/// drawing units to the metre, in `space`, as one transaction, its layers
/// made where the drawing has none of their names, and its objects in a
/// group — `PLAN-1`, the first number free — whose description holds the
/// brief and where it was drawn, `alternative` being which plan (#325).
/// Picking one object picks that object alone. Returns what it made.
pub fn draw(
    drawing: &mut Drawing,
    space: LayoutId,
    spec: &Spec,
    layout: &Layout,
    origin: Point2,
    scale: f64,
    alternative: u32,
) -> Result<Vec<EntityId>, TransactionError> {
    let found = LAYERS.map(|name| drawing.layers().find(name));
    let taken: Vec<String> = drawing
        .groups()
        .iter()
        .map(|(_, group)| group.name.clone())
        .collect();
    let mut transaction = drawing.transaction().in_space(space);
    let drawn = entities(spec, layout, origin, scale);
    let mut layer = [None; LAYERS.len()];
    for (i, name) in LAYERS.iter().enumerate() {
        // The stair's layer only for a stair.
        if name == &STAIRS && !drawn.iter().any(|(on, _)| *on == i) {
            continue;
        }
        layer[i] = Some(match found[i] {
            Some(id) => id,
            None => transaction.add_layer(Layer::named(*name))?,
        });
    }
    let mut made = Vec::new();
    for (i, entity) in drawn {
        let properties = Properties {
            layer: layer[i].expect("found or made"),
            ..Properties::default()
        };
        made.push(transaction.insert_with(entity, properties)?);
    }
    let name = (1..)
        .map(|n| format!("{PLAN_GROUP}{n}"))
        .find(|name| !taken.contains(name))
        .expect("a number is free");
    let mut group = Group::named(name, made.clone());
    group.description = plan_description(spec, origin, scale, alternative);
    group.selectable = false;
    transaction.add_group(group)?;
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
            storey_height: 2.8,
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
            1,
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
            1,
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

    /// #325: a drawn plan is a group, PLAN-1 then PLAN-2, not picked whole,
    /// its description the brief and where it was drawn.
    #[test]
    fn a_drawn_plan_is_a_group_holding_its_brief() {
        let spec = house();
        let layout = solve(&spec).unwrap();
        let mut drawing = Drawing::new();
        for (origin, which) in [(Point2::ORIGIN, 1), (Point2::new(50.0, 0.0), 2)] {
            let made = draw(
                &mut drawing,
                LayoutId::MODEL,
                &spec,
                &layout,
                origin,
                1.0,
                which,
            )
            .unwrap();
            let (_, group) = drawing
                .groups()
                .iter()
                .find(|(_, g)| g.name == format!("PLAN-{which}"))
                .expect("the plan's group");
            assert_eq!(group.members, made);
            assert!(!group.selectable);
            assert_eq!(
                group.description,
                plan_description(&spec, origin, 1.0, which)
            );
            assert!(group.description.contains(r#""alternative":"#));
            assert!(group.description.contains(&spec.to_json()));
        }
        assert_eq!(drawing.groups().len(), 2);
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
            1,
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
