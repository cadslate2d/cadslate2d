//! A sheet's border and title block (#340, ADR-0040), laid out in paper
//! millimetres and drawn at a scale — on a layout at 1, in the model at
//! the drawing's scale — as ordinary lines and text on `TITLE`.
//!
//! The border keeps 20 mm from the sheet's left edge, for binding, and
//! 10 mm from the others (ISO 5457). The block, 180 by 32 mm (ISO 7200),
//! sits in the border's lower right corner: the title and the drawing's
//! number across the top, who drew it, the date, the scale, the sheet,
//! and the projection's symbol across the bottom, each field's name small
//! in its box's corner.

use cad_core::{Point2, Vec2};
use cad_document::{CircleEntity, Entity, LineEntity, PolylineEntity, TextEntity};
use serde_json::json;

use crate::settings::Paper;

pub const LAYER: &str = "TITLE";
/// The prefix of a title block's group: `TITLE-1`, …
pub const GROUP: &str = "TITLE-";

const LEFT_MARGIN: f64 = 20.0;
const MARGIN: f64 = 10.0;
const BLOCK_WIDTH: f64 = 180.0;
const BLOCK_HEIGHT: f64 = 32.0;
const LABEL: f64 = 2.0;
const VALUE: f64 = 3.5;
/// The narrow boxes' values: a date's ten figures fit 35 mm.
const SMALL: f64 = 2.5;
const TITLE: f64 = 5.0;

/// What a title block says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fields {
    pub title: String,
    pub number: String,
    pub drawn_by: String,
    pub date: String,
    /// `1:50`; on a layout, the views' own.
    pub scale: String,
    pub sheet: String,
    /// First-angle projection; third otherwise.
    pub first_angle: bool,
}

/// The sheet's size in millimetres, landscape or upright.
pub fn sheet(paper: Paper, landscape: bool) -> (f64, f64) {
    let (w, h) = paper.size_mm();
    if landscape { (h, w) } else { (w, h) }
}

fn rectangle(a: Point2, b: Point2) -> Vec<Point2> {
    vec![a, Point2::new(b.x, a.y), b, Point2::new(a.x, b.y)]
}

/// The sheet's border and block as entities, in paper millimetres times
/// `scale` drawing units each, the sheet's lower left corner at `at`.
pub fn entities(
    paper: Paper,
    landscape: bool,
    fields: &Fields,
    at: Point2,
    scale: f64,
) -> Vec<Entity> {
    let place = |x: f64, y: f64| at + Vec2::new(x * scale, y * scale);
    let (w, h) = sheet(paper, landscape);
    let mut out: Vec<Entity> = Vec::new();
    let poly = |points: Vec<Point2>| -> Entity { PolylineEntity::new(points, true).into() };
    // The sheet's edge, then the border.
    out.push(poly(rectangle(place(0.0, 0.0), place(w, h))));
    out.push(poly(rectangle(
        place(LEFT_MARGIN, MARGIN),
        place(w - MARGIN, h - MARGIN),
    )));
    // The block, in the border's lower right corner.
    let (x0, y0) = (w - MARGIN - BLOCK_WIDTH, MARGIN);
    let (x1, y1) = (w - MARGIN, MARGIN + BLOCK_HEIGHT);
    out.push(poly(rectangle(place(x0, y0), place(x1, y1))));
    let middle = y0 + BLOCK_HEIGHT / 2.0;
    let line = |a: (f64, f64), b: (f64, f64)| -> Entity {
        LineEntity::new(place(a.0, a.1), place(b.0, b.1)).into()
    };
    out.push(line((x0, middle), (x1, middle)));
    // Across the top: the title, the number; across the bottom: drawn by,
    // date, scale, sheet, projection.
    let split = x0 + 110.0;
    out.push(line((split, y0), (split, y1)));
    for x in [x0 + 45.0, x0 + 80.0, split + 35.0] {
        out.push(line((x, y0), (x, middle)));
    }
    let text = |x: f64, y: f64, height: f64, words: &str| -> Entity {
        TextEntity::new(place(x, y), height * scale, 0.0, words).into()
    };
    let cell = |x: f64, y: f64, label: &str, value: &str, size: f64, out: &mut Vec<Entity>| {
        out.push(text(
            x + 1.5,
            y + BLOCK_HEIGHT / 2.0 - LABEL - 1.5,
            LABEL,
            label,
        ));
        if !value.is_empty() {
            out.push(text(x + 1.5, y + 2.0, size, value));
        }
    };
    cell(x0, middle, "TITLE", &fields.title, TITLE, &mut out);
    cell(split, middle, "NUMBER", &fields.number, VALUE, &mut out);
    cell(x0, y0, "DRAWN BY", &fields.drawn_by, SMALL, &mut out);
    cell(x0 + 45.0, y0, "DATE", &fields.date, SMALL, &mut out);
    cell(x0 + 80.0, y0, "SCALE", &fields.scale, SMALL, &mut out);
    cell(split, y0, "SHEET", &fields.sheet, SMALL, &mut out);
    out.extend(symbol(split + 35.0, y0, fields.first_angle, &place, scale));
    out
}

