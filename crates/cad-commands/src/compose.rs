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
use cad_document::{Drawing, Entity, Group, Layer, LayoutId, Properties, TextEntity};
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
    for view in views {
        if !valid_id(&view.id) {
            return Err(format!(
                "\"{}\" cannot name a view: use letters, digits, - and _.",
                view.id
            ));
        }
        if view.entities.is_empty() {
            return Err(format!("View {} has nothing in it.", view.id));
        }
        let mut extent: Option<Aabb> = None;
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
            let b = entity.bounds(tables);
            extent = Some(extent.map_or(b, |e| e.union(&b)));
        }
        let extent = extent.expect("not empty");
        let relative = Aabb::from_points(
            Point2::ORIGIN + (extent.min() - view.origin),
            Point2::ORIGIN + (extent.max() - view.origin),
        );
        // The title, left-aligned under what the view holds.
        let title = TextEntity::new(
            Point2::new(
                relative.min().x,
                relative.min().y - (TITLE_SPACE + TITLE) * unit,
            ),
            TITLE * unit,
            0.0,
            view.title.clone(),
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
                view.entities
                    .iter()
                    .map(|&id| (id, drawing.entity(id).expect("read above").transformed(&by)))
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
    for (i, entities) in moved {
        let view = &views[i];
        let placed = out.iter().find(|p| p.view.id == view.id).expect("placed");
        let mut members = Vec::new();
        for (id, entity) in entities {
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
        }
        .to_json();
        group.selectable = false;
        transaction.add_group(group).map_err(|e| e.to_string())?;
    }
    transaction.commit();
    Ok(views.iter().map(|v| v.id.clone()).collect())
}
