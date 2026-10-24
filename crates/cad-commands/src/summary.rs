//! What the Properties panel shows about the selection (#141): how many
//! objects of which kinds, the properties they share, and — for one object
//! — its defining geometry. Computed here, in Rust, and bounded in size:
//! a selection of a hundred thousand objects is four counts and four
//! values, never a hundred thousand records (ADR-0002 §5).

use std::f64::consts::TAU;

use cad_core::Point2;
use cad_document::{Drawing, Entity, Properties};

use crate::Selection;

/// The kinds an entity can be, in the order they are listed.
const KINDS: [(&str, &str); 19] = [
    ("line", "lines"),
    ("circle", "circles"),
    ("arc", "arcs"),
    ("ellipse", "ellipses"),
    ("spline", "splines"),
    ("polyline", "polylines"),
    ("text", "texts"),
    ("multiline text", "multiline texts"),
    ("attribute definition", "attribute definitions"),
    ("block reference", "block references"),
    ("dimension", "dimensions"),
    ("hatch", "hatches"),
    ("viewport", "viewports"),
    ("point", "points"),
    ("leader", "leaders"),
    ("construction line", "construction lines"),
    ("ray", "rays"),
    ("wipeout", "wipeouts"),
    ("multileader", "multileaders"),
];

/// The kind named by `name` — singular or plural, any case — if any.
pub(crate) fn kind_named(name: &str) -> Option<usize> {
    let name = name.trim();
    KINDS
        .iter()
        .position(|(one, many)| one.eq_ignore_ascii_case(name) || many.eq_ignore_ascii_case(name))
}

/// The kinds, singular, for a message that lists them.
pub(crate) fn kind_names() -> Vec<&'static str> {
    KINDS.iter().map(|(one, _)| *one).collect()
}

pub(crate) fn kind_index(entity: &Entity) -> usize {
    match entity {
        Entity::Line(_) => 0,
        Entity::Circle(_) => 1,
        Entity::Arc(_) => 2,
        Entity::Ellipse(_) => 3,
        Entity::Spline(_) => 4,
        Entity::Polyline(_) => 5,
        Entity::Text(_) => 6,
        Entity::MText(_) => 7,
        Entity::AttributeDefinition(_) => 8,
        Entity::BlockRef(_) => 9,
        Entity::Dimension(_) => 10,
        Entity::Hatch(_) => 11,
        Entity::Viewport(_) => 12,
        Entity::Point(_) => 13,
        Entity::Leader(_) => 14,
        Entity::XLine(_) => 15,
        Entity::Ray(_) => 16,
        Entity::Wipeout(_) => 17,
        Entity::MLeader(_) => 18,
    }
}

/// The selection as the Properties panel shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionSummary {
    /// Objects selected.
    pub count: usize,
    /// How many of each kind present: `("line", 3)`, `("circles", 2)`,
    /// in a fixed order, the word already plural where it should be.
    pub kinds: Vec<(String, usize)>,
    /// The layer every selected object is on, or `None` when they differ.
    pub layer: Option<String>,
    /// The colour they all have — `ByLayer`, `red`, `42` — or `None`.
    pub color: Option<String>,
    /// The linetype they all have, by name, or `None`.
    pub linetype: Option<String>,
    /// The lineweight they all have — `ByLayer`, `0.25 mm` — or `None`.
    pub lineweight: Option<String>,
    /// For one object, what defines it, as label and value, read only.
    pub geometry: Vec<(String, String)>,
    /// How the objects were selected — "window", "fence", "quick select" —
    /// or empty (#185).
    pub method: String,
    /// The groups the selected objects are in, by name, in the order they
    /// were made (#214).
    pub groups: Vec<String>,
}

