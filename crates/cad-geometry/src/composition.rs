//! Where a composed sheet's views go (ADR-0041 §1, §2): a pure function of
//! the sheet's drawing area, what is already placed on it, and the views to
//! place, each with the box its contents and annotations were measured to
//! fill around its origin.
//!
//! Projected views form a tree rooted at the principal view. Each goes in
//! its slot beside its parent — which side by the sheet's projection, first
//! or third angle — aligned with it by reference coordinates: a row shares
//! the origin's height, a column its width. A view from behind goes beyond
//! the side view of its row. The boxes decide only the spacing along the
//! other axis, a gap apart. A new principal view and its new projected
//! views move as one block to the first place the block fits, from the top
//! left; free views are packed into what is left, largest first. Nothing
//! already placed moves. A view that cannot be placed refuses the whole,
//! saying which and how large it is.

use std::fmt;

use cad_core::{Aabb, Point2, Vec2};

/// Where a view looks at the object from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Looking {
    Front,
    Behind,
    Left,
    Right,
    Above,
    Below,
}

impl Looking {
    pub fn name(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Behind => "behind",
            Self::Left => "left",
            Self::Right => "right",
            Self::Above => "above",
            Self::Below => "below",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase();
        let text = text.strip_prefix("from_").unwrap_or(&text);
        Some(match text {
            "front" => Self::Front,
            "behind" | "rear" | "back" => Self::Behind,
            "left" => Self::Left,
            "right" => Self::Right,
            "above" | "top" => Self::Above,
            "below" | "bottom" => Self::Below,
            _ => return None,
        })
    }
}

/// First- or third-angle projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Angle {
    First,
    Third,
}

/// How a view is placed.
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    /// The root of the projection: the front, as a rule. One per sheet.
    Principal,
    /// Beside its parent, looking from a side.
    Projected { of: String, looking: Looking },
    /// Wherever there is room.
    Free,
}

/// A view to place: its id, how it is placed, and the box its contents
/// fill, measured from its own origin.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewBox {
    pub id: String,
    pub role: Role,
    pub extent: Aabb,
}

/// A view and where its origin is on the sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub view: ViewBox,
    pub origin: Point2,
}

impl Placed {
    /// The box it fills on the sheet.
    pub fn bounds(&self) -> Aabb {
        shifted(&self.view.extent, self.origin - Point2::ORIGIN)
    }
}

/// The sheet's drawing area, what in it is not free (the title block), the
/// gap between views, and the projection.
#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub area: Aabb,
    pub obstacles: Vec<Aabb>,
    pub gap: f64,
    pub angle: Angle,
}

/// Why the views could not be placed.
#[derive(Clone, Debug, PartialEq)]
pub enum PlaceError {
    /// Two views with one id.
    Duplicate(String),
    /// A projected view whose parent is not on the sheet or among the new.
    UnknownParent { view: String, of: String },
    /// Projected views that are each other's parents.
    Cycle(String),
    /// A second principal view.
    SecondPrincipal(String),
    /// A projected view whose chain of parents reaches no principal view.
    NoPrincipal(String),
    /// Two views of one parent looking from the same side.
    SlotTaken {
        view: String,
        of: String,
        looking: Looking,
    },
    /// A view projected to look from the front.
    ProjectedFront(String),
    /// A view, or a principal view's block, that fits nowhere free.
    NoRoom { view: String, size: Vec2 },
}

impl fmt::Display for PlaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(id) => write!(f, "two views are called {id}"),
            Self::UnknownParent { view, of } => {
                write!(
                    f,
                    "view {view} is projected of {of}, which is not on the sheet"
                )
            }
            Self::Cycle(id) => write!(f, "view {id} is projected of itself, through others"),
            Self::SecondPrincipal(id) => {
                write!(
                    f,
                    "view {id} would be a second principal view; a sheet has one"
                )
            }
            Self::NoPrincipal(id) => {
                write!(f, "view {id} is projected from no principal view")
            }
            Self::SlotTaken { view, of, looking } => write!(
                f,
                "view {view} would take the place of another view of {of} from the {}",
                looking.name()
            ),
            Self::ProjectedFront(id) => write!(
                f,
                "view {id} is projected looking from the front; the front is the principal view"
            ),
            Self::NoRoom { view, size } => write!(
                f,
                "view {view} needs {:.0} × {:.0} free on the sheet and there is not that much room",
                size.x, size.y
            ),
        }
    }
}

