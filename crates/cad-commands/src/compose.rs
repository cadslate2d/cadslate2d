//! Composed sheets (ADR-0041, #351): views placed on a sheet a title block
//! drew, by what they hold.
//!
//! A view is objects already drawn at its own origin — the image of the
//! object's reference point — and a title. Composing measures each view as
//! it will be drawn (its objects' bounds, dimension text and all, and its
//! title), places the views with `cad_geometry::composition::place`, and
//! moves each view's objects by a pure translation to where it went, so
//! what was measured is what is drawn: one transaction. Each view is then a
//! group `VIEW-id` whose description records how it was placed; views
//! already on the sheet are read from those groups and never moved.

use cad_core::{Aabb, EntityId, Point2};
use cad_document::{
    DimStyle, DimStyleId, Drawing, Entity, Group, HatchStyle, Layer, LayoutId, Properties,
    TextEntity,
};
use cad_geometry::Transform;
use cad_geometry::composition::{self, Angle, Looking, Placed, Role, ViewBox};
use serde_json::json;

use crate::titleblock::{self, SheetFrame};

/// The prefix of a view's group: `VIEW-front`.
pub const VIEW_GROUP: &str = "VIEW-";
/// The layer views' titles are drawn on.
pub const TITLE_LAYER: &str = "VIEW-TITLES";
/// The gap between views, in paper millimetres.
const GAP: f64 = 10.0;
/// A view's title, in paper millimetres, and the space above it.
const TITLE: f64 = 5.0;
const TITLE_SPACE: f64 = 4.0;

/// The sheet views are placed on: the title block drawn last.
#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub group: String,
    pub frame: SheetFrame,
}

impl Sheet {
    /// The sheet as the composition sees it: the border to place in, the
    /// title block to keep clear of, the gap between views.
    fn area(&self) -> composition::Sheet {
        composition::Sheet {
            area: self.frame.border,
            obstacles: vec![self.frame.block],
            gap: GAP * self.frame.units_per_mm,
            angle: if self.frame.first_angle {
                Angle::First
            } else {
                Angle::Third
            },
        }
    }
}

/// Moves what is composed on the sheet — its views, sections and their
/// marks, tables, notes, and balloons — as one toward the middle of its
/// drawing area, clear of the title block (#358); each view's record moved
/// with it. The number of views moved, or `None` when they are where they
/// should be already. One transaction.
pub fn centre(drawing: &mut Drawing, space: LayoutId) -> Result<Option<usize>, String> {
    let Some(sheet) = sheet_of(drawing) else {
        return Err("There is no sheet to centre: draw one with TITLEBLOCK first.".to_owned());
    };
    let of_sheet = |description: &str| {
        serde_json::from_str::<serde_json::Value>(description)
            .is_ok_and(|v| v["sheet"].as_str() == Some(sheet.group.as_str()))
    };
    let mut members: Vec<EntityId> = Vec::new();
    let mut views: Vec<(cad_document::GroupId, Group)> = Vec::new();
    for (id, group) in drawing.groups().iter() {
        let view = group.name.starts_with(VIEW_GROUP) && of_sheet(&group.description);
        let marks = group.name.starts_with(SECTION_GROUP) && of_sheet(&group.description);
        if view || marks || group.name.starts_with(crate::bom::BALLOON_GROUP) {
            members.extend(group.members.iter().copied());
        }
        if view {
            views.push((id, group.clone()));
        }
    }
    members.sort();
    members.dedup();
    let tables = drawing.tables();
    let boxes: Vec<Aabb> = members
        .iter()
        .filter_map(|id| drawing.entity(*id))
        .map(|e| e.bounds(tables))
        .collect();
    let Some(by) = composition::centring(&sheet.area(), &boxes) else {
        return Ok(None);
    };
    let shift = Transform::translation(by);
    let moved: Vec<(EntityId, Entity)> = members
        .iter()
        .filter_map(|id| Some((*id, drawing.entity(*id)?.transformed(&shift))))
        .collect();
    let mut transaction = drawing.transaction().in_space(space);
    for (id, entity) in moved {
        transaction.replace(id, entity).map_err(|e| e.to_string())?;
    }
    for (id, mut group) in views.iter().cloned() {
        if let Some(mut record) = ViewRecord::read(&group.description) {
            record.origin = record.origin + by;
            record.title_at = record.title_at.map(|p| p + by);
            group.description = record.to_json();
            transaction
                .replace_group(id, group)
                .map_err(|e| e.to_string())?;
        }
    }
    transaction.commit();
    Ok(Some(views.len()))
}