impl SelectionSummary {
    /// Summarises `selection` against `drawing`. Selected IDs that are not
    /// in the drawing — there should be none — are passed over.
    pub fn of(drawing: &Drawing, selection: &Selection) -> Self {
        let mut counts = [0usize; KINDS.len()];
        let mut common: Option<Common> = None;
        let mut only: Option<&Entity> = None;
        let mut count = 0;
        let mut groups = std::collections::BTreeMap::new();
        for id in selection.iter() {
            let Some(entity) = drawing.entity(id) else {
                continue;
            };
            for (group_id, group) in drawing.groups_of(id) {
                groups.entry(group_id).or_insert_with(|| group.name.clone());
            }
            let properties = drawing.properties(id).unwrap_or_default();
            counts[kind_index(entity)] += 1;
            count += 1;
            only = if count == 1 { Some(entity) } else { None };
            common = Some(match common {
                None => Common::of(drawing, properties),
                Some(so_far) => so_far.narrowed(Common::of(drawing, properties)),
            });
        }
        let kinds = KINDS
            .iter()
            .zip(counts)
            .filter(|(_, count)| *count > 0)
            .map(|((one, many), count)| ((if count == 1 { *one } else { *many }).to_owned(), count))
            .collect();
        let common = common.unwrap_or_default();
        Self {
            count,
            kinds,
            layer: common.layer,
            color: common.color,
            linetype: common.linetype,
            lineweight: common.lineweight,
            geometry: only
                .map(|entity| geometry_of(entity, drawing))
                .unwrap_or_default(),
            method: selection.method().describe().to_owned(),
            groups: groups.into_values().collect(),
        }
    }

    /// "3 objects: 2 lines, 1 circle", or "No selection".
    pub fn headline(&self) -> String {
        if self.count == 0 {
            return "No selection".to_owned();
        }
        let kinds: Vec<String> = self
            .kinds
            .iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();
        if self.kinds.len() == 1 {
            return kinds[0].clone();
        }
        format!("{} objects: {}", self.count, kinds.join(", "))
    }
}

/// The properties a set of objects shares, each `None` once they differ.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Common {
    layer: Option<String>,
    color: Option<String>,
    linetype: Option<String>,
    lineweight: Option<String>,
}

impl Common {
    fn of(drawing: &Drawing, properties: Properties) -> Self {
        Self {
            layer: drawing
                .layers()
                .get(properties.layer)
                .map(|layer| layer.name.clone()),
            color: Some(properties.color.describe()),
            linetype: Some(properties.linetype.describe(drawing.linetypes())),
            lineweight: Some(properties.lineweight.describe()),
        }
    }

    fn narrowed(self, other: Self) -> Self {
        let same = |a: Option<String>, b: Option<String>| if a == b { a } else { None };
        Self {
            layer: same(self.layer, other.layer),
            color: same(self.color, other.color),
            linetype: same(self.linetype, other.linetype),
            lineweight: same(self.lineweight, other.lineweight),
        }
    }
}

/// Four decimals, like the status bar's readout.
pub(crate) fn number(value: f64) -> String {
    format!("{value:.4}")
}

pub(crate) fn point(point: Point2) -> String {
    format!("{}, {}", number(point.x), number(point.y))
}

pub(crate) fn degrees(radians: f64) -> String {
    format!("{}°", number(radians.to_degrees()))
}