/// The projection's symbol (ISO 128), in its box: a truncated cone seen
/// from the side, its wide end left, and seen end on — two circles — on
/// its right in first-angle projection, on its left in third.
fn symbol(
    x: f64,
    y: f64,
    first_angle: bool,
    place: &impl Fn(f64, f64) -> Point2,
    scale: f64,
) -> Vec<Entity> {
    let (cone, circles) = if first_angle {
        (x + 5.0, x + 26.0)
    } else {
        (x + 17.0, x + 9.0)
    };
    let mid = y + 6.0;
    let mut out: Vec<Entity> = vec![
        PolylineEntity::new(
            vec![
                place(cone, mid - 5.0),
                place(cone + 12.0, mid - 3.0),
                place(cone + 12.0, mid + 3.0),
                place(cone, mid + 5.0),
            ],
            true,
        )
        .into(),
    ];
    for r in [5.0, 3.0] {
        out.push(CircleEntity::new(place(circles, mid), r * scale).into());
    }
    let name = if first_angle {
        "1ST ANGLE"
    } else {
        "3RD ANGLE"
    };
    out.push(
        TextEntity::new(
            place(x + 1.5, y + BLOCK_HEIGHT / 2.0 - LABEL - 1.5),
            LABEL * scale,
            0.0,
            name,
        )
        .into(),
    );
    out
}

/// What a title block's group says of it, as JSON.
pub fn description(
    paper: Paper,
    landscape: bool,
    fields: &Fields,
    at: Point2,
    scale: f64,
) -> String {
    json!({
        "paper": paper.name(),
        "landscape": landscape,
        "title": fields.title,
        "number": fields.number,
        "drawn_by": fields.drawn_by,
        "date": fields.date,
        "scale": fields.scale,
        "sheet": fields.sheet,
        "projection": if fields.first_angle { "first" } else { "third" },
        "at": [at.x, at.y],
        "units_per_mm": scale,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extent(entities: &[Entity]) -> (Point2, Point2) {
        let points: Vec<Point2> = entities
            .iter()
            .flat_map(|e| match e {
                Entity::Polyline(p) => p.vertices.clone(),
                Entity::Line(l) => vec![l.start, l.end],
                Entity::Text(t) => vec![t.insertion],
                Entity::Circle(c) => vec![c.center],
                _ => vec![],
            })
            .collect();
        points.iter().fold(
            (
                Point2::new(f64::INFINITY, f64::INFINITY),
                Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            |(lo, hi), q| {
                (
                    Point2::new(lo.x.min(q.x), lo.y.min(q.y)),
                    Point2::new(hi.x.max(q.x), hi.y.max(q.y)),
                )
            },
        )
    }

    #[test]
    fn the_border_keeps_its_margins_and_the_block_its_corner() {
        let fields = Fields {
            title: "BRACKET".into(),
            first_angle: true,
            ..Fields::default()
        };
        let made = entities(Paper::A3, true, &fields, Point2::ORIGIN, 1.0);
        let Entity::Polyline(border) = &made[1] else {
            panic!()
        };
        assert_eq!(border.vertices[0], Point2::new(20.0, 10.0));
        assert_eq!(border.vertices[2], Point2::new(410.0, 287.0));
        let Entity::Polyline(block) = &made[2] else {
            panic!()
        };
        assert_eq!(block.vertices[0], Point2::new(230.0, 10.0));
        assert_eq!(block.vertices[2], Point2::new(410.0, 42.0));
        // Every text inside the block's boxes, at heights legible on paper.
        for e in &made {
            if let Entity::Text(t) = e {
                assert!(
                    t.insertion.x > 230.0 && t.insertion.x < 410.0,
                    "{}",
                    t.content
                );
                assert!(
                    t.insertion.y > 10.0 && t.insertion.y < 42.0,
                    "{}",
                    t.content
                );
                assert!(t.height >= 2.0 && t.height <= 5.0);
            }
        }
        // At 1:50 in the model, everything fifty times as large, from the
        // corner given.
        let at = Point2::new(1000.0, -500.0);
        let scaled = entities(Paper::A3, true, &fields, at, 50.0);
        let (lo, hi) = extent(&scaled);
        assert_eq!(lo, at);
        assert_eq!(
            hi,
            Point2::new(1000.0 + 420.0 * 50.0, -500.0 + 297.0 * 50.0)
        );
    }

    #[test]
    fn the_symbol_puts_the_end_view_right_in_first_angle_and_left_in_third() {
        let circles_x = |first_angle: bool| {
            let fields = Fields {
                first_angle,
                ..Fields::default()
            };
            entities(Paper::A4, true, &fields, Point2::ORIGIN, 1.0)
                .iter()
                .find_map(|e| match e {
                    Entity::Circle(c) => Some(c.center.x),
                    _ => None,
                })
                .unwrap()
        };
        let cone_x = 297.0 - 10.0 - 180.0 + 110.0 + 35.0;
        assert!(circles_x(true) > cone_x + 17.0);
        assert!(circles_x(false) < cone_x + 17.0);
    }
}