/// A layer the sheet's machinery is drawn on, made with the lineweight
/// its kind of line takes on a drawing sheet (#358): titles and section
/// marks 0.35 and 0.50 mm, a table's rules, notes, and balloons 0.25 mm.
pub(crate) fn sheet_layer(name: &str) -> Layer {
    let mut layer = Layer::named(name);
    let hundredths = match name {
        SECTION_LAYER => 50,
        TITLE_LAYER => 35,
        _ => 25,
    };
    layer.lineweight = cad_document::Lineweight::Hundredths(hundredths);
    layer
}

/// The sheet in `drawing`: the title block group with the highest number.
pub fn sheet_of(drawing: &Drawing) -> Option<Sheet> {
    drawing
        .groups()
        .iter()
        .filter_map(|(_, group)| {
            let number: u32 = group.name.strip_prefix(titleblock::GROUP)?.parse().ok()?;
            Some((number, group))
        })
        .max_by_key(|(number, _)| *number)
        .and_then(|(_, group)| {
            Some(Sheet {
                group: group.name.clone(),
                frame: SheetFrame::read(&group.description)?,
            })
        })
}

/// How a view was placed, as its group's description keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewRecord {
    pub id: String,
    pub title: String,
    pub role: Role,
    /// Its origin on the sheet.
    pub origin: Point2,
    /// The box it fills, from its origin.
    pub extent: Aabb,
    /// The sheet's group.
    pub sheet: String,
    /// Paper to real, for a view at a scale of its own (#352).
    pub ratio: Option<f64>,
    /// A section's letter and the view its cut crosses (#353).
    pub section: Option<(char, String)>,
    /// The parts of a bill of materials it holds (#354).
    pub parts: Vec<crate::bom::Part>,
    /// Where its title was put: how far the view has moved since is how
    /// far its title has (#357).
    pub title_at: Option<Point2>,
}

impl ViewRecord {
    pub fn to_json(&self) -> String {
        let (role, of, looking) = match &self.role {
            Role::Principal => ("principal", None, None),
            Role::Projected { of, looking } => {
                ("projected", Some(of.as_str()), Some(looking.name()))
            }
            Role::Free => ("free", None, None),
        };
        json!({
            "id": self.id,
            "title": self.title,
            "role": role,
            "of": of,
            "looking": looking,
            "origin": [self.origin.x, self.origin.y],
            "extent": [[self.extent.min().x, self.extent.min().y], [self.extent.max().x, self.extent.max().y]],
            "sheet": self.sheet,
            "ratio": self.ratio,
            "section": self.section.as_ref().map(|(letter, of)| json!({"letter": letter.to_string(), "of": of})),
            "parts": self.parts.iter().map(crate::bom::Part::to_json).collect::<Vec<_>>(),
            "title_at": self.title_at.map(|p| [p.x, p.y]),
        })
        .to_string()
    }

    pub fn read(description: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(description).ok()?;
        let point = |p: &serde_json::Value| Some(Point2::new(p[0].as_f64()?, p[1].as_f64()?));
        let role = match v["role"].as_str()? {
            "principal" => Role::Principal,
            "free" => Role::Free,
            "projected" => Role::Projected {
                of: v["of"].as_str()?.to_owned(),
                looking: Looking::parse(v["looking"].as_str()?)?,
            },
            _ => return None,
        };
        Some(Self {
            id: v["id"].as_str()?.to_owned(),
            title: v["title"].as_str()?.to_owned(),
            role,
            origin: point(&v["origin"])?,
            extent: Aabb::from_points(point(&v["extent"][0])?, point(&v["extent"][1])?),
            sheet: v["sheet"].as_str()?.to_owned(),
            ratio: v["ratio"].as_f64(),
            section: v["section"]["letter"]
                .as_str()
                .and_then(|l| l.chars().next())
                .zip(v["section"]["of"].as_str().map(str::to_owned)),
            parts: v["parts"]
                .as_array()
                .map(|parts| parts.iter().filter_map(crate::bom::Part::read).collect())
                .unwrap_or_default(),
            title_at: point(&v["title_at"]),
        })
    }
}