impl std::error::Error for PlaceError {}

fn shifted(b: &Aabb, by: Vec2) -> Aabb {
    Aabb::from_points(b.min() + by, b.max() + by)
}

fn size(b: &Aabb) -> Vec2 {
    b.max() - b.min()
}

/// Whether two boxes share some area: boxes that only touch do not.
fn overlap(a: &Aabb, b: &Aabb) -> bool {
    a.min().x < b.max().x && b.min().x < a.max().x && a.min().y < b.max().y && b.min().y < a.max().y
}

fn grown(b: &Aabb, by: f64) -> Aabb {
    Aabb::from_points(
        Point2::new(b.min().x - by, b.min().y - by),
        Point2::new(b.max().x + by, b.max().y + by),
    )
}

fn within(outer: &Aabb, inner: &Aabb) -> bool {
    let t = 1e-9 * (size(outer).x + size(outer).y + 1.0);
    inner.min().x >= outer.min().x - t
        && inner.min().y >= outer.min().y - t
        && inner.max().x <= outer.max().x + t
        && inner.max().y <= outer.max().y + t
}

/// Which way a slot is from its parent on the page: +x, −x, +y, −y.
fn side(looking: Looking, angle: Angle) -> Option<(i8, i8)> {
    let third = match looking {
        Looking::Left => (-1, 0),
        Looking::Right => (1, 0),
        Looking::Above => (0, 1),
        Looking::Below => (0, -1),
        Looking::Front | Looking::Behind => return None,
    };
    Some(match angle {
        Angle::Third => third,
        Angle::First => (-third.0, -third.1),
    })
}

/// The origin of a view placed beside `reference` (a box on the sheet) on
/// `toward`'s side, its shared coordinate that of `parent_origin`.
fn beside(
    parent_origin: Point2,
    reference: &Aabb,
    extent: &Aabb,
    toward: (i8, i8),
    gap: f64,
) -> Point2 {
    match toward {
        (1, 0) => Point2::new(reference.max().x + gap - extent.min().x, parent_origin.y),
        (-1, 0) => Point2::new(reference.min().x - gap - extent.max().x, parent_origin.y),
        (0, 1) => Point2::new(parent_origin.x, reference.max().y + gap - extent.min().y),
        _ => Point2::new(parent_origin.x, reference.min().y - gap - extent.max().y),
    }
}

