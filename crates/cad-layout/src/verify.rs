//! A plan checked against its brief, independently of the search that
//! made it: what the tests, the corpus, and the commands rely on.

use cad_core::tolerance::LinearTolerance;

use crate::search::{AREA_TOLERANCE, DOOR_CLEARANCE};
use crate::{Layout, Spec};

/// Every hard constraint `layout` breaks, in words; empty when it keeps
/// them all.
pub fn verify(spec: &Spec, layout: &Layout) -> Vec<String> {
    let t = LinearTolerance::DEFAULT.at_magnitude(spec.terrain.width.max(spec.terrain.depth));
    let mut broken = Vec::new();
    let s = spec.setbacks;
    let f = layout.footprint;
    if f.x < s.left - t.value()
        || f.y < s.front - t.value()
        || f.right() > spec.terrain.width - s.right + t.value()
        || f.top() > spec.terrain.depth - s.back + t.value()
    {
        broken.push("the building is outside the buildable area".to_owned());
    }
    let spaces = spec.rooms.len() + usize::from(layout.circulation.is_some());
    if layout.rooms.len() != spaces || layout.circulation.is_some_and(|c| c != spec.rooms.len()) {
        broken.push("a room is missing".to_owned());
        return broken;
    }
    // The rooms tile the outline: inside it, apart, and filling it.
    let o = layout.outline;
    let mut covered = 0.0;
    for (i, room) in layout.rooms.iter().enumerate() {
        let r = room.rect;
        if r.x < o.x - t.value()
            || r.y < o.y - t.value()
            || r.right() > o.right() + t.value()
            || r.top() > o.top() + t.value()
        {
            broken.push(format!("the {} is outside the building", room.name));
        }
        covered += r.area();
        for other in &layout.rooms[i + 1..] {
            let q = other.rect;
            let w = r.right().min(q.right()) - r.x.max(q.x);
            let h = r.top().min(q.top()) - r.y.max(q.y);
            if w > t.value() && h > t.value() {
                broken.push(format!("the {} and the {} overlap", room.name, other.name));
            }
        }
    }
    if (covered - o.area()).abs() > o.area() * 1e-9 {
        broken.push("the rooms leave a gap".to_owned());
    }
    for (room, placed) in spec.rooms.iter().zip(&layout.rooms) {
        let floor = placed.inner;
        if (floor.area() - room.area).abs() > room.area * AREA_TOLERANCE {
            broken.push(format!(
                "the {}'s area is {:.2} m²",
                room.name,
                floor.area()
            ));
        }
        if floor.w.min(floor.h) < room.min_width - t.value() {
            broken.push(format!(
                "the {} is narrower than {} m",
                room.name, room.min_width
            ));
        }
        if floor.aspect() > room.max_aspect + 1e-9 {
            broken.push(format!("the {} is longer than its aspect ratio", room.name));
        }
    }
    if let Some(c) = layout.circulation {
        let passage = layout.rooms[c].inner;
        if passage.w.min(passage.h) < spec.door - t.value() {
            broken.push("the circulation is narrower than a door".to_owned());
        }
    }
    let span = spec.door + 2.0 * DOOR_CLEARANCE + spec.exterior_wall;
    let rects: Vec<_> = layout.rooms.iter().map(|r| r.rect).collect();
    for &(a, b) in &layout.doors {
        if rects[a].shared(&rects[b], t) < span {
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
    if rects[layout.entrance].on_outline(&o, t) < span {
        broken.push("the entrance has no outside wall for its door".to_owned());
    }
    let mut reached = vec![false; rects.len()];
    reached[layout.entrance] = true;
    let mut grew = true;
    while grew {
        grew = false;
        for &(a, b) in &layout.doors {
            if reached[a] != reached[b] {
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