/// The parts the sheet's bills of materials hold.
pub fn parts_on(drawing: &Drawing, sheet: &str) -> Vec<crate::bom::Part> {
    views_on(drawing, sheet)
        .into_iter()
        .flat_map(|view| view.parts)
        .collect()
}

/// The views placed on `sheet`.
pub fn views_on(drawing: &Drawing, sheet: &str) -> Vec<ViewRecord> {
    drawing
        .groups()
        .iter()
        .filter(|(_, group)| group.name.starts_with(VIEW_GROUP))
        .filter_map(|(_, group)| ViewRecord::read(&group.description))
        .filter(|view| view.sheet == sheet)
        .collect()
}

/// A view to compose: its objects, drawn at `origin`, and how it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct NewView {
    pub id: String,
    pub title: String,
    pub role: Role,
    pub entities: Vec<EntityId>,
    pub origin: Point2,
    /// Paper to real — 2 for 2:1 — for a free view at a scale of its own;
    /// the sheet's when absent (ADR-0041 §4, #352).
    pub ratio: Option<f64>,
    /// For a section view, the cut it shows (#353).
    pub cut: Option<Cut>,
    /// For a bill of materials, its parts (#354).
    pub parts: Vec<crate::bom::Part>,
    /// Objects not in the drawing yet — a table, notes — drawn at
    /// `origin`, inserted on `fresh_layer` only if the view is placed: a
    /// view that does not fit leaves the drawing as it was.
    pub fresh: Vec<Entity>,
    pub fresh_layer: &'static str,
}

/// Where a section view's cut is: across the view `of`, from `from` to
/// `to`, its arrows toward `toward` — each in that view's own coordinates
/// (ADR-0041 §5).
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    pub of: String,
    pub from: Point2,
    pub to: Point2,
    pub toward: Point2,
}

/// The layer section markers are drawn on, in the CENTER linetype.
pub const SECTION_LAYER: &str = "SECTION-MARKS";
/// The prefix of a section marker's group: `SECTION-A`.
pub const SECTION_GROUP: &str = "SECTION-";
/// A marker's arrow, its head, and its letter, in paper millimetres.
const ARROW: f64 = 10.0;
const HEAD: f64 = 3.0;
const LETTER: f64 = 5.0;

/// The letters sections take, in order: I, O, and Q are skipped, being
/// read as figures.
fn section_letters() -> impl Iterator<Item = char> {
    ('A'..='Z').filter(|c| !matches!(c, 'I' | 'O' | 'Q'))
}

/// A cut's marker, in its view's own coordinates scaled by the view's
/// factor `k`: the cutting line, an arrow at each end looking toward
/// `toward`, and the letter beyond each arrow; sizes in paper millimetres
/// times `unit`. `None` when the cut has no length or `toward` is on it.
fn marker(cut: &Cut, k: f64, unit: f64, letter: char) -> Option<Vec<Entity>> {
    use cad_document::{LineEntity, PolylineEntity};
    let (a, b, toward) = (
        Point2::ORIGIN + (cut.from - Point2::ORIGIN) * k,
        Point2::ORIGIN + (cut.to - Point2::ORIGIN) * k,
        Point2::ORIGIN + (cut.toward - Point2::ORIGIN) * k,
    );
    let along = b - a;
    let length = (along.x * along.x + along.y * along.y).sqrt();
    if length.is_nan() || length <= 0.0 {
        return None;
    }
    let mut normal = cad_core::Vec2::new(-along.y / length, along.x / length);
    let side = (toward - a).x * normal.x + (toward - a).y * normal.y;
    if side.abs() <= 1e-9 * length {
        return None;
    }
    if side < 0.0 {
        normal = normal * -1.0;
    }
    let across = cad_core::Vec2::new(along.x / length, along.y / length);
    let mut out: Vec<Entity> = vec![LineEntity::new(a, b).into()];
    for end in [a, b] {
        let tip = end + normal * (ARROW * unit);
        out.push(LineEntity::new(end, tip).into());
        let back = tip - normal * (HEAD * unit);
        out.push(
            PolylineEntity::new(
                vec![
                    tip,
                    back + across * (HEAD * unit / 3.0),
                    back - across * (HEAD * unit / 3.0),
                ],
                true,
            )
            .into(),
        );
        let at = tip + normal * (2.0 * unit) - across * (LETTER * unit / 3.0);
        out.push(TextEntity::new(at, LETTER * unit, 0.0, letter.to_string()).into());
    }
    Some(out)
}