/// What defines one entity, as the panel lists it.
pub(crate) fn geometry_of(entity: &Entity, drawing: &Drawing) -> Vec<(String, String)> {
    let row = |label: &str, value: String| (label.to_owned(), value);
    match entity {
        Entity::Line(line) => {
            let delta = line.end - line.start;
            vec![
                row("Start", point(line.start)),
                row("End", point(line.end)),
                row("Length", number(delta.length())),
                row("Angle", degrees(delta.y.atan2(delta.x).rem_euclid(TAU))),
            ]
        }
        Entity::Circle(circle) => vec![
            row("Centre", point(circle.center)),
            row("Radius", number(circle.radius)),
            row("Diameter", number(2.0 * circle.radius)),
            row("Circumference", number(TAU * circle.radius)),
        ],
        Entity::Arc(arc) => {
            let geometry = arc.arc();
            vec![
                row("Centre", point(arc.center)),
                row("Radius", number(arc.radius)),
                row("Start angle", degrees(arc.start_angle)),
                row("End angle", degrees(arc.end_angle)),
                row("Arc length", number(geometry.radius * geometry.sweep)),
            ]
        }
        Entity::Ellipse(entity) => {
            let ellipse = entity.ellipse();
            let major = ellipse.major.length();
            let mut rows = vec![
                row("Centre", point(entity.center)),
                row("Major axis", number(2.0 * major)),
                row("Minor axis", number(2.0 * major * entity.ratio)),
                row("Ratio", number(entity.ratio)),
                row(
                    "Rotation",
                    degrees(ellipse.major.y.atan2(ellipse.major.x).rem_euclid(TAU)),
                ),
            ];
            if !ellipse.is_full() {
                rows.push(row("Start parameter", degrees(ellipse.start)));
                rows.push(row("End parameter", degrees(ellipse.end())));
            }
            rows
        }
        Entity::Spline(spline) => {
            let length: f64 = spline
                .spline()
                .segments()
                .iter()
                .map(|segment| segment.length())
                .sum();
            vec![
                row("Degree", spline.degree.to_string()),
                row("Control points", spline.control.len().to_string()),
                row("Fit points", spline.fit.len().to_string()),
                row(
                    "Closed",
                    (if spline.is_closed() { "Yes" } else { "No" }).to_owned(),
                ),
                row("Length", number(length)),
            ]
        }
        Entity::Polyline(polyline) => {
            let length: f64 = polyline
                .primitives()
                .map(|primitive| match primitive {
                    cad_geometry::Primitive::Segment(segment) => segment.length(),
                    cad_geometry::Primitive::Arc(arc) => arc.radius * arc.sweep,
                    cad_geometry::Primitive::Circle(circle) => TAU * circle.radius,
                })
                .sum();
            let mut rows = vec![
                row("Vertices", polyline.vertices.len().to_string()),
                row(
                    "Closed",
                    (if polyline.closed { "Yes" } else { "No" }).to_owned(),
                ),
                row("Length", number(length)),
            ];
            if polyline.width() > 0.0 {
                rows.push(row("Width", number(polyline.width())));
            }
            rows
        }
        Entity::Text(text) => vec![
            row("Contents", text.content.clone()),
            row("Insertion", point(text.insertion)),
            row("Height", number(text.height)),
            row("Rotation", degrees(text.rotation)),
            row("Justification", text.justification.label().to_owned()),
            row("Width", number(text.width())),
        ],
        Entity::MText(text) => vec![
            row("Contents", text.content.replace('\n', " ⏎ ")),
            row("Insertion", point(text.insertion)),
            row("Height", number(text.height)),
            row("Width", number(text.width)),
            row("Rotation", degrees(text.rotation)),
            row("Lines", text.lines().len().to_string()),
        ],
        Entity::Viewport(viewport) => vec![
            row("Centre", point(viewport.center)),
            row("Width", number(viewport.width)),
            row("Height", number(viewport.height)),
            row("View centre", point(viewport.view_center)),
            row("Scale", format!("1:{}", number(1.0 / viewport.scale))),
            row("On", if viewport.on { "yes" } else { "no" }.to_owned()),
        ],
        Entity::Point(place) => vec![row("Position", point(place.position))],
        Entity::Wipeout(wipeout) => vec![
            row("Vertices", wipeout.vertices.len().to_string()),
            row(
                "Area",
                number(cad_geometry::signed_area(&wipeout.vertices).abs()),
            ),
            row(
                "Frame",
                if drawing.settings().wipeout_frame {
                    "shown"
                } else {
                    "hidden"
                }
                .to_owned(),
            ),
        ],
        Entity::XLine(line) => vec![
            row("Base point", point(line.base)),
            row(
                "Angle",
                degrees(line.direction.y.atan2(line.direction.x).rem_euclid(TAU)),
            ),
        ],
        Entity::Ray(ray) => vec![
            row("Start", point(ray.start)),
            row(
                "Angle",
                degrees(ray.direction.y.atan2(ray.direction.x).rem_euclid(TAU)),
            ),
        ],
        Entity::MLeader(mleader) => {
            let style = drawing.tables().mleader_style(mleader.style);
            vec![
                row("Leaders", mleader.leaders.len().to_string()),
                row("Landing", point(mleader.landing)),
                row(
                    "Note",
                    match &mleader.content {
                        cad_document::MLeaderContent::Text(text) => text.clone(),
                        cad_document::MLeaderContent::Block { block, .. } => drawing
                            .blocks()
                            .get(*block)
                            .map_or_else(|| "(block)".to_owned(), |b| format!("block {}", b.name)),
                        cad_document::MLeaderContent::None => "(none)".to_owned(),
                    },
                ),
                row("Style", style.name.clone()),
            ]
        }
        Entity::Leader(leader) => {
            let style = drawing.tables().dim_style(leader.style);
            vec![
                row("Vertices", leader.vertices.len().to_string()),
                row("Start", point(leader.vertices[0])),
                row(
                    "Text",
                    leader.text.clone().unwrap_or_else(|| "(none)".to_owned()),
                ),
                row("Style", style.name.clone()),
            ]
        }
        Entity::Hatch(hatch) => {
            let mut rows = vec![row("Pattern", hatch.style.name().to_owned())];
            if let cad_document::HatchStyle::Pattern { angle, scale, .. } = &hatch.style {
                rows.push(row("Angle", degrees(*angle)));
                rows.push(row("Scale", number(*scale)));
            }
            rows.push(row("Loops", hatch.loops.len().to_string()));
            rows.push(row("Area", number(hatch.area())));
            rows
        }
        Entity::Dimension(dimension) => {
            let style = drawing.tables().dim_style(dimension.style);
            let value = dimension.value();
            vec![
                row("Kind", {
                    let mut kind = dimension.measure.kind().to_owned();
                    kind[..1].make_ascii_uppercase();
                    kind
                }),
                row(
                    "Value",
                    if matches!(dimension.measure, cad_document::Measure::Angular { .. }) {
                        degrees(value.to_radians())
                    } else {
                        number(value)
                    },
                ),
                row("Text", dimension.content(style)),
                row("Style", style.name.clone()),
            ]
        }
        Entity::AttributeDefinition(definition) => vec![
            row("Tag", definition.tag.clone()),
            row("Prompt", definition.prompt.clone()),
            row("Default", definition.text.content.clone()),
            row("Insertion", point(definition.text.insertion)),
            row("Height", number(definition.text.height)),
        ],
        Entity::BlockRef(reference) => vec![
            row(
                "Block",
                drawing
                    .blocks()
                    .get(reference.block)
                    .map_or_else(|| reference.block.to_string(), |block| block.name.clone()),
            ),
            row("Insertion", point(reference.insertion)),
            row("Scale", number(reference.scale)),
            row("Rotation", degrees(reference.rotation)),
            row(
                "Mirrored",
                (if reference.mirrored { "Yes" } else { "No" }).to_owned(),
            ),
        ]
        .into_iter()
        .chain(
            drawing
                .blocks()
                .get(reference.block)
                .into_iter()
                .flat_map(|block| block.attribute_definitions().cloned().collect::<Vec<_>>())
                .map(|definition| {
                    let value = reference
                        .attribute(&definition.tag)
                        .unwrap_or(&definition.text.content)
                        .to_owned();
                    (definition.tag.clone(), value)
                }),
        )
        .collect(),
    }
}

