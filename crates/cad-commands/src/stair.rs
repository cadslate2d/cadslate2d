//! A stair drawn (#331): the pieces `cad_geometry::stair` lays out, in
//! metres from the stair's foot climbing along +y, placed in the drawing —
//! its foot at a point, climbing at an angle, scaled to the drawing's unit —
//! as ordinary entities on the `STAIR` layer, made if the drawing has none.

use cad_core::{EntityId, Point2, Vec2};
use cad_document::{
    ArcEntity, Drawing, Entity, Layer, LayoutId, LineEntity, PolylineEntity, Properties,
    TextEntity, TransactionError,
};
use cad_geometry::stair::{self, Piece};

/// The layer a stair is drawn on.
pub const STAIRS: &str = "STAIR";

/// The pieces of `drawing`, placed: the stair's foot at `foot`, its own
/// frame turned by `turn` (radians, counter-clockwise), `scale` drawing
/// units to the metre. A plan climbs along its frame's +y, so a plan
/// climbing at an angle is turned by the angle less a quarter; a section
/// runs along its frame's +x.
pub fn entities(drawing: &stair::Drawing, foot: Point2, turn: f64, scale: f64) -> Vec<Entity> {
    let (sin, cos) = turn.sin_cos();
    let at = |q: Point2| {
        let (x, y) = (q.x * scale, q.y * scale);
        foot + Vec2::new(x * cos - y * sin, x * sin + y * cos)
    };
    drawing
        .treads
        .iter()
        .chain(&drawing.walking)
        .chain(&drawing.cut)
        .chain(&drawing.label)
        .chain(&drawing.beyond)
        .map(|piece| match piece {
            Piece::Line(a, b) => LineEntity::new(at(*a), at(*b)).into(),
            Piece::Polyline { points, closed } => {
                PolylineEntity::new(points.iter().map(|q| at(*q)).collect(), *closed).into()
            }
            Piece::Arc {
                center,
                radius,
                start,
                end,
            } => {
                let full = (end - start) >= std::f64::consts::TAU - 1e-12;
                if full {
                    cad_document::CircleEntity::new(at(*center), radius * scale).into()
                } else {
                    ArcEntity::new(at(*center), radius * scale, start + turn, end + turn).into()
                }
            }
            Piece::Text {
                at: q,
                height,
                words,
            } => TextEntity::new(at(*q), height * scale, turn, words.clone()).into(),
        })
        .collect()
}

/// Draws `entities` on the `STAIR` layer in `space`, as one transaction.
pub fn draw(
    drawing: &mut Drawing,
    space: LayoutId,
    entities: Vec<Entity>,
) -> Result<Vec<EntityId>, TransactionError> {
    let found = drawing.layers().find(STAIRS);
    let mut transaction = drawing.transaction().in_space(space);
    let layer = match found {
        Some(id) => id,
        None => transaction.add_layer(Layer::named(STAIRS))?,
    };
    let properties = Properties {
        layer,
        ..Properties::default()
    };
    let mut made = Vec::new();
    for entity in entities {
        made.push(transaction.insert_with(entity, properties)?);
    }
    transaction.commit();
    Ok(made)
}

/// A stair's sizing as the command line says it.
pub fn describe(sizing: &stair::Sizing) -> String {
    format!(
        "{} risers of {:.1} cm and {} treads of {:.1} cm: two risers and a tread, {:.1} cm.",
        sizing.risers,
        sizing.riser * 100.0,
        sizing.treads(),
        sizing.tread * 100.0,
        sizing.step() * 100.0
    )
}

/// Why a stair was refused, in the user's words.
pub fn explain(error: stair::StairError) -> String {
    match error {
        stair::StairError::NotPositive(what) => {
            format!("The stair's {what} must be greater than zero.")
        }
        stair::StairError::Rule => {
            "No riser and tread keep Blondel's rule with these limits.".to_owned()
        }
        stair::StairError::Newel => "A helix's newel must be narrower than the stair.".to_owned(),
        stair::StairError::Across => {
            "A cut across the flights is drawn for a straight or U stair; cut an L or a helix along it."
                .to_owned()
        }
        stair::StairError::Headroom(climb) => format!(
            "A full turn climbs only {:.2} m: less than the {:.1} m of headroom under the stair above. Make the stair wider.",
            climb,
            stair::HEADROOM
        ),
    }
}
