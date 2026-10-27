//! A plan as drawn, kept with the drawing (ADR-0040 §3): its layout and the
//! doors and windows drafted from it, so what is drawn from the plan later
//! — a façade, a section — is drawn from the plan the user has, and never
//! from its brief laid out again by another version of the search, or on
//! another platform (ADR-0039 promises determinism within one build on
//! one platform only).
//!
//! As JSON, versioned: a snapshot of another version is refused, never
//! read as something it is not.

use cad_core::{Point2, Vec2};
use serde_json::{Value, json};

use crate::{Door, Layout, PlacedRoom, Rect, Spec, Window, doors, windows};

/// The snapshot's format.
pub const SNAPSHOT_VERSION: u64 = 1;

/// A plan as drawn: its layout, its doors, its windows.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub layout: Layout,
    pub doors: Vec<Door>,
    pub windows: Vec<Window>,
}

fn point(p: Point2) -> Value {
    json!([p.x, p.y])
}

fn rect(r: Rect) -> Value {
    json!([r.x, r.y, r.w, r.h])
}

fn read_point(v: &Value) -> Option<Point2> {
    Some(Point2::new(v.get(0)?.as_f64()?, v.get(1)?.as_f64()?))
}

fn read_rect(v: &Value) -> Option<Rect> {
    Some(Rect::new(
        v.get(0)?.as_f64()?,
        v.get(1)?.as_f64()?,
        v.get(2)?.as_f64()?,
        v.get(3)?.as_f64()?,
    ))
}

fn read_index(v: &Value) -> Option<usize> {
    usize::try_from(v.as_u64()?).ok()
}

impl Resolved {
    /// The plan `layout` of `spec`, its doors and windows drafted now.
    pub fn of(spec: &Spec, layout: &Layout) -> Self {
        Self {
            layout: layout.clone(),
            doors: doors(spec, layout),
            windows: windows(spec, layout),
        }
    }

    pub fn to_json(&self) -> Value {
        let l = &self.layout;
        json!({
            "version": SNAPSHOT_VERSION,
            "footprint": rect(l.footprint),
            "outline": rect(l.outline),
            "rooms": l.rooms.iter().map(|r| json!({
                "name": r.name, "rect": rect(r.rect), "inner": rect(r.inner),
            })).collect::<Vec<_>>(),
            "circulation": l.circulation,
            "entrance": l.entrance,
            "pairs": l.doors.iter().map(|&(a, b)| [a, b]).collect::<Vec<_>>(),
            "energy": l.energy,
            "seed": l.seed,
            "doors": self.doors.iter().map(|d| json!({
                "rooms": [d.rooms.0, d.rooms.1],
                "from": point(d.from), "to": point(d.to),
                "thickness": d.thickness, "into": d.into,
                "hinge": point(d.hinge), "leaf": point(d.leaf),
                "swing": [d.swing.0, d.swing.1],
                "along": [d.along.x, d.along.y],
            })).collect::<Vec<_>>(),
            "windows": self.windows.iter().map(|w| json!({
                "room": w.room, "from": point(w.from), "to": point(w.to), "thickness": w.thickness,
            })).collect::<Vec<_>>(),
        })
    }

    /// The snapshot `value` holds; `None` when it is not one, or of
    /// another version.
    pub fn from_json(value: &Value) -> Option<Self> {
        if value.get("version")?.as_u64()? != SNAPSHOT_VERSION {
            return None;
        }
        let list = |key: &str| value.get(key).and_then(Value::as_array);
        let rooms = list("rooms")?
            .iter()
            .map(|r| {
                Some(PlacedRoom {
                    name: r.get("name")?.as_str()?.to_owned(),
                    rect: read_rect(r.get("rect")?)?,
                    inner: read_rect(r.get("inner")?)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let pairs = list("pairs")?
            .iter()
            .map(|p| Some((read_index(p.get(0)?)?, read_index(p.get(1)?)?)))
            .collect::<Option<Vec<_>>>()?;
        let layout = Layout {
            footprint: read_rect(value.get("footprint")?)?,
            outline: read_rect(value.get("outline")?)?,
            rooms,
            circulation: match value.get("circulation")? {
                Value::Null => None,
                v => Some(read_index(v)?),
            },
            entrance: read_index(value.get("entrance")?)?,
            doors: pairs,
            energy: value.get("energy")?.as_f64()?,
            seed: value.get("seed")?.as_u64()?,
        };
        let doors = list("doors")?
            .iter()
            .map(|d| {
                let rooms = d.get("rooms")?;
                let swing = d.get("swing")?;
                let along = d.get("along")?;
                Some(Door {
                    rooms: (
                        read_index(rooms.get(0)?)?,
                        match rooms.get(1)? {
                            Value::Null => None,
                            v => Some(read_index(v)?),
                        },
                    ),
                    from: read_point(d.get("from")?)?,
                    to: read_point(d.get("to")?)?,
                    thickness: d.get("thickness")?.as_f64()?,
                    into: read_index(d.get("into")?)?,
                    hinge: read_point(d.get("hinge")?)?,
                    leaf: read_point(d.get("leaf")?)?,
                    swing: (swing.get(0)?.as_f64()?, swing.get(1)?.as_f64()?),
                    along: Vec2::new(along.get(0)?.as_f64()?, along.get(1)?.as_f64()?),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let windows = list("windows")?
            .iter()
            .map(|w| {
                Some(Window {
                    room: read_index(w.get("room")?)?,
                    from: read_point(w.get("from")?)?,
                    to: read_point(w.get("to")?)?,
                    thickness: w.get("thickness")?.as_f64()?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            layout,
            doors,
            windows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Room, Setbacks, Terrain, solve};

    #[test]
    fn a_plan_comes_back_from_its_snapshot_exactly() {
        let spec = Spec {
            windows: Some(1.2),
            ..Spec {
                terrain: Terrain {
                    width: 15.0,
                    depth: 20.0,
                },
                setbacks: Setbacks::default(),
                rooms: vec![
                    Room {
                        entrance: true,
                        ..Room::new("living", 24.0)
                    },
                    Room::new("kitchen", 10.0),
                    Room::new("bedroom", 12.0),
                ],
                adjacent: vec![["living".into(), "kitchen".into()]],
                exterior_wall: 0.25,
                interior_wall: 0.15,
                door: 0.9,
                max_circulation: 0.15,
                windows: None,
                storey_height: 2.8,
                building: crate::Building::default(),
                seed: 0,
            }
        };
        let resolved = Resolved::of(&spec, &solve(&spec).unwrap());
        assert!(!resolved.doors.is_empty() && !resolved.windows.is_empty());
        // Through text, as a group's description keeps it: bit for bit.
        let text = resolved.to_json().to_string();
        let back = Resolved::from_json(&serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(back, resolved);
        // Another version, or something else, is not read.
        let mut other = resolved.to_json();
        other["version"] = json!(SNAPSHOT_VERSION + 1);
        assert_eq!(Resolved::from_json(&other), None);
        assert_eq!(Resolved::from_json(&json!({"rooms": []})), None);
    }
}