/// The scales a drafter gives a view, largest first, paper to real.
const STANDARD_RATIOS: [f64; 19] = [
    10.0, 5.0, 2.0, 1.0, 0.5, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005, 0.002, 0.001, 0.0005, 0.0002,
    0.0001, 0.00005, 0.00002, 0.00001,
];

/// A ratio as a drafter writes it: `2:1`, `1:5`.
pub fn ratio_text(ratio: f64) -> String {
    let plain = |v: f64| {
        let text = format!("{v:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    if ratio >= 1.0 {
        format!("{}:1", plain(ratio))
    } else {
        format!("1:{}", plain(1.0 / ratio))
    }
}

/// `entity` of a view drawn k times its true size about `origin`: its
/// geometry scaled, what is sized for paper — a text's height, a hatch's
/// pattern — as it was (ADR-0041 §4). A dimension's style is changed later.
fn at_ratio(entity: &Entity, k: f64, origin: Point2) -> Entity {
    let Some(scaling) = Transform::scaling(origin, k) else {
        return entity.clone();
    };
    let mut scaled = entity.transformed(&scaling);
    match (&mut scaled, entity) {
        (Entity::Text(t), Entity::Text(o)) => t.height = o.height,
        (Entity::MText(m), Entity::MText(o)) => {
            m.height = o.height;
            m.width = o.width;
        }
        (Entity::Hatch(h), Entity::Hatch(o)) => {
            if let (HatchStyle::Pattern { scale, .. }, HatchStyle::Pattern { scale: was, .. }) =
                (&mut h.style, &o.style)
            {
                *scale = *was;
            }
        }
        _ => {}
    }
    scaled
}

fn shifted(b: &Aabb, origin: Point2) -> Aabb {
    Aabb::from_points(
        origin + (b.min() - Point2::ORIGIN),
        origin + (b.max() - Point2::ORIGIN),
    )
}

/// Whether `id` may name a view: letters, digits, `-` and `_`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Places `views` on the drawing's sheet and moves their objects there, as
/// one transaction. The views placed, by id; or why nothing was.
pub fn compose(
    drawing: &mut Drawing,
    space: LayoutId,
    views: &[NewView],
) -> Result<Vec<String>, String> {
    let Some(sheet) = sheet_of(drawing) else {
        return Err(
            "There is no sheet to place views on: draw one with TITLEBLOCK first.".to_owned(),
        );
    };
    let unit = sheet.frame.units_per_mm;
    let placed_before = views_on(drawing, &sheet.group);
    // Each section's letter: the next the sheet has not used, in order.
    let mut used: Vec<char> = placed_before
        .iter()
        .filter_map(|v| v.section.as_ref().map(|(letter, _)| *letter))
        .collect();
    let mut letters: Vec<Option<char>> = Vec::new();
    for view in views {
        let Some(cut) = &view.cut else {
            letters.push(None);
            continue;
        };
        if view.role != Role::Free {
            return Err(format!(
                "View {} is a section: it is placed where there is room, not projected.",
                view.id
            ));
        }
        if cut.of == view.id {
            return Err(format!("Section {} cannot cut across itself.", view.id));
        }
        let Some(letter) = section_letters().find(|l| !used.contains(l)) else {
            return Err("The sheet has used every section letter.".to_owned());
        };
        used.push(letter);
        letters.push(Some(letter));
    }
    let sheet_members: Vec<EntityId> = drawing
        .groups()
        .iter()
        .find(|(_, g)| g.name == sheet.group)
        .map(|(_, g)| g.members.clone())
        .unwrap_or_default();
    let in_a_view = |id: EntityId| -> Option<String> {
        drawing
            .groups_of(id)
            .find(|(_, g)| g.name.starts_with(VIEW_GROUP))
            .map(|(_, g)| g.name.clone())
    };
    let tables = drawing.tables();
    let mut boxes = Vec::new();
    let mut titles = Vec::new();
    // Each view's objects as they will be drawn, at its ratio, before they
    // are placed; and the dimension styles a view at its own ratio needs —
    // the style each of its dimensions had, its lengths read through 1/k.
    let mut prepared: Vec<Vec<(EntityId, Entity)>> = Vec::new();
    let mut derived: Vec<(usize, DimStyleId, DimStyle)> = Vec::new();
    for (index, view) in views.iter().enumerate() {
        if !valid_id(&view.id) {
            return Err(format!(
                "\"{}\" cannot name a view: use letters, digits, - and _.",
                view.id
            ));
        }
        if view.entities.is_empty() && view.fresh.is_empty() {
            return Err(format!("View {} has nothing in it.", view.id));
        }
        let k = match view.ratio {
            None => 1.0,
            Some(_) if view.role != Role::Free => {
                return Err(format!(
                    "View {} takes the sheet's scale: only a free view has one of its own.",
                    view.id
                ));
            }
            Some(r) if r.is_finite() && r > 0.0 => r * unit,
            Some(_) => {
                return Err(format!(
                    "View {}'s scale must be greater than zero.",
                    view.id
                ));
            }
        };
        let mut extent: Option<Aabb> = view
            .fresh
            .iter()
            .map(|e| e.bounds(tables))
            .reduce(|a, b| a.union(&b));
        let mut objects = Vec::new();
        for &id in &view.entities {
            let Some(entity) = drawing.entity(id) else {
                return Err(format!(
                    "View {} names an object that is not there.",
                    view.id
                ));
            };
            if sheet_members.contains(&id) {
                return Err(format!(
                    "View {} would hold part of the sheet itself.",
                    view.id
                ));
            }
            if let Some(other) = in_a_view(id) {
                return Err(format!("View {} would take objects of {other}.", view.id));
            }
            if entity.is_unbounded() {
                return Err(format!(
                    "View {} holds a construction line, which has no end.",
                    view.id
                ));
            }
            let drawn = if view.ratio.is_none() {
                entity.clone()
            } else {
                at_ratio(entity, k, view.origin)
            };
            let b = match &drawn {
                Entity::Dimension(d) if view.ratio.is_some() => {
                    let mut style = tables.dim_style(d.style).clone();
                    // Read true through 1/k; sized for the sheet, N (§4).
                    style.linear_factor /= k;
                    style.scale = unit;
                    style.name = format!("{}-{}", view.id, style.name);
                    let b = d.layout(&style).bounds();
                    if !derived
                        .iter()
                        .any(|(v, from, _)| *v == index && *from == d.style)
                    {
                        derived.push((index, d.style, style));
                    }
                    b
                }
                _ => drawn.bounds(tables),
            };
            extent = Some(extent.map_or(b, |e| e.union(&b)));
            objects.push((id, drawn));
        }
        prepared.push(objects);
        let extent = extent.expect("not empty");
        let relative = Aabb::from_points(
            Point2::ORIGIN + (extent.min() - view.origin),
            Point2::ORIGIN + (extent.max() - view.origin),
        );
        // The title, left-aligned under what the view holds; its scale
        // after it when it has its own.
        let named = match letters[index] {
            Some(letter) => format!("SECTION {letter}-{letter}"),
            None => view.title.clone(),
        };
        // Its scale once: a title that says it already keeps its words.
        let words = match view.ratio {
            Some(r) if !named.contains(&ratio_text(r)) => format!("{named} ({})", ratio_text(r)),
            _ => named,
        };
        let title = TextEntity::new(
            Point2::new(
                relative.min().x,
                relative.min().y - (TITLE_SPACE + TITLE) * unit,
            ),
            TITLE * unit,
            0.0,
            words,
        );
        let with_title = relative.union(&Entity::Text(title.clone()).bounds(tables));
        boxes.push(ViewBox {
            id: view.id.clone(),
            role: view.role.clone(),
            extent: with_title,
        });
        titles.push(title);
    }
    // Each section's marker, in its parent's own coordinates; a parent
    // placed with it is measured with its marker.
    let mut markers: Vec<(usize, String, Vec<Entity>)> = Vec::new();
    for (i, view) in views.iter().enumerate() {
        let (Some(cut), Some(letter)) = (&view.cut, letters[i]) else {
            continue;
        };
        let parent_k = if let Some(parent) = views.iter().find(|v| v.id == cut.of) {
            parent.ratio.map_or(1.0, |r| r * unit)
        } else if let Some(parent) = placed_before.iter().find(|v| v.id == cut.of) {
            parent.ratio.map_or(1.0, |r| r * unit)
        } else {
            return Err(format!(
                "Section {letter}'s cut is across view {}, which is not on the sheet.",
                cut.of
            ));
        };
        let Some(entities) = marker(cut, parent_k, unit, letter) else {
            return Err(format!(
                "Section {letter}'s cut has no length, or its arrows no side to look toward."
            ));
        };
        if let Some(j) = views.iter().position(|v| v.id == cut.of) {
            let reach = entities
                .iter()
                .map(|e| e.bounds(tables))
                .reduce(|a, b| a.union(&b))
                .expect("a marker has a line");
            boxes[j].extent = boxes[j].extent.union(&reach);
        }
        markers.push((i, cut.of.clone(), entities));
    }
    let placed: Vec<Placed> = placed_before
        .iter()
        .map(|v| Placed {
            view: ViewBox {
                id: v.id.clone(),
                role: v.role.clone(),
                extent: v.extent,
            },
            origin: v.origin,
        })
        .collect();
    let area = sheet.area();
    let out = composition::place(&area, &placed, &boxes).map_err(|error| match error {
        composition::PlaceError::NoRoom { view, size } => {
            let (w, h) = (size.x / unit, size.y / unit);
            // A view at its own scale is as large as that scale makes it,
            // whatever the sheet's: say so, or a larger sheet scale is
            // tried and changes nothing.
            match views.iter().find(|v| v.id == view).and_then(|v| v.ratio) {
                Some(ratio) => {
                    // The largest standard scale at which it takes no more
                    // than half the sheet each way: a number to give back.
                    let border = sheet.frame.border;
                    let room = ((border.max().x - border.min().x) / unit / 2.0 / w)
                        .min((border.max().y - border.min().y) / unit / 2.0 / h);
                    let fits = ratio * room;
                    let suggestion = STANDARD_RATIOS
                        .iter()
                        .copied()
                        .find(|r| *r <= fits)
                        .map_or_else(String::new, |r| {
                            format!(" A ratio of {r} ({}) or smaller fits.", ratio_text(r))
                        });
                    format!(
                        "View {view}, at its own scale {}, needs {w:.0} × {h:.0} mm of the sheet free, and there is not that much room: its ratio alone sets its size on paper, whatever the sheet's scale — give it a smaller ratio, paper to real, or a larger sheet.{suggestion}",
                        ratio_text(ratio)
                    )
                }
                None => format!(
                    "View {view} needs {w:.0} × {h:.0} mm of the sheet free, and there is not that much room: choose a larger sheet or scale."
                ),
            }
        }
        other => {
            let mut said = other.to_string();
            if let Some(first) = said.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            format!("{said}.")
        }
    })?;

    // Moved where they go, with their titles, as one transaction.
    let taken: Vec<String> = drawing
        .groups()
        .iter()
        .map(|(_, g)| g.name.clone())
        .collect();
    for view in views {
        let name = format!("{VIEW_GROUP}{}", view.id);
        if taken.contains(&name) {
            return Err(format!("There is a view {} already.", view.id));
        }
    }
    // Each marker where its parent went; its cut must cross what the parent
    // draws — its ends may, and usually do, reach past the outline.
    let mut placed_markers: Vec<(usize, Vec<Entity>)> = Vec::new();
    for (i, of, entities) in &markers {
        let letter = letters[*i].expect("a section");
        let (origin, geometry): (Point2, Vec<Entity>) =
            if let Some(j) = views.iter().position(|v| &v.id == of) {
                let origin = out
                    .iter()
                    .find(|p| &p.view.id == of)
                    .expect("placed")
                    .origin;
                let by = Transform::translation(origin - views[j].origin);
                (
                    origin,
                    prepared[j]
                        .iter()
                        .map(|(_, e)| e.transformed(&by))
                        .collect(),
                )
            } else {
                let record = placed_before.iter().find(|v| &v.id == of).expect("checked");
                let members = drawing
                    .groups()
                    .iter()
                    .find(|(_, g)| g.name == format!("{VIEW_GROUP}{of}"))
                    .map(|(_, g)| g.members.clone())
                    .unwrap_or_default();
                (
                    record.origin,
                    members
                        .iter()
                        .filter_map(|id| drawing.entity(*id).cloned())
                        .collect(),
                )
            };
        let at = Transform::translation(origin - Point2::ORIGIN);
        let absolute: Vec<Entity> = entities.iter().map(|e| e.transformed(&at)).collect();
        let Entity::Line(cut) = &absolute[0] else {
            unreachable!("a marker begins with its line")
        };
        let line = cad_geometry::Primitive::Segment(cad_geometry::Segment {
            start: cut.start,
            end: cut.end,
        });
        let crosses = geometry
            .iter()
            .filter(|e| !matches!(e, Entity::Text(_) | Entity::MText(_) | Entity::Dimension(_)))
            .flat_map(|e| e.primitives(tables))
            .any(|p| !line.intersect(&p).as_slice().is_empty());
        if !crosses {
            return Err(format!(
                "Section {letter}'s cut does not cross what view {of} draws."
            ));
        }
        // A parent placed before: its marker must not run into another view.
        if !views.iter().any(|v| &v.id == of) {
            let reach = absolute
                .iter()
                .map(|e| e.bounds(tables))
                .reduce(|a, b| a.union(&b))
                .expect("a line");
            let others = placed_before
                .iter()
                .filter(|v| &v.id != of)
                .map(|v| (v.id.clone(), shifted(&v.extent, v.origin)))
                .chain(out.iter().map(|p| (p.view.id.clone(), p.bounds())));
            for (id, b) in others {
                if reach.min().x < b.max().x
                    && b.min().x < reach.max().x
                    && reach.min().y < b.max().y
                    && b.min().y < reach.max().y
                {
                    return Err(format!(
                        "Section {letter}'s marker on view {of} would run into view {id}."
                    ));
                }
            }
        }
        placed_markers.push((*i, absolute));
    }
    let fresh: Vec<Vec<Entity>> = views
        .iter()
        .map(|view| {
            let at = out
                .iter()
                .find(|p| p.view.id == view.id)
                .expect("placed")
                .origin;
            let by = Transform::translation(at - view.origin);
            view.fresh.iter().map(|e| e.transformed(&by)).collect()
        })
        .collect();
    let fresh_layers: Vec<(&'static str, Option<cad_document::LayerId>)> = views
        .iter()
        .filter(|v| !v.fresh.is_empty())
        .map(|v| (v.fresh_layer, drawing.layers().find(v.fresh_layer)))
        .collect();
    let moved: Vec<(usize, Vec<(EntityId, Entity)>)> = views
        .iter()
        .enumerate()
        .map(|(i, view)| {
            let at = out
                .iter()
                .find(|p| p.view.id == view.id)
                .expect("placed")
                .origin;
            let by = Transform::translation(at - view.origin);
            (
                i,
                prepared[i]
                    .iter()
                    .map(|(id, entity)| (*id, entity.transformed(&by)))
                    .collect(),
            )
        })
        .collect();
    let title_layer = drawing.layers().find(TITLE_LAYER);
    let section_layer = drawing.layers().find(SECTION_LAYER);
    let center = drawing.linetypes().find("CENTER");
    let mut transaction = drawing.transaction().in_space(space);
    let layer = match title_layer {
        Some(id) => id,
        None => transaction
            .add_layer(sheet_layer(TITLE_LAYER))
            .map_err(|e| e.to_string())?,
    };
    let marks = match (placed_markers.is_empty(), section_layer) {
        (true, _) => None,
        (false, Some(id)) => Some(id),
        (false, None) => {
            let linetype = match center {
                Some(id) => id,
                None => transaction
                    .add_linetype(
                        cad_document::LinetypeDef::built_in_named("CENTER").expect("built in"),
                    )
                    .map_err(|e| e.to_string())?,
            };
            let mut layer = sheet_layer(SECTION_LAYER);
            layer.linetype = cad_document::Linetype::Named(linetype);
            Some(transaction.add_layer(layer).map_err(|e| e.to_string())?)
        }
    };
    for (i, entities) in placed_markers {
        let letter = letters[i].expect("a section");
        let mut members = Vec::new();
        for entity in entities {
            members.push(
                transaction
                    .insert_with(
                        entity,
                        Properties {
                            layer: marks.expect("a layer for marks"),
                            ..Properties::default()
                        },
                    )
                    .map_err(|e| e.to_string())?,
            );
        }
        let mut group = Group::named(format!("{SECTION_GROUP}{letter}"), members);
        group.description = json!({
            "letter": letter.to_string(),
            "of": views[i].cut.as_ref().map(|c| c.of.clone()),
            "view": views[i].id,
            "sheet": sheet.group,
        })
        .to_string();
        group.selectable = false;
        transaction.add_group(group).map_err(|e| e.to_string())?;
    }
    let mut fresh_layer_ids: Vec<(&'static str, cad_document::LayerId)> = Vec::new();
    for (name, found) in fresh_layers {
        if fresh_layer_ids.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let id = match found {
            Some(id) => id,
            None => transaction
                .add_layer(sheet_layer(name))
                .map_err(|e| e.to_string())?,
        };
        fresh_layer_ids.push((name, id));
    }
    let mut styles: Vec<(usize, DimStyleId, DimStyleId)> = Vec::new();
    for (view, from, style) in derived {
        let made = transaction
            .add_dim_style(style)
            .map_err(|e| e.to_string())?;
        styles.push((view, from, made));
    }
    for (i, entities) in moved {
        let view = &views[i];
        let placed = out.iter().find(|p| p.view.id == view.id).expect("placed");
        let mut members = Vec::new();
        for (id, mut entity) in entities {
            if let Entity::Dimension(d) = &mut entity
                && let Some((_, _, to)) = styles
                    .iter()
                    .find(|(v, from, _)| *v == i && *from == d.style)
            {
                d.style = *to;
            }
            transaction.replace(id, entity).map_err(|e| e.to_string())?;
            members.push(id);
        }
        for entity in &fresh[i] {
            let layer = fresh_layer_ids
                .iter()
                .find(|(n, _)| *n == view.fresh_layer)
                .map(|(_, id)| *id)
                .expect("made above");
            members.push(
                transaction
                    .insert_with(
                        entity.clone(),
                        Properties {
                            layer,
                            ..Properties::default()
                        },
                    )
                    .map_err(|e| e.to_string())?,
            );
        }
        let title = titles[i].transformed(&Transform::translation(placed.origin - Point2::ORIGIN));
        let title_at = title.insertion;
        let title = transaction
            .insert_with(
                Entity::Text(title),
                Properties {
                    layer,
                    ..Properties::default()
                },
            )
            .map_err(|e| e.to_string())?;
        members.push(title);
        let mut group = Group::named(format!("{VIEW_GROUP}{}", view.id), members);
        group.description = ViewRecord {
            id: view.id.clone(),
            title: titles[i].content.clone(),
            role: view.role.clone(),
            origin: placed.origin,
            extent: placed.view.extent,
            sheet: sheet.group.clone(),
            ratio: view.ratio,
            section: letters[i].zip(view.cut.as_ref().map(|c| c.of.clone())),
            parts: view.parts.clone(),
            title_at: Some(title_at),
        }
        .to_json();
        group.selectable = false;
        transaction.add_group(group).map_err(|e| e.to_string())?;
    }
    transaction.commit();
    Ok(views.iter().map(|v| v.id.clone()).collect())
}
