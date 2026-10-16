//! The drawing order (#226, ADR-0032): which objects are painted over
//! which.
//!
//! Every record is in a tier, an `i16`. An object of a higher tier is
//! painted over one of a lower; within a tier, areas (a solid hatch, a
//! wipeout, a wide polyline) are beneath strokes, and of two areas, or two
//! strokes, the later made is over the earlier. That is the order the
//! screen draws in — areas write depth and strokes only test it — and the
//! order a page is painted in. A file keeps an order as a sequence (DXF
//! `SORTENTSTABLE`): [`paint_key`] turns tiers into one, [`tiers_of`] one
//! into tiers.

use cad_core::EntityId;

/// Where an object is painted: sorting by this is painting in the drawing
/// order, first beneath.
pub fn paint_key(id: EntityId, order: i16, fill: bool) -> (i16, bool, EntityId) {
    (order, !fill, id)
}

/// The tiers that paint objects in the sequence given — each an ID and
/// whether it is an area — as nearly as tiers can: the fewest, from 0 up.
/// A new tier begins where an object must be over one the tiers would
/// put above it: an area after a stroke, or an object made before the
/// last of its kind in the tier. Past `i16::MAX` tiers, the rest share
/// the last.
pub fn tiers_of(painted: &[(EntityId, bool)]) -> Vec<i16> {
    let mut tier: i16 = 0;
    let mut latest_area: Option<EntityId> = None;
    let mut latest_stroke: Option<EntityId> = None;
    painted
        .iter()
        .map(|&(id, fill)| {
            let latest = if fill { latest_area } else { latest_stroke };
            let after_stroke = fill && latest_stroke.is_some();
            if (after_stroke || latest.is_some_and(|latest| id < latest)) && tier < i16::MAX {
                tier += 1;
                latest_area = None;
                latest_stroke = None;
            }
            if fill {
                latest_area = Some(id);
            } else {
                latest_stroke = Some(id);
            }
            tier
        })
        .collect()
}

/// Where `DRAWORDER` puts what it moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Over everything else.
    Front,
    /// Beneath everything else.
    Back,
    /// Just over the highest tier of these reference objects, beneath what
    /// was over them.
    Above(i16),
    /// Just beneath the lowest tier of the reference objects.
    Under(i16),
}

