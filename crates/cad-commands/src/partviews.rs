//! A part's views drawn (#339, ADR-0040): what `cad_geometry::views`
//! works out, laid on the sheet in first- or third-angle projection — the
//! top view under or over the front, the right side view left or right of
//! it, each aligned with the front — visible edges continuous, hidden ones
//! dashed, centre lines chained, a section hatched with its cutting plane
//! marked in the top view, each view titled. A part is described as JSON:
//!
//! - a prism, `{"profile": [[x, z], …], "depth": d, "holes": [{"axis":
//!   "y", "at": [x, z], "diameter": d}, …]}` — a hole along `y` at `(x,
//!   z)`, along `z` at `(x, y)`, along `x` at `(y, z)`;
//! - a turned part, `{"steps": [[diameter, length], …], "bore": d}`.

use cad_core::{Point2, Vec2};
use cad_document::{
    CircleEntity, Entity, HatchEntity, HatchStyle, Justification, LineEntity, PolylineEntity,
    TextEntity,
};
use cad_geometry::Path;
use cad_geometry::views::{Axis, Hole, Mark, Part, View, Views};
use serde_json::{Value, json};

use crate::views::LayerSpec;

pub const LAYERS: [LayerSpec; 6] = [
    LayerSpec::plain("PART-VISIBLE"),
    LayerSpec {
        name: "PART-HIDDEN",
        linetype: Some("HIDDEN"),
    },
    LayerSpec {
        name: "PART-CENTER",
        linetype: Some("CENTER"),
    },
    LayerSpec::plain("PART-HATCH"),
    LayerSpec {
        name: "PART-CUT",
        linetype: Some("CENTER"),
    },
    LayerSpec::plain("PART-TEXT"),
];
const VISIBLE: usize = 0;
const HIDDEN: usize = 1;
const CENTER: usize = 2;
const HATCH: usize = 3;
const CUT: usize = 4;
const TEXT: usize = 5;

/// The prefix of a part's group: `PART-1`, …
pub const GROUP: &str = "PART-";

/// How the views are laid out: first-angle — the top view under the
/// front, the right side on its left — or third-angle, the other way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection {
    First,
    Third,
}

impl Projection {
    pub fn name(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Third => "third",
        }
    }
}