/// Places `new` on `sheet`, where `placed` already are; each new view with
/// its origin.
pub fn place(sheet: &Sheet, placed: &[Placed], new: &[ViewBox]) -> Result<Vec<Placed>, PlaceError> {
    // Ids, one each.
    let mut ids: Vec<&str> = placed.iter().map(|p| p.view.id.as_str()).collect();
    for view in new {
        if ids.contains(&view.id.as_str()) {
            return Err(PlaceError::Duplicate(view.id.clone()));
        }
        ids.push(&view.id);
    }
    let role_of = |id: &str| -> Option<&Role> {
        placed
            .iter()
            .map(|p| &p.view)
            .chain(new)
            .find(|v| v.id == id)
            .map(|v| &v.role)
    };
    // One principal; every projected view's parents known, reaching it,
    // in no cycle; each slot taken once.
    let principals = placed
        .iter()
        .map(|p| &p.view)
        .chain(new)
        .filter(|v| v.role == Role::Principal)
        .map(|v| v.id.clone())
        .collect::<Vec<_>>();
    if principals.len() > 1 {
        return Err(PlaceError::SecondPrincipal(principals[1].clone()));
    }
    let mut slots: Vec<(&str, Looking)> = Vec::new();
    for view in placed.iter().map(|p| &p.view).chain(new) {
        let Role::Projected { of, looking } = &view.role else {
            continue;
        };
        if *looking == Looking::Front {
            return Err(PlaceError::ProjectedFront(view.id.clone()));
        }
        if slots.contains(&(of.as_str(), *looking)) {
            return Err(PlaceError::SlotTaken {
                view: view.id.clone(),
                of: of.clone(),
                looking: *looking,
            });
        }
        slots.push((of, *looking));
        let mut at = view.id.as_str();
        let mut steps = 0;
        loop {
            match role_of(at) {
                None => {
                    return Err(PlaceError::UnknownParent {
                        view: view.id.clone(),
                        of: at.to_owned(),
                    });
                }
                Some(Role::Principal) => break,
                Some(Role::Free) => return Err(PlaceError::NoPrincipal(view.id.clone())),
                Some(Role::Projected { of, .. }) => {
                    steps += 1;
                    if steps > ids.len() {
                        return Err(PlaceError::Cycle(view.id.clone()));
                    }
                    at = of;
                }
            }
        }
    }

    // Origins, absolute where the principal is placed, else relative to a
    // new principal at the origin.
    let mut origins: Vec<(String, Point2)> = placed
        .iter()
        .map(|p| (p.view.id.clone(), p.origin))
        .collect();
    let extent_of = |id: &str| -> Aabb {
        placed
            .iter()
            .map(|p| &p.view)
            .chain(new)
            .find(|v| v.id == id)
            .map(|v| v.extent)
            .expect("known")
    };
    let new_principal = new.iter().find(|v| v.role == Role::Principal);
    if let Some(principal) = new_principal {
        origins.push((principal.id.clone(), Point2::ORIGIN));
    }
    // Projected views in order: each after its parent; of one parent, the
    // side views first, then a view from behind beyond them, then the views
    // above and below, clear of the whole row.
    let rank = |v: &ViewBox| match &v.role {
        Role::Projected {
            looking: Looking::Left | Looking::Right,
            ..
        } => 0,
        Role::Projected {
            looking: Looking::Behind,
            ..
        } => 1,
        _ => 2,
    };
    let mut pending: Vec<&ViewBox> = new
        .iter()
        .filter(|v| matches!(v.role, Role::Projected { .. }))
        .collect();
    pending.sort_by_key(|v| rank(v));
    let mut cluster: Vec<String> = new_principal.iter().map(|v| v.id.clone()).collect();
    while !pending.is_empty() {
        let before = pending.len();
        let mut i = 0;
        while i < pending.len() {
            let view = pending[i];
            let Role::Projected { of, looking } = &view.role else {
                unreachable!()
            };
            let Some(&(_, parent_origin)) = origins.iter().find(|(id, _)| id == of) else {
                i += 1;
                continue;
            };
            // A view of this parent still to come before this one waits.
            if pending[..i].iter().any(|earlier| {
                matches!(&earlier.role, Role::Projected { of: o, .. } if o == of)
                    && rank(earlier) < rank(view)
            }) {
                i += 1;
                continue;
            }
            let parent_box = shifted(&extent_of(of), parent_origin - Point2::ORIGIN);
            // The parent's row: it and the views beside it, which a view above
            // or below clears as a whole.
            let row = origins
                .iter()
                .filter(|(id, _)| {
                    matches!(role_of(id), Some(Role::Projected { of: o, looking })
                        if o == of && matches!(looking, Looking::Left | Looking::Right | Looking::Behind))
                })
                .map(|(id, origin)| shifted(&extent_of(id), *origin - Point2::ORIGIN))
                .fold(parent_box, |a, b| a.union(&b));
            let (reference, toward) = match side(*looking, sheet.angle) {
                Some(toward @ (0, _)) => {
                    let band = Aabb::from_points(
                        Point2::new(parent_box.min().x, row.min().y),
                        Point2::new(parent_box.max().x, row.max().y),
                    );
                    (band, toward)
                }
                Some(toward) => (parent_box, toward),
                None => {
                    // Behind: beyond the side view of the parent's row, on
                    // its side; else where a view from the right goes.
                    let sides: Vec<(i8, Aabb)> = origins
                        .iter()
                        .filter_map(|(id, origin)| {
                            let Some(Role::Projected { of: o, looking: l }) = role_of(id) else {
                                return None;
                            };
                            let (dx, 0) = side(*l, sheet.angle)? else {
                                return None;
                            };
                            (o == of)
                                .then(|| (dx, shifted(&extent_of(id), *origin - Point2::ORIGIN)))
                        })
                        .collect();
                    match sides.iter().max_by_key(|(dx, _)| *dx) {
                        Some((dx, b)) => (*b, (*dx, 0)),
                        None => (
                            parent_box,
                            side(Looking::Right, sheet.angle).expect("a side"),
                        ),
                    }
                }
            };
            let origin = beside(parent_origin, &reference, &view.extent, toward, sheet.gap);
            origins.push((view.id.clone(), origin));
            if cluster.contains(of) {
                cluster.push(view.id.clone());
            }
            pending.remove(i);
        }
        if pending.len() == before {
            return Err(PlaceError::Cycle(pending[0].id.clone()));
        }
    }

    // Centred where the principal's block and every free view still fit,
    // so a view added later has room on every side; else from the top
    // left, which leaves the rest of the sheet in one piece.
    let mut out = match lay_out(
        sheet,
        placed,
        new,
        new_principal,
        &cluster,
        &origins,
        &extent_of,
        true,
    ) {
        Ok(out) => out,
        Err(_) => lay_out(
            sheet,
            placed,
            new,
            new_principal,
            &cluster,
            &origins,
            &extent_of,
            false,
        )?,
    };
    // Never crowded: a new view that comes within the gap of another is
    // refused, not drawn.
    let clear = sheet.gap / 2.0 - 1e-9;
    let before: Vec<Aabb> = placed
        .iter()
        .map(Placed::bounds)
        .chain(sheet.obstacles.iter().copied())
        .collect();
    for (i, a) in out.iter().enumerate() {
        let others = out[..i]
            .iter()
            .map(Placed::bounds)
            .chain(before.iter().copied());
        let crowded = !within(&sheet.area, &a.bounds())
            || others
                .into_iter()
                .any(|b| overlap(&grown(&a.bounds(), clear), &grown(&b, clear)));
        if crowded {
            return Err(PlaceError::NoRoom {
                view: a.view.id.clone(),
                size: size(&a.view.extent),
            });
        }
    }
    // In the order they were given.
    out.sort_by_key(|p| new.iter().position(|v| v.id == p.view.id));
    Ok(out)
}