#[cfg(test)]
mod tests {
    use cad_document::{ArcEntity, CircleEntity, Color, LineEntity, PolylineEntity};

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn drawing() -> (Drawing, Vec<cad_core::EntityId>) {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let ids = vec![
            tx.insert(LineEntity::new(p(0.0, 0.0), p(30.0, 40.0)))
                .unwrap(),
            tx.insert(LineEntity::new(p(0.0, 0.0), p(-10.0, 0.0)))
                .unwrap(),
            tx.insert(CircleEntity::new(p(1.0, 2.0), 5.0)).unwrap(),
            tx.insert(ArcEntity::new(
                p(0.0, 0.0),
                10.0,
                0.0,
                std::f64::consts::FRAC_PI_2,
            ))
            .unwrap(),
            tx.insert(PolylineEntity::with_bulges(
                vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(0.0, 4.0)],
                vec![0.0, 1.0, 0.0, 1.0],
                true,
            ))
            .unwrap(),
        ];
        tx.commit();
        (drawing, ids)
    }

    #[test]
    fn nothing_selected_is_an_empty_summary() {
        let (drawing, _) = drawing();
        let summary = SelectionSummary::of(&drawing, &Selection::default());
        assert_eq!(summary, SelectionSummary::default());
        assert_eq!(summary.headline(), "No selection");
    }

    #[test]
    fn one_object_shows_its_properties_and_its_geometry() {
        let (drawing, ids) = drawing();
        let mut selection = Selection::default();
        selection.add(ids[0]);
        let summary = SelectionSummary::of(&drawing, &selection);
        assert_eq!(summary.count, 1);
        assert_eq!(summary.kinds, vec![("line".to_owned(), 1)]);
        assert_eq!(summary.headline(), "1 line");
        assert_eq!(summary.layer.as_deref(), Some("0"));
        assert_eq!(summary.color.as_deref(), Some("ByLayer"));
        assert_eq!(summary.linetype.as_deref(), Some("ByLayer"));
        assert_eq!(summary.lineweight.as_deref(), Some("ByLayer"));
        assert_eq!(
            summary.geometry,
            vec![
                ("Start".to_owned(), "0.0000, 0.0000".to_owned()),
                ("End".to_owned(), "30.0000, 40.0000".to_owned()),
                ("Length".to_owned(), "50.0000".to_owned()),
                ("Angle".to_owned(), "53.1301°".to_owned()),
            ]
        );

        selection.replace_with(ids[1]);
        let left = SelectionSummary::of(&drawing, &selection);
        assert_eq!(left.geometry[3].1, "180.0000°", "angles are in [0, 360)");

        selection.replace_with(ids[2]);
        let circle = SelectionSummary::of(&drawing, &selection);
        assert_eq!(circle.headline(), "1 circle");
        assert_eq!(
            circle.geometry[1],
            ("Radius".to_owned(), "5.0000".to_owned())
        );
        assert_eq!(
            circle.geometry[2],
            ("Diameter".to_owned(), "10.0000".to_owned())
        );

        selection.replace_with(ids[3]);
        let arc = SelectionSummary::of(&drawing, &selection);
        assert_eq!(
            arc.geometry[2],
            ("Start angle".to_owned(), "0.0000°".to_owned())
        );
        assert_eq!(
            arc.geometry[3],
            ("End angle".to_owned(), "90.0000°".to_owned())
        );
        assert_eq!(
            arc.geometry[4],
            ("Arc length".to_owned(), "15.7080".to_owned())
        );

        selection.replace_with(ids[4]);
        let slot = SelectionSummary::of(&drawing, &selection);
        assert_eq!(slot.geometry[0], ("Vertices".to_owned(), "4".to_owned()));
        assert_eq!(slot.geometry[1], ("Closed".to_owned(), "Yes".to_owned()));
        // Two sides of 10 and two half circles of radius 2.
        assert_eq!(
            slot.geometry[2],
            ("Length".to_owned(), "32.5664".to_owned())
        );
    }

    #[test]
    fn several_objects_show_what_they_share_and_no_geometry() {
        let (mut drawing, ids) = drawing();
        let mut tx = drawing.transaction();
        tx.set_properties(
            ids[1],
            Properties {
                color: Color::Index(1),
                ..Properties::default()
            },
        )
        .unwrap();
        tx.commit();
        let mut selection = Selection::default();
        for id in &ids {
            selection.add(*id);
        }
        let summary = SelectionSummary::of(&drawing, &selection);
        assert_eq!(summary.count, 5);
        assert_eq!(
            summary.kinds,
            vec![
                ("lines".to_owned(), 2),
                ("circle".to_owned(), 1),
                ("arc".to_owned(), 1),
                ("polyline".to_owned(), 1),
            ]
        );
        assert_eq!(
            summary.headline(),
            "5 objects: 2 lines, 1 circle, 1 arc, 1 polyline"
        );
        assert_eq!(summary.layer.as_deref(), Some("0"), "all on layer 0");
        assert_eq!(summary.color, None, "one is red");
        assert_eq!(summary.linetype.as_deref(), Some("ByLayer"));
        assert!(summary.geometry.is_empty());
    }

    /// The summary is bounded: a selection of many objects is a few counts
    /// and a few strings.
    #[test]
    fn the_summary_does_not_grow_with_the_selection() {
        let lines = (0..10_000)
            .map(|i| Entity::Line(LineEntity::new(p(f64::from(i), 0.0), p(f64::from(i), 1.0))));
        let drawing = Drawing::from_entities(lines);
        let mut selection = Selection::default();
        for (id, _) in drawing.entities() {
            selection.add(id);
        }
        let summary = SelectionSummary::of(&drawing, &selection);
        assert_eq!(summary.count, 10_000);
        assert_eq!(summary.kinds, vec![("lines".to_owned(), 10_000)]);
        assert_eq!(summary.headline(), "10000 lines");
        assert!(summary.geometry.is_empty());
    }
}

