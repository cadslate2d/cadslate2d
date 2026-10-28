//! A composed sheet's bill of materials and balloons (ADR-0041 §5, #354):
//! the parts declared — number, name, quantity, material — drawn as a
//! table in paper millimetres and placed on the sheet as a view; balloons
//! pointing at a part by its number, refused for a part the table does not
//! have.

use cad_core::{Point2, Vec2};
use cad_document::{CircleEntity, Entity, Justification, LineEntity, PolylineEntity, TextEntity};
use serde_json::{Value, json};

/// The layer the table is drawn on.
pub const TABLE_LAYER: &str = "BOM";
/// The layer balloons are drawn on.
pub const BALLOON_LAYER: &str = "BALLOONS";
/// The prefix of a balloon's group: `BALLOON-3-1`, part 3's first.
pub const BALLOON_GROUP: &str = "BALLOON-";
/// The table's row height and text, in paper millimetres; a cell's margin.
const ROW: f64 = 7.0;
const TEXT: f64 = 3.5;
const MARGIN: f64 = 2.0;
/// A balloon's radius and its number's height, in paper millimetres.
pub const BALLOON: f64 = 5.0;

/// One part of the bill of materials.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub number: u32,
    pub name: String,
    pub quantity: u32,
    pub material: String,
}

impl Part {
    /// A part as typed: `1, Shell, 2, Titanium alloy` — number, name,
    /// quantity, and the material when there is one.
    pub fn parse(text: &str) -> Result<Self, String> {
        let fields: Vec<&str> = text.split(',').map(str::trim).collect();
        let usage = || {
            "A part is its number, name, quantity, and material if any: 1, Shell, 2, Titanium alloy."
                .to_owned()
        };
        if !(3..=4).contains(&fields.len()) {
            return Err(usage());
        }
        let number: u32 = fields[0].parse().map_err(|_| usage())?;
        let quantity: u32 = fields[2].parse().map_err(|_| usage())?;
        if number == 0 {
            return Err("A part's number is greater than zero.".to_owned());
        }
        if quantity == 0 {
            return Err("A part's quantity is a whole number greater than zero.".to_owned());
        }
        if fields[1].is_empty() {
            return Err("A part has a name.".to_owned());
        }
        Ok(Self {
            number,
            name: fields[1].to_owned(),
            quantity,
            material: fields.get(3).map_or_else(String::new, |m| (*m).to_owned()),
        })
    }

    pub fn to_json(&self) -> Value {
        json!({
            "number": self.number,
            "name": self.name,
            "quantity": self.quantity,
            "material": self.material,
        })
    }

    pub fn read(value: &Value) -> Option<Self> {
        Some(Self {
            number: u32::try_from(value["number"].as_u64()?).ok()?,
            name: value["name"].as_str()?.to_owned(),
            quantity: u32::try_from(value["quantity"].as_u64()?).ok()?,
            material: value["material"].as_str().unwrap_or_default().to_owned(),
        })
    }
}

/// How wide `words` are at the table's text height, in paper millimetres.
fn width(words: &str) -> f64 {
    TextEntity::new(Point2::ORIGIN, TEXT, 0.0, words).width()
}