/// The new views placed: the principal's block centred (`centre`) or from
/// the top left, then projected views of what was placed before, then free
/// views largest first.
#[allow(clippy::too_many_arguments)]
fn lay_out(
    sheet: &Sheet,
    placed: &[Placed],
    new: &[ViewBox],
    new_principal: Option<&ViewBox>,
    cluster: &[String],
    origins: &[(String, Point2)],
    extent_of: &dyn Fn(&str) -> Aabb,
    centre: bool,
) -> Result<Vec<Placed>, PlaceError> {
    let mut taken: Vec<Aabb> = sheet.obstacles.clone();
    taken.extend(placed.iter().map(Placed::bounds));
    let mut out: Vec<Placed> = Vec::new();
    let origin_of = |id: &str, origins: &[(String, Point2)]| {
        origins
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, o)| *o)
            .expect("placed")
    };

    // The new principal's block, moved as one to where it fits.
    if let Some(principal) = new_principal {
        let block = cluster
            .iter()
            .map(|id| shifted(&extent_of(id), origin_of(id, origins) - Point2::ORIGIN))
            .reduce(|a, b| a.union(&b))
            .expect("the principal at least");
        // Centred where it can be, so a view added later has room on every
        // side; else where it first fits.
        let at = if centre {
            centred(sheet, &taken, &block)
        } else {
            fit(sheet, &taken, &block)
        };
        let Some(at) = at else {
            return Err(PlaceError::NoRoom {
                view: principal.id.clone(),
                size: size(&block),
            });
        };
        let by = at - block.min();
        for id in cluster {
            let view = new.iter().find(|v| &v.id == id).expect("new").clone();
            let origin = origin_of(id, origins) + by;
            let bounds = shifted(&view.extent, origin - Point2::ORIGIN);
            taken.push(bounds);
            out.push(Placed { view, origin });
        }
    }
    // Projected views of what was placed before: where their slot is.
    for view in new {
        if out.iter().any(|p| p.view.id == view.id) || !matches!(view.role, Role::Projected { .. })
        {
            continue;
        }
        let origin = origin_of(&view.id, origins);
        let bounds = shifted(&view.extent, origin - Point2::ORIGIN);
        let free = within(&sheet.area, &bounds)
            && !taken.iter().any(|t| {
                overlap(
                    &grown(t, sheet.gap / 2.0 - 1e-9),
                    &grown(&bounds, sheet.gap / 2.0 - 1e-9),
                )
            });
        if !free {
            return Err(PlaceError::NoRoom {
                view: view.id.clone(),
                size: size(&view.extent),
            });
        }
        taken.push(bounds);
        out.push(Placed {
            view: view.clone(),
            origin,
        });
    }
    // Free views, largest first, each where it first fits.
    let mut free: Vec<&ViewBox> = new.iter().filter(|v| v.role == Role::Free).collect();
    free.sort_by(|a, b| {
        let (sa, sb) = (size(&a.extent), size(&b.extent));
        (sb.x * sb.y)
            .total_cmp(&(sa.x * sa.y))
            .then_with(|| a.id.cmp(&b.id))
    });
    for view in free {
        let Some(at) = fit(sheet, &taken, &view.extent) else {
            return Err(PlaceError::NoRoom {
                view: view.id.clone(),
                size: size(&view.extent),
            });
        };
        let origin = Point2::ORIGIN + (at - view.extent.min());
        taken.push(shifted(&view.extent, origin - Point2::ORIGIN));
        out.push(Placed {
            view: view.clone(),
            origin,
        });
    }
    Ok(out)
}

