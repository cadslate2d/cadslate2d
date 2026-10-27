//! An elevation drawn (#337, ADR-0040): a façade `cad_layout::elevation`
//! works out, placed in the drawing — its ground line's left end of the
//! building at a point, scaled to the drawing's unit — as ordinary lines
//! and polylines on its layers, titled under the ground line.

use cad_core::{Point2, Vec2};
use cad_document::{Entity, Justification, LineEntity, PolylineEntity, TextEntity};
use cad_layout::{Building, Elevation, Side};

pub const WALLS: &str = "ELEV-WALL";
pub const OPENINGS: &str = "ELEV-OPENING";
pub const ROOF: &str = "ELEV-ROOF";
pub const GROUND: &str = "ELEV-GROUND";
pub const TITLE: &str = "ELEV-TEXT";
pub const LAYERS: [&str; 5] = [WALLS, OPENINGS, ROOF, GROUND, TITLE];

/// The prefix of an elevation's group: `ELEV-1`, `ELEV-2`, …
pub const GROUP: &str = "ELEV-";

/// The title's height, metres.
const TITLE_HEIGHT: f64 = 0.3;

/// What `elevation` is drawn as, each entity with the index of its layer
/// in [`LAYERS`]: the building's left end on the ground at `at`, `scale`
/// drawing units to the metre.
pub fn entities(elevation: &Elevation, side: Side, at: Point2, scale: f64) -> Vec<(usize, Entity)> {
    let place = |q: Point2| at + Vec2::new(q.x * scale, q.y * scale);
    let ring = |points: &Vec<Point2>| -> Entity {
        PolylineEntity::new(points.iter().map(|q| place(*q)).collect(), true).into()
    };
    let mut out: Vec<(usize, Entity)> = Vec::new();
    out.extend(elevation.walls.iter().map(|w| (0, ring(w))));
    out.extend(elevation.openings.iter().map(|o| (1, ring(o))));
    out.extend(elevation.roof.iter().map(|r| (2, ring(r))));
    for (a, b) in &elevation.lines {
        out.push((0, LineEntity::new(place(*a), place(*b)).into()));
    }
    let (a, b) = elevation.ground;
    out.push((3, LineEntity::new(place(a), place(b)).into()));
    let mut title = TextEntity::new(
        place(Point2::new(elevation.width / 2.0, -2.5 * TITLE_HEIGHT)),
        TITLE_HEIGHT * scale,
        0.0,
        format!("{} ELEVATION", side.name().to_uppercase()),
    );
    title.justification = Justification::Center;
    out.push((4, title.into()));
    out
}

/// What an elevation's group says of it: the plan it was drawn from, the
/// side, the roof, and where, as JSON.
pub fn description(plan: &str, side: Side, at: Point2, scale: f64, building: &Building) -> String {
    serde_json::json!({
        "plan": plan,
        "side": side.name().to_lowercase(),
        "roof": building.roof,
        "roof_pitch": building.roof_pitch,
        "at": [at.x, at.y],
        "scale": scale,
    })
    .to_string()
}