/// The new tier of every object that moves when `chosen` goes to
/// `placement`, among `all` — every record with its tier. What is chosen
/// keeps its own order, in as many tiers as it had; for `Above` and
/// `Under`, what lay beyond the reference moves on to make room when it
/// must. When the tiers would run out, every object is renumbered, from
/// the middle of the range. Nothing moves when nothing would change.
pub fn reordered(
    all: &[(EntityId, i16)],
    chosen: &std::collections::HashSet<EntityId>,
    placement: Placement,
) -> Vec<(EntityId, i16)> {
    let mut tiers: Vec<i16> = all
        .iter()
        .filter(|(id, _)| chosen.contains(id))
        .map(|(_, tier)| *tier)
        .collect();
    tiers.sort_unstable();
    tiers.dedup();
    let others = || all.iter().filter(|(id, _)| !chosen.contains(id));
    let (Some(first), Some(last)) = (tiers.first().copied(), tiers.last().copied()) else {
        return Vec::new();
    };
    let span = tiers.len() as i32;
    let rank = |tier: i16| tiers.binary_search(&tier).expect("a chosen tier") as i32;
    // Where the chosen tiers start, and how far the others beyond `at`
    // move — up for a positive shift, down for a negative.
    let (start, at, shift): (i32, i32, i32) = match placement {
        Placement::Front => {
            let Some(top) = others().map(|(_, tier)| *tier).max() else {
                return Vec::new();
            };
            if first > top {
                return Vec::new();
            }
            (i32::from(top) + 1, 0, 0)
        }
        Placement::Back => {
            let Some(bottom) = others().map(|(_, tier)| *tier).min() else {
                return Vec::new();
            };
            if last < bottom {
                return Vec::new();
            }
            (i32::from(bottom) - span, 0, 0)
        }
        Placement::Above(reference) => {
            let reference = i32::from(reference);
            let crowded = others().any(|(_, tier)| {
                let tier = i32::from(*tier);
                tier > reference && tier <= reference + span
            });
            (reference + 1, reference, if crowded { span } else { 0 })
        }
        Placement::Under(reference) => {
            let reference = i32::from(reference);
            let crowded = others().any(|(_, tier)| {
                let tier = i32::from(*tier);
                tier < reference && tier >= reference - span
            });
            (reference - span, reference, if crowded { -span } else { 0 })
        }
    };
    let wanted: Vec<(EntityId, i16, i32)> = all
        .iter()
        .map(|&(id, tier)| {
            let now = i32::from(tier);
            let then = if chosen.contains(&id) {
                start + rank(tier)
            } else if (shift > 0 && now > at) || (shift < 0 && now < at) {
                now + shift
            } else {
                now
            };
            (id, tier, then)
        })
        .collect();
    let fits = wanted
        .iter()
        .all(|(_, _, then)| i16::try_from(*then).is_ok());
    if fits {
        return wanted
            .into_iter()
            .filter(|(_, now, then)| i32::from(*now) != *then)
            .map(|(id, _, then)| (id, then as i16))
            .collect();
    }
    // Out of tiers: the same order, every tier one apart, about zero.
    let mut used: Vec<i32> = wanted.iter().map(|(_, _, then)| *then).collect();
    used.sort_unstable();
    used.dedup();
    let middle = (used.len() / 2) as i32;
    wanted
        .into_iter()
        .filter_map(|(id, now, then)| {
            let renumbered = used.binary_search(&then).expect("a tier in use") as i32 - middle;
            let renumbered = i16::try_from(renumbered).unwrap_or(if renumbered < 0 {
                i16::MIN
            } else {
                i16::MAX
            });
            (renumbered != now).then_some((id, renumbered))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn id(raw: u64) -> EntityId {
        EntityId::from_raw(raw)
    }

    #[test]
    fn made_order_with_strokes_over_areas_needs_one_tier() {
        let painted = [(id(2), true), (id(5), true), (id(1), false), (id(3), false)];
        assert_eq!(tiers_of(&painted), [0, 0, 0, 0]);
    }

    #[test]
    fn an_area_over_a_stroke_or_an_older_object_over_a_newer_needs_another() {
        // A wipeout made after the lines it hides, then a note over it.
        let painted = [(id(1), false), (id(2), true), (id(3), false)];
        assert_eq!(tiers_of(&painted), [0, 1, 1]);
        // A text brought in front of a line made after it.
        let painted = [(id(2), false), (id(1), false)];
        assert_eq!(tiers_of(&painted), [0, 1]);
    }

    fn moved(all: &[(u64, i16)], chosen: &[u64], placement: Placement) -> Vec<(u64, i16)> {
        let all: Vec<(EntityId, i16)> = all.iter().map(|(raw, tier)| (id(*raw), *tier)).collect();
        let chosen = chosen.iter().map(|raw| id(*raw)).collect();
        let mut moved: Vec<(u64, i16)> = reordered(&all, &chosen, placement)
            .into_iter()
            .map(|(id, tier)| (id.raw(), tier))
            .collect();
        moved.sort_unstable();
        moved
    }

    #[test]
    fn front_and_back_go_past_everything_else_keeping_their_own_order() {
        let all = [(1, 0), (2, 0), (3, 2), (4, 5), (5, -1)];
        assert_eq!(moved(&all, &[1, 3], Placement::Front), [(1, 6), (3, 7)]);
        assert_eq!(moved(&all, &[2, 4], Placement::Back), [(2, -3), (4, -2)]);
        assert!(
            moved(&all, &[4], Placement::Front).is_empty(),
            "already in front"
        );
        assert!(
            moved(&all, &[5], Placement::Back).is_empty(),
            "already at the back"
        );
        assert!(moved(&all, &[1, 2, 3, 4, 5], Placement::Front).is_empty());
        assert!(moved(&all, &[], Placement::Front).is_empty());
    }

    #[test]
    fn above_and_under_go_next_to_the_reference_and_make_room() {
        // Nothing just over tier 0: the chosen go there, nothing else moves.
        let all = [(1, 0), (2, 0), (3, 2), (4, -4)];
        assert_eq!(moved(&all, &[4], Placement::Above(0)), [(4, 1)]);
        // Tier 1 taken: what is over the reference moves up one.
        let all = [(1, 0), (2, 1), (3, 2), (4, -4)];
        assert_eq!(
            moved(&all, &[4], Placement::Above(0)),
            [(2, 2), (3, 3), (4, 1)]
        );
        let all = [(1, 0), (2, -1), (3, 3)];
        assert_eq!(moved(&all, &[3], Placement::Under(0)), [(2, -2), (3, -1)]);
    }

    #[test]
    fn out_of_tiers_everything_is_renumbered_in_the_same_order() {
        let all = [(1, i16::MAX), (2, 0), (3, i16::MIN)];
        assert_eq!(
            moved(&all, &[2], Placement::Front),
            [(1, 0), (2, 1), (3, -1)]
        );
    }

    proptest! {
        /// Whatever sequence a file keeps, the tiers paint it back in that
        /// sequence.
        #[test]
        fn tiers_paint_any_sequence_as_it_was(
            fills in prop::collection::vec(any::<bool>(), 0..40),
            seed in any::<u64>(),
        ) {
            let mut painted: Vec<(EntityId, bool)> = fills
                .iter()
                .enumerate()
                .map(|(n, fill)| (id(n as u64 + 1), *fill))
                .collect();
            // A shuffle, from the seed.
            let mut state = seed | 1;
            for i in (1..painted.len()).rev() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                painted.swap(i, (state % (i as u64 + 1)) as usize);
            }
            let tiers = tiers_of(&painted);
            let mut again: Vec<(EntityId, bool)> = painted.clone();
            let tier_of: std::collections::HashMap<EntityId, i16> =
                painted.iter().zip(&tiers).map(|((id, _), tier)| (*id, *tier)).collect();
            again.sort_by_key(|(id, fill)| paint_key(*id, tier_of[id], *fill));
            prop_assert_eq!(again, painted);
        }
    }
}
