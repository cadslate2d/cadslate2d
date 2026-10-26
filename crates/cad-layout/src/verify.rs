//! A plan checked against its brief, independently of the search that
//! made it: what the tests, the corpus, and the commands rely on. It
//! shares no geometry with the search — every floor, contact, and outside
//! wall is worked out here again from the rooms' centreline rectangles —
//! so a fault in one is not hidden by the same fault in the other.

use cad_core::tolerance::LinearTolerance;

use crate::search::{AREA_TOLERANCE, DOOR_CLEARANCE};
use crate::{Layout, Rect, Spec};

/// Every hard constraint `layout` breaks, in words; empty when it keeps
/// them all.
pub fn verify(spec: &Spec, layout: &Layout) -> Vec<String> {
    let t = LinearTolerance::DEFAULT
        .at_magnitude(spec.terrain.width.max(spec.terrain.depth))
        .value();
    let (te, ti) = (spec.exterior_wall, spec.interior_wall);
    let mut broken = Vec::new();

    let s = spec.setbacks;
    let f = layout.footprint;
    let o = layout.outline;
    if f.x < s.left - t
        || f.y < s.front - t
        || f.x + f.w > spec.terrain.width - s.right + t
        || f.y + f.h > spec.terrain.depth - s.back + t
    {
        broken.push("the building is outside the buildable area".to_owned());
    }
    let walled = [o.x - te / 2.0, o.y - te / 2.0, o.w + te, o.h + te];
    if [f.x, f.y, f.w, f.h]
        .iter()
        .zip(walled)
        .any(|(a, b)| (a - b).abs() > t)
    {
        broken.push("the footprint is not the outline with its exterior walls".to_owned());
    }

    let n = spec.rooms.len();
    let spaces = n + usize::from(layout.circulation.is_some());
    if layout.rooms.len() != spaces || layout.circulation.is_some_and(|c| c != n) {
        broken.push("the plan's spaces are not the brief's rooms".to_owned());
        return broken;
    }

    // The rooms tile the outline: inside it, apart, and filling it.
    let mut covered = 0.0;
    for (i, room) in layout.rooms.iter().enumerate() {
        let r = room.rect;
        if r.w <= t || r.h <= t {
            broken.push(format!("the {} has no extent", room.name));
        }
        if r.x < o.x - t || r.y < o.y - t || r.x + r.w > o.x + o.w + t || r.y + r.h > o.y + o.h + t
        {
            broken.push(format!("the {} is outside the building", room.name));
        }
        covered += r.w * r.h;
        for other in &layout.rooms[i + 1..] {
            let q = other.rect;
            let w = (r.x + r.w).min(q.x + q.w) - r.x.max(q.x);
            let h = (r.y + r.h).min(q.y + q.h) - r.y.max(q.y);
            if w > t && h > t {
                broken.push(format!("the {} and the {} overlap", room.name, other.name));
            }
        }
    }
    if (covered - o.w * o.h).abs() > t * (o.w + o.h) {
        broken.push("the rooms leave a gap in the building".to_owned());
    }

    // Each floor: its rectangle less half of each wall round it.
    let floors: Vec<Rect> = layout
        .rooms
        .iter()
        .map(|room| {
            let r = room.rect;
            let half = |outside: bool| if outside { te / 2.0 } else { ti / 2.0 };
            let (left, right) = (
                half((r.x - o.x).abs() <= t),
                half((r.x + r.w - (o.x + o.w)).abs() <= t),
            );
            let (bottom, top) = (
                half((r.y - o.y).abs() <= t),
                half((r.y + r.h - (o.y + o.h)).abs() <= t),
            );
            Rect::new(
                r.x + left,
                r.y + bottom,
                r.w - left - right,
                r.h - bottom - top,
            )
        })
        .collect();
    for (room, floor) in layout.rooms.iter().zip(&floors) {
        let given = room.inner;
        if [
            given.x - floor.x,
            given.y - floor.y,
            given.w - floor.w,
            given.h - floor.h,
        ]
        .iter()
        .any(|d| d.abs() > t)
        {
            broken.push(format!(
                "the {}'s floor does not match its walls",
                room.name
            ));
        }
    }
    for (room, floor) in spec.rooms.iter().zip(&floors) {
        let area = floor.w.max(0.0) * floor.h.max(0.0);
        if (area - room.area).abs() > room.area * AREA_TOLERANCE + t {
            broken.push(format!("the {}'s area is {area:.3} m²", room.name));
        }
        let (narrow, wide) = (floor.w.min(floor.h), floor.w.max(floor.h));
        if narrow < room.min_width - t {
            broken.push(format!(
                "the {} is narrower than {} m",
                room.name, room.min_width
            ));
        }
        if narrow <= 0.0 || wide > room.max_aspect * narrow + t {
            broken.push(format!("the {} is longer than its aspect ratio", room.name));
        }
    }
    if let Some(c) = layout.circulation {
        let floor = floors[c];
        if floor.w.min(floor.h) < spec.door - t {
            broken.push("the circulation is narrower than a door".to_owned());
        }
        let budget = spec.max_circulation * spec.rooms.iter().map(|r| r.area).sum::<f64>();
        if floor.w * floor.h > budget + t {
            broken.push(format!(
                "the circulation's {:.2} m² is more than the brief allows, {budget:.2} m²",
                floor.w * floor.h
            ));
        }
    }

    // Doors: a stretch of shared wall long enough for one.
    let span = spec.door + 2.0 * DOOR_CLEARANCE + te;
    let contact = |a: Rect, b: Rect| -> f64 {
        let along = |a0: f64, a1: f64, b0: f64, b1: f64| (a1.min(b1) - a0.max(b0)).max(0.0);
        let side_by_side = (a.x + a.w - b.x).abs() <= t || (b.x + b.w - a.x).abs() <= t;
        let stacked = (a.y + a.h - b.y).abs() <= t || (b.y + b.h - a.y).abs() <= t;
        let mut length: f64 = 0.0;
        if side_by_side {
            length = length.max(along(a.y, a.y + a.h, b.y, b.y + b.h));
        }
        if stacked {
            length = length.max(along(a.x, a.x + a.w, b.x, b.x + b.w));
        }
        length
    };
    let rects: Vec<Rect> = layout.rooms.iter().map(|r| r.rect).collect();
    for &(a, b) in &layout.doors {
        if a >= spaces || b >= spaces || a == b {
            broken.push("a door names a space the plan does not have".to_owned());
            continue;
        }
        if contact(rects[a], rects[b]) < span - t {
            broken.push(format!(
                "the door between the {} and the {} has no wall long enough",
                layout.rooms[a].name, layout.rooms[b].name
            ));
        }
    }
    for (a, b) in spec.adjacent_pairs() {
        if !layout.doors.contains(&(a.min(b), a.max(b))) {
            broken.push(format!(
                "the {} does not open onto the {}",
                spec.rooms[a].name, spec.rooms[b].name
            ));
        }
    }
    // The entrance's door: one side of it on the outline, long enough.
    let entrance = rects[layout.entrance];
    let mut outside: f64 = 0.0;
    if (entrance.x - o.x).abs() <= t || (entrance.x + entrance.w - (o.x + o.w)).abs() <= t {
        outside = outside.max(entrance.h);
    }
    if (entrance.y - o.y).abs() <= t || (entrance.y + entrance.h - (o.y + o.h)).abs() <= t {
        outside = outside.max(entrance.w);
    }
    if layout.entrance != spec.entrance() || outside < span - t {
        broken.push("the entrance has no outside wall for its door".to_owned());
    }
    let mut reached = vec![false; spaces];
    reached[layout.entrance] = true;
    let mut grew = true;
    while grew {
        grew = false;
        for &(a, b) in &layout.doors {
            if a < spaces && b < spaces && reached[a] != reached[b] {
                reached[a] = true;
                reached[b] = true;
                grew = true;
            }
        }
    }
    for (i, r) in reached.iter().enumerate() {
        if !r {
            broken.push(format!("the {} cannot be reached", layout.rooms[i].name));
        }
    }
    broken
}