/// The table of `parts`, its lower left corner at the origin, sized in
/// paper millimetres times `unit`: a header — item, name, quantity,
/// material — over a row per part, by number; each column as wide as its
/// widest entry. Parts in number order, the header on top.
pub fn table(parts: &[Part], unit: f64) -> Vec<Entity> {
    let mut parts = parts.to_vec();
    parts.sort_by_key(|p| p.number);
    let header = ["ITEM", "NAME", "QTY", "MATERIAL"];
    let rows: Vec<[String; 4]> = parts
        .iter()
        .map(|p| {
            [
                p.number.to_string(),
                p.name.clone(),
                p.quantity.to_string(),
                p.material.clone(),
            ]
        })
        .collect();
    let columns: Vec<f64> = (0..4)
        .map(|c| {
            rows.iter()
                .map(|r| width(&r[c]))
                .chain([width(header[c])])
                .fold(0.0, f64::max)
                + 2.0 * MARGIN
        })
        .collect();
    let total_w: f64 = columns.iter().sum();
    let total_h = ROW * (rows.len() + 1) as f64;
    let p = |x: f64, y: f64| Point2::new(x * unit, y * unit);
    let mut out: Vec<Entity> = vec![
        PolylineEntity::new(
            vec![
                p(0.0, 0.0),
                p(total_w, 0.0),
                p(total_w, total_h),
                p(0.0, total_h),
            ],
            true,
        )
        .into(),
    ];
    for row in 1..=rows.len() {
        let y = ROW * row as f64;
        out.push(LineEntity::new(p(0.0, y), p(total_w, y)).into());
    }
    let mut x = 0.0;
    for w in &columns[..3] {
        x += w;
        out.push(LineEntity::new(p(x, 0.0), p(x, total_h)).into());
    }
    let cells = std::iter::once(header.map(str::to_owned)).chain(rows);
    for (r, cells) in cells.enumerate() {
        let baseline = total_h - ROW * (r + 1) as f64 + (ROW - TEXT) / 2.0;
        let mut left = 0.0;
        for (c, words) in cells.iter().enumerate() {
            if !words.is_empty() {
                out.push(
                    TextEntity::new(p(left + MARGIN, baseline), TEXT * unit, 0.0, words.clone())
                        .into(),
                );
            }
            left += columns[c];
        }
    }
    out
}

/// A balloon for part `number`: a leader from `on`, the point on the part,
/// with a dot there, to a circle centred at `at`, the number in it; sizes
/// in paper millimetres times `unit`.
pub fn balloon(number: u32, on: Point2, at: Point2, unit: f64) -> Vec<Entity> {
    let r = BALLOON * unit;
    let toward = at - on;
    let length = toward.x.hypot(toward.y);
    let edge = if length > r {
        at - toward * (r / length)
    } else {
        at
    };
    let mut out: Vec<Entity> = Vec::new();
    if length > r {
        out.push(LineEntity::new(on, edge).into());
    }
    out.push(CircleEntity::new(on, 0.6 * unit).into());
    out.push(CircleEntity::new(at, r).into());
    out.push(
        TextEntity::new(
            at - Vec2::new(0.0, TEXT * unit / 2.0),
            TEXT * unit,
            0.0,
            number.to_string(),
        )
        .justified(Justification::Center)
        .into(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_is_read_as_typed_and_refused_when_it_cannot_be() {
        assert_eq!(
            Part::parse("1, Shell, 2, Titanium alloy"),
            Ok(Part {
                number: 1,
                name: "Shell".into(),
                quantity: 2,
                material: "Titanium alloy".into()
            })
        );
        assert_eq!(Part::parse("3,Frame,1").unwrap().material, "");
        for bad in [
            "",
            "1, Shell",
            "0, Shell, 1",
            "1, Shell, 0",
            "x, Shell, 1",
            "1, , 1",
            "1, a, 2, b, c",
        ] {
            assert!(Part::parse(bad).is_err(), "{bad}");
        }
    }

    /// Every entry inside its cell: the table as wide as its widest entry
    /// in each column needs, a row per part and the header.
    #[test]
    fn the_table_holds_every_entry_inside_it() {
        let parts = [
            Part::parse("2, Internal structural frame, 1, Titanium alloy").unwrap(),
            Part::parse("1, Shell, 12, Ti-6Al-4V").unwrap(),
        ];
        let out = table(&parts, 3.0);
        let Entity::Polyline(frame) = &out[0] else {
            panic!()
        };
        let (w, h) = (frame.vertices[1].x, frame.vertices[2].y);
        assert!((h - 3.0 * ROW * 3.0).abs() < 1e-9);
        let texts: Vec<&TextEntity> = out
            .iter()
            .filter_map(|e| match e {
                Entity::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 4 + 4 + 4);
        for t in &texts {
            assert!(
                t.insertion.x >= 0.0 && t.insertion.x + t.width() <= w + 1e-9,
                "{}",
                t.content
            );
            assert!(t.insertion.y >= 0.0 && t.insertion.y + t.height <= h + 1e-9);
        }
        // Part 1 comes first, under the header.
        let first = texts.iter().find(|t| t.content == "1").unwrap();
        let second = texts.iter().find(|t| t.content == "2").unwrap();
        assert!(first.insertion.y > second.insertion.y);
    }
}
