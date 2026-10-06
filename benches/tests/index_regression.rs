//! Regression: an index rebuilt from a large drawing must accept later edits.
//!
//! Found by the M0 benchmarks. With rstar 0.13.0, `RTree::bulk_load` of the
//! 100,000-line benchmark drawing produced a tree on which the next insert
//! panicked inside rstar ("This is a bug in rstar"). In the browser that is a
//! crash: open a large file, draw one line. `RTreeIndex::rebuild` now grows
//! the tree by insertion instead (ADR-0007). This test fails if bulk loading
//! is reintroduced without the upstream bug being fixed.

use cad_benchmarks::{Rng, lines, pick_box};
use cad_core::EntityId;
use cad_document::spatial::RTreeIndex;
use cad_document::{Drawing, Entity, LineEntity, SpatialIndex};

const SIZE: usize = 100_000;
const SEED: u64 = 2026;

#[test]
fn a_rebuilt_index_accepts_inserts_and_updates() {
    let entries: Vec<_> = lines(SIZE, SEED)
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            (
                EntityId::from_raw(index as u64 + 1),
                Entity::Line(line).bounds(),
            )
        })
        .collect();
    let mut index = RTreeIndex::new();
    index.rebuild(&mut entries.iter().copied());

    let mut rng = Rng::new(SEED);
    for offset in 0..1000 {
        index.insert(
            EntityId::from_raw((SIZE + offset) as u64 + 1),
            pick_box(&mut rng, SIZE),
        );
    }
    for step in 0..1000 {
        index.update(entries[step * (SIZE / 1000)].0, pick_box(&mut rng, SIZE));
    }
    assert_eq!(index.len(), SIZE + 1000);
}

/// The same failure through the public path a user takes: open, then draw.
#[test]
fn an_opened_drawing_accepts_new_entities() {
    let mut drawing = Drawing::from_entities(lines(SIZE, SEED).into_iter().map(Entity::Line));
    for line in lines(1000, SEED + 1) {
        let mut transaction = drawing.transaction();
        transaction
            .insert(LineEntity::new(line.start, line.end))
            .unwrap();
        transaction.commit();
    }
    assert_eq!(drawing.len(), SIZE + 1000);
    drawing.undo();
    assert_eq!(drawing.len(), SIZE + 999);
}
