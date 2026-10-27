//! What ADR-0040's generators share: their entities drawn on their own
//! layers — made where the drawing has none of those names, and only those
//! used — in one transaction, kept in a group named after the kind, the
//! first number free, whose description holds what it was drawn from.

use cad_core::EntityId;
use cad_document::{Drawing, Entity, Group, Layer, LayoutId, Properties, TransactionError};

/// Draws `entities`, each with the index of its layer in `layers`, in
/// `space`, as one transaction, in a group `prefix` and a number —
/// `ELEV-1` — described by `description`. Returns what it made and the
/// group's name.
pub fn draw_group(
    drawing: &mut Drawing,
    space: LayoutId,
    layers: &[&str],
    entities: Vec<(usize, Entity)>,
    prefix: &str,
    description: String,
) -> Result<(Vec<EntityId>, String), TransactionError> {
    let found: Vec<_> = layers
        .iter()
        .map(|name| drawing.layers().find(name))
        .collect();
    let taken: Vec<String> = drawing
        .groups()
        .iter()
        .map(|(_, group)| group.name.clone())
        .collect();
    let mut transaction = drawing.transaction().in_space(space);
    let mut layer = vec![None; layers.len()];
    for (i, name) in layers.iter().enumerate() {
        if !entities.iter().any(|(on, _)| *on == i) {
            continue;
        }
        layer[i] = Some(match found[i] {
            Some(id) => id,
            None => transaction.add_layer(Layer::named(*name))?,
        });
    }
    let mut made = Vec::new();
    for (i, entity) in entities {
        let properties = Properties {
            layer: layer[i].expect("made for what is on it"),
            ..Properties::default()
        };
        made.push(transaction.insert_with(entity, properties)?);
    }
    let name = (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|name| !taken.contains(name))
        .expect("a number is free");
    let mut group = Group::named(name.clone(), made.clone());
    group.description = description;
    group.selectable = false;
    transaction.add_group(group)?;
    transaction.commit();
    Ok((made, name))
}