/// The most selected objects a request to the AI may describe (ADR-0037):
/// the service refuses more.
pub const AI_CONTEXT_LIMIT: usize = 500;

/// The most vertices of one polyline a request describes.
pub const AI_CONTEXT_VERTICES: usize = 64;

/// What a request to the AI may carry with the user's consent (ADR-0037,
/// #260): the selected objects as the Properties panel describes them,
/// with the IDs an intent refers to them by, the layers by name, and the
/// unit. Shown to the user before it is sent; never kept by the service.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiContext {
    pub unit: Option<&'static str>,
    pub layers: Vec<String>,
    pub objects: Vec<AiObject>,
    /// Objects selected beyond `AI_CONTEXT_LIMIT`, left out.
    pub left_out: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiObject {
    pub id: u64,
    pub kind: &'static str,
    /// `start 0,0; end 100,0; length 100; angle 0°`.
    pub geometry: String,
    pub layer: Option<String>,
}

impl AiContext {
    /// The context of `selection` in `drawing`.
    pub fn of(drawing: &Drawing, selection: &crate::Selection) -> Self {
        let mut ids: Vec<cad_core::EntityId> = selection.iter().collect();
        ids.sort_unstable();
        let left_out = ids.len().saturating_sub(AI_CONTEXT_LIMIT);
        let objects = ids
            .into_iter()
            .take(AI_CONTEXT_LIMIT)
            .filter_map(|id| {
                let entity = drawing.entity(id)?;
                let mut geometry = geometry_of(entity, drawing)
                    .into_iter()
                    .map(|(label, value)| format!("{} {value}", label.to_lowercase()))
                    .collect::<Vec<_>>()
                    .join("; ");
                // The panel counts a polyline's vertices; an edit needs them.
                if let Entity::Polyline(polyline) = entity {
                    let listed: Vec<String> = polyline
                        .vertices
                        .iter()
                        .take(AI_CONTEXT_VERTICES)
                        .enumerate()
                        .map(|(index, vertex)| {
                            let bulge = polyline.bulge(index);
                            if bulge == 0.0 {
                                format!("({})", point(*vertex))
                            } else {
                                format!("({}) bulge {}", point(*vertex), number(bulge))
                            }
                        })
                        .collect();
                    geometry.push_str(&format!("; points {}", listed.join(", ")));
                    if polyline.vertices.len() > AI_CONTEXT_VERTICES {
                        geometry.push_str(&format!(
                            "; and {} more vertices",
                            polyline.vertices.len() - AI_CONTEXT_VERTICES
                        ));
                    }
                }
                let layer = drawing
                    .properties(id)
                    .and_then(|p| drawing.layers().get(p.layer))
                    .map(|layer| layer.name.clone());
                Some(AiObject {
                    id: id.raw(),
                    kind: KINDS[kind_index(entity)].0,
                    geometry,
                    layer,
                })
            })
            .collect();
        Self {
            unit: drawing.unit().map(cad_core::LengthUnit::suffix),
            layers: drawing
                .layers()
                .iter()
                .map(|(_, layer)| layer.name.clone())
                .collect(),
            objects,
            left_out,
        }
    }
}