/// The lower left corner that centres `block` in the drawing area, when it
/// is free there.
fn centred(sheet: &Sheet, taken: &[Aabb], block: &Aabb) -> Option<Point2> {
    let centre = sheet.area.min() + size(&sheet.area) * 0.5;
    let at = centre - size(block) * 0.5;
    let candidate = Aabb::from_points(at, at + size(block));
    let clear = sheet.gap / 2.0 - 1e-9;
    (within(&sheet.area, &candidate)
        && !taken
            .iter()
            .any(|t| overlap(&grown(t, clear), &grown(&candidate, clear))))
    .then_some(at)
}

/// The lower left corner at which `block` first fits, from the top left:
/// inside the area, a gap from everything taken.
fn fit(sheet: &Sheet, taken: &[Aabb], block: &Aabb) -> Option<Point2> {
    let (w, h) = (size(block).x, size(block).y);
    let mut xs = vec![sheet.area.min().x];
    xs.extend(taken.iter().map(|t| t.max().x + sheet.gap));
    let mut tops = vec![sheet.area.max().y];
    tops.extend(taken.iter().map(|t| t.min().y - sheet.gap));
    xs.sort_by(f64::total_cmp);
    tops.sort_by(|a, b| b.total_cmp(a));
    let clear = sheet.gap / 2.0 - 1e-9;
    for &top in &tops {
        for &x in &xs {
            let candidate = Aabb::from_points(Point2::new(x, top - h), Point2::new(x + w, top));
            if within(&sheet.area, &candidate)
                && !taken
                    .iter()
                    .any(|t| overlap(&grown(t, clear), &grown(&candidate, clear)))
            {
                return Some(candidate.min());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn view(id: &str, role: Role, w: f64, h: f64) -> ViewBox {
        // A box reaching a little below and left of the origin, as a
        // dimension below a view's geometry does.
        ViewBox {
            id: id.to_owned(),
            role,
            extent: Aabb::from_points(p(-5.0, -12.0), p(w - 5.0, h - 12.0)),
        }
    }

    fn projected(of: &str, looking: Looking) -> Role {
        Role::Projected {
            of: of.to_owned(),
            looking,
        }
    }

    fn a3(angle: Angle) -> Sheet {
        Sheet {
            area: Aabb::from_points(p(20.0, 10.0), p(410.0, 287.0)),
            obstacles: vec![Aabb::from_points(p(230.0, 10.0), p(410.0, 42.0))],
            gap: 10.0,
            angle,
        }
    }

    fn origin(out: &[Placed], id: &str) -> Point2 {
        out.iter().find(|p| p.view.id == id).unwrap().origin
    }

    #[test]
    fn third_angle_puts_the_left_view_left_and_the_top_above_aligned_by_origin() {
        let views = [
            view("front", Role::Principal, 100.0, 60.0),
            view("left", projected("front", Looking::Left), 40.0, 60.0),
            view("top", projected("front", Looking::Above), 100.0, 40.0),
        ];
        let out = place(&a3(Angle::Third), &[], &views).unwrap();
        let (front, left, top) = (
            origin(&out, "front"),
            origin(&out, "left"),
            origin(&out, "top"),
        );
        assert_eq!(left.y, front.y, "a row shares heights");
        assert_eq!(top.x, front.x, "a column shares widths");
        assert!(left.x < front.x && top.y > front.y);
        // A gap between the boxes, exactly.
        let b = |id| out.iter().find(|q| q.view.id == id).unwrap().bounds();
        assert!((b("front").min().x - b("left").max().x - 10.0).abs() < 1e-9);
        assert!((b("top").min().y - b("front").max().y - 10.0).abs() < 1e-9);
    }

    #[test]
    fn first_angle_puts_them_on_the_other_side() {
        let views = [
            view("front", Role::Principal, 100.0, 60.0),
            view("left", projected("front", Looking::Left), 40.0, 60.0),
            view("top", projected("front", Looking::Above), 100.0, 40.0),
        ];
        let out = place(&a3(Angle::First), &[], &views).unwrap();
        assert!(origin(&out, "left").x > origin(&out, "front").x);
        assert!(origin(&out, "top").y < origin(&out, "front").y);
    }

    #[test]
    fn a_view_from_behind_goes_beyond_the_side_view() {
        let views = [
            view("front", Role::Principal, 80.0, 60.0),
            view("right", projected("front", Looking::Right), 40.0, 60.0),
            view("rear", projected("front", Looking::Behind), 80.0, 60.0),
        ];
        let out = place(&a3(Angle::Third), &[], &views).unwrap();
        let b = |id| out.iter().find(|q| q.view.id == id).unwrap().bounds();
        assert!(b("rear").min().x >= b("right").max().x + 10.0 - 1e-9);
        assert_eq!(origin(&out, "rear").y, origin(&out, "front").y);
    }

    #[test]
    fn what_is_wrong_with_the_tree_is_refused_by_name() {
        let sheet = a3(Angle::Third);
        let front = view("front", Role::Principal, 50.0, 50.0);
        assert_eq!(
            place(&sheet, &[], &[front.clone(), front.clone()]),
            Err(PlaceError::Duplicate("front".into()))
        );
        assert!(matches!(
            place(
                &sheet,
                &[],
                &[
                    front.clone(),
                    view("x", projected("nope", Looking::Left), 10.0, 10.0)
                ]
            ),
            Err(PlaceError::UnknownParent { .. })
        ));
        assert!(matches!(
            place(
                &sheet,
                &[],
                &[
                    view("a", projected("b", Looking::Left), 10.0, 10.0),
                    view("b", projected("a", Looking::Right), 10.0, 10.0)
                ]
            ),
            Err(PlaceError::Cycle(_))
        ));
        assert!(matches!(
            place(
                &sheet,
                &[],
                &[front.clone(), view("f2", Role::Principal, 10.0, 10.0)]
            ),
            Err(PlaceError::SecondPrincipal(_))
        ));
        assert!(matches!(
            place(
                &sheet,
                &[],
                &[
                    front.clone(),
                    view("l1", projected("front", Looking::Left), 10.0, 10.0),
                    view("l2", projected("front", Looking::Left), 10.0, 10.0)
                ]
            ),
            Err(PlaceError::SlotTaken { .. })
        ));
        assert!(matches!(
            place(
                &sheet,
                &[],
                &[
                    view("free", Role::Free, 10.0, 10.0),
                    view("x", projected("free", Looking::Left), 10.0, 10.0)
                ]
            ),
            Err(PlaceError::NoPrincipal(_))
        ));
        // Too large for the sheet: refused with its size, never crowded.
        match place(&sheet, &[], &[view("huge", Role::Principal, 500.0, 50.0)]) {
            Err(PlaceError::NoRoom { view, size }) => {
                assert_eq!(view, "huge");
                assert_eq!(size, Vec2::new(500.0, 50.0));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn what_is_placed_does_not_move_and_a_new_view_takes_its_slot() {
        let sheet = a3(Angle::Third);
        let first = place(&sheet, &[], &[view("front", Role::Principal, 80.0, 60.0)]).unwrap();
        let second = place(
            &sheet,
            &first,
            &[view(
                "right",
                projected("front", Looking::Right),
                40.0,
                60.0,
            )],
        )
        .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].origin.y, first[0].origin.y);
        assert!(second[0].bounds().min().x >= first[0].bounds().max().x + 10.0 - 1e-9);
    }

    fn boxes() -> impl Strategy<Value = Vec<(f64, f64, u8)>> {
        prop::collection::vec((10.0..90.0f64, 10.0..70.0f64, 0u8..6), 1..7)
    }

    proptest! {
        /// Whatever is placed: projected neighbours share their reference
        /// coordinate exactly, no two boxes overlap or come within the gap,
        /// and every box is inside the drawing area and clear of the block.
        #[test]
        fn placement_aligns_and_never_overlaps(sizes in boxes(), first in any::<bool>()) {
            let sheet = a3(if first { Angle::First } else { Angle::Third });
            let lookings = [Looking::Left, Looking::Right, Looking::Above, Looking::Below, Looking::Behind];
            let mut views = vec![view("v0", Role::Principal, sizes[0].0, sizes[0].1)];
            let mut used = Vec::new();
            for (i, (w, h, kind)) in sizes.iter().enumerate().skip(1) {
                let role = match lookings.get(*kind as usize) {
                    Some(l) if !used.contains(l) => { used.push(*l); projected("v0", *l) }
                    _ => Role::Free,
                };
                views.push(view(&format!("v{i}"), role, *w, *h));
            }
            let Ok(out) = place(&sheet, &[], &views) else { return Ok(()); };
            prop_assert_eq!(out.len(), views.len());
            let front = origin(&out, "v0");
            for placed in &out {
                if let Role::Projected { looking, .. } = &placed.view.role {
                    match looking {
                        Looking::Above | Looking::Below => prop_assert_eq!(placed.origin.x, front.x),
                        _ => prop_assert_eq!(placed.origin.y, front.y),
                    }
                }
                prop_assert!(within(&sheet.area, &placed.bounds()));
                for obstacle in &sheet.obstacles {
                    prop_assert!(!overlap(obstacle, &placed.bounds()));
                }
            }
            for (i, a) in out.iter().enumerate() {
                for b in &out[i + 1..] {
                    prop_assert!(!overlap(&grown(&a.bounds(), 4.999), &grown(&b.bounds(), 4.999)), "{:?} {:?}", a, b);
                }
            }
        }
    }
}