/// A part from its JSON; why not, said.
pub fn parse(text: &str) -> Result<Part, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("The part cannot be read: {e}"))?;
    let number = |v: &Value, what: &str| {
        v.as_f64()
            .ok_or_else(|| format!("The part's {what} must be a number."))
    };
    let pair = |v: &Value, what: &str| -> Result<[f64; 2], String> {
        match v.as_array().map(Vec::as_slice) {
            Some([a, b]) => Ok([number(a, what)?, number(b, what)?]),
            _ => Err(format!("The part's {what} must be two numbers.")),
        }
    };
    if let Some(steps) = value.get("steps") {
        let steps = steps
            .as_array()
            .ok_or("The part's steps must be a list of [diameter, length].")?
            .iter()
            .map(|s| pair(s, "steps").map(|[d, l]| (d, l)))
            .collect::<Result<Vec<_>, _>>()?;
        let bore = match value.get("bore") {
            Some(v) => number(v, "bore")?,
            None => 0.0,
        };
        return Ok(Part::Turned { steps, bore });
    }
    let profile = value
        .get("profile")
        .and_then(Value::as_array)
        .ok_or("A part is a prism, with a profile and a depth, or turned, with steps.")?
        .iter()
        .map(|q| pair(q, "profile").map(|[x, z]| Point2::new(x, z)))
        .collect::<Result<Vec<_>, _>>()?;
    let depth = number(value.get("depth").unwrap_or(&Value::Null), "depth")?;
    let holes = match value.get("holes") {
        None => Vec::new(),
        Some(list) => list
            .as_array()
            .ok_or("The part's holes must be a list.")?
            .iter()
            .map(|h| {
                let axis = match h.get("axis").and_then(Value::as_str) {
                    Some("x") | Some("X") => Axis::X,
                    Some("z") | Some("Z") => Axis::Z,
                    Some("y") | Some("Y") | None => Axis::Y,
                    Some(other) => {
                        return Err(format!("A hole's axis is x, y, or z, not {other}."));
                    }
                };
                Ok(Hole {
                    axis,
                    at: pair(h.get("at").unwrap_or(&Value::Null), "hole's centre")?,
                    diameter: number(h.get("diameter").unwrap_or(&Value::Null), "hole's diameter")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    };
    Ok(Part::Prism {
        profile,
        depth,
        holes,
    })
}

/// A part as JSON, as [`parse`] reads it.
pub fn to_json(part: &Part) -> Value {
    match part {
        Part::Prism {
            profile,
            depth,
            holes,
        } => json!({
            "profile": profile.iter().map(|q| [q.x, q.y]).collect::<Vec<_>>(),
            "depth": depth,
            "holes": holes.iter().map(|h| json!({
                "axis": match h.axis { Axis::X => "x", Axis::Y => "y", Axis::Z => "z" },
                "at": h.at,
                "diameter": h.diameter,
            })).collect::<Vec<_>>(),
        }),
        Part::Turned { steps, bore } => json!({
            "steps": steps.iter().map(|&(d, l)| [d, l]).collect::<Vec<_>>(),
            "bore": bore,
        }),
    }
}

/// The part's largest size: what its gaps, texts, and dashes go by.
pub fn size(views: &Views) -> f64 {
    let f = &views.front;
    let r = &views.right;
    (f.max.x - f.min.x)
        .max(f.max.y - f.min.y)
        .max(r.max.x - r.min.x)
}

/// A linetype scale that shows a part of `size` with dashes of a few
/// hundredths of it: 1, 2, or 5 times a power of ten.
pub fn ltscale_for(size: f64) -> f64 {
    let wanted = (size / 10.0).max(1.0);
    let power = 10_f64.powf(wanted.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|k| k * power)
        .find(|v| *v >= wanted * 0.75)
        .unwrap_or(wanted)
}

/// Where each view's own coordinates go on the sheet: the front view's
/// lower left at `at`, the others `gap` from it.
struct Layout {
    front: Vec2,
    top: Vec2,
    right: Vec2,
}

fn layout(views: &Views, projection: Projection, at: Point2, gap: f64) -> Layout {
    let f = &views.front;
    let r = &views.right;
    let (fw, fh) = (f.max.x - f.min.x, f.max.y - f.min.y);
    let rw = r.max.x - r.min.x;
    let front = Vec2::new(at.x - f.min.x, at.y - f.min.y);
    // The top view shares the front's x; the side view its height.
    let top = views.top.as_ref().map_or(Vec2::new(0.0, 0.0), |t| {
        let th = t.max.y - t.min.y;
        let y = match projection {
            Projection::Third => at.y + fh + gap,
            Projection::First => at.y - gap - th,
        };
        Vec2::new(at.x - f.min.x, y - t.min.y)
    });
    let x = match projection {
        Projection::Third => at.x + fw + gap,
        Projection::First => at.x - gap - rw,
    };
    let right = Vec2::new(x - r.min.x, at.y - f.min.y);
    Layout { front, top, right }
}

fn marks(view: &View, by: Vec2, out: &mut Vec<(usize, Entity)>) {
    let place = |q: Point2| q + by;
    for (layer, list) in [(VISIBLE, &view.visible), (HIDDEN, &view.hidden)] {
        for mark in list {
            out.push((
                layer,
                match *mark {
                    Mark::Line(a, b) => LineEntity::new(place(a), place(b)).into(),
                    Mark::Circle(c, r) => CircleEntity::new(place(c), r).into(),
                },
            ));
        }
    }
    for (a, b) in &view.centre {
        out.push((CENTER, LineEntity::new(place(*a), place(*b)).into()));
    }
}

fn title(view: &View, by: Vec2, words: &str, height: f64) -> (usize, Entity) {
    let mut text = TextEntity::new(
        Point2::new((view.min.x + view.max.x) / 2.0, view.min.y - 2.5 * height) + by,
        height,
        0.0,
        words,
    );
    text.justification = Justification::Center;
    (TEXT, text.into())
}

/// What the views are drawn as, each entity with the index of its layer in
/// [`LAYERS`].
pub fn entities(views: &Views, projection: Projection, at: Point2) -> Vec<(usize, Entity)> {
    let size = size(views);
    let gap = (size * 0.3).max(10.0);
    let text = (size * 0.04).max(2.5);
    let placed = layout(views, projection, at, gap);
    let mut out = Vec::new();
    marks(&views.front, placed.front, &mut out);
    for region in &views.front.cut {
        let loops = region
            .iter()
            .map(|ring| Path {
                vertices: ring.iter().map(|q| *q + placed.front).collect(),
                bulges: vec![0.0; ring.len()],
                closed: true,
            })
            .collect();
        let hatch = HatchEntity::solid(loops).styled(HatchStyle::named(
            "ANSI31",
            0.0,
            (size / 40.0) / 3.175,
        ));
        out.push((HATCH, hatch.into()));
    }
    let sectioned = !views.front.cut.is_empty();
    out.push(title(
        &views.front,
        placed.front,
        if sectioned {
            "SECTION A-A"
        } else {
            "FRONT VIEW"
        },
        text,
    ));
    if let Some(top) = &views.top {
        marks(top, placed.top, &mut out);
        out.push(title(top, placed.top, "TOP VIEW", text));
        if let Some(y) = views.cutting_plane {
            // The cutting plane across the top view, its arrows towards
            // where the section looks — back, up the top view — and its
            // letters.
            let past = gap * 0.3;
            let (x0, x1) = (top.min.x - past, top.max.x + past);
            out.push((
                CUT,
                LineEntity::new(
                    Point2::new(x0, y) + placed.top,
                    Point2::new(x1, y) + placed.top,
                )
                .into(),
            ));
            for x in [x0, x1] {
                let base = Point2::new(x, y) + placed.top;
                let tip = base + Vec2::new(0.0, text * 2.0);
                out.push((VISIBLE, LineEntity::new(base, tip).into()));
                out.push((
                    VISIBLE,
                    PolylineEntity::new(
                        vec![
                            tip + Vec2::new(-text * 0.4, -text * 0.8),
                            tip,
                            tip + Vec2::new(text * 0.4, -text * 0.8),
                        ],
                        false,
                    )
                    .into(),
                ));
                let mut letter = TextEntity::new(tip + Vec2::new(0.0, text * 0.6), text, 0.0, "A");
                letter.justification = Justification::Center;
                out.push((TEXT, letter.into()));
            }
        }
    }
    marks(&views.right, placed.right, &mut out);
    out.push(title(&views.right, placed.right, "RIGHT SIDE VIEW", text));
    out
}

/// What a part's group says of it: the part, the projection, whether it is
/// sectioned, and where, as JSON.
pub fn description(part: &Part, projection: Projection, section: bool, at: Point2) -> String {
    json!({
        "part": to_json(part),
        "projection": projection.name(),
        "section": section,
        "at": [at.x, at.y],
    })
    .to_string()
}
