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
        })
    }
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
}

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
        if view.entities.is_empty() {
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
        let mut extent: Option<Aabb> = None;
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
        let words = match view.ratio {
            Some(r) => format!("{} ({})", view.title, ratio_text(r)),
            None => view.title.clone(),
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
    let area = composition::Sheet {
        area: sheet.frame.border,
        obstacles: vec![sheet.frame.block],
        gap: GAP * unit,
        angle: if sheet.frame.first_angle {
            Angle::First
        } else {
            Angle::Third
        },
    };
    let out = composition::place(&area, &placed, &boxes).map_err(|error| match error {
        composition::PlaceError::NoRoom { view, size } => format!(
            "View {view} needs {:.0} × {:.0} mm of the sheet free, and there is not that much room: choose a larger sheet or scale.",
            size.x / unit,
            size.y / unit
        ),
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
    let mut transaction = drawing.transaction().in_space(space);
    let layer = match title_layer {
        Some(id) => id,
        None => transaction
            .add_layer(Layer::named(TITLE_LAYER))
            .map_err(|e| e.to_string())?,
    };
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
        let title = titles[i].transformed(&Transform::translation(placed.origin - Point2::ORIGIN));
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
            title: view.title.clone(),
            role: view.role.clone(),
            origin: placed.origin,
            extent: placed.view.extent,
            sheet: sheet.group.clone(),
            ratio: view.ratio,
        }
        .to_json();
        group.selectable = false;
        transaction.add_group(group).map_err(|e| e.to_string())?;
    }
    transaction.commit();
    Ok(views.iter().map(|v| v.id.clone()).collect())
}
