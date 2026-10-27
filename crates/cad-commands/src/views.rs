//! What ADR-0040's generators share: their entities drawn on their own
//! layers — made where the drawing has none of those names, and only those
//! used — in one transaction, kept in a group named after the kind, the
//! first number free, whose description holds what it was drawn from.

use cad_core::EntityId;
use cad_document::{
    Drawing, Entity, Group, Layer, LayoutId, Linetype, LinetypeDef, Properties, TransactionError,
};

/// A layer a generator draws on, and the built-in linetype it is made
/// with when the drawing has no layer of that name.
#[derive(Clone, Copy, Debug)]
pub struct LayerSpec {
    pub name: &'static str,
    pub linetype: Option<&'static str>,
}

impl LayerSpec {
    pub const fn plain(name: &'static str) -> Self {
        Self {
            name,
            linetype: None,
        }
    }
}

/// Draws `entities`, each with the index of its layer in `layers`, in
/// `space`, as one transaction, in a group `prefix` and a number —
/// `ELEV-1` — described by `description`. Returns what it made and the
/// group's name.
pub fn draw_group(
    drawing: &mut Drawing,
    space: LayoutId,
    layers: &[&'static str],
    entities: Vec<(usize, Entity)>,
    prefix: &str,
    description: String,
) -> Result<(Vec<EntityId>, String), TransactionError> {
    let specs: Vec<LayerSpec> = layers.iter().map(|name| LayerSpec::plain(name)).collect();
    draw_group_styled(drawing, space, &specs, entities, prefix, description, None)
}

/// [`draw_group`], each layer made with its linetype — loaded from the
/// built-in ones when the drawing has none of that name — and `ltscale`,
/// when given, set in the same transaction.
pub fn draw_group_styled(
    drawing: &mut Drawing,
    space: LayoutId,
    layers: &[LayerSpec],
    entities: Vec<(usize, Entity)>,
    prefix: &str,
    description: String,
    ltscale: Option<f64>,
) -> Result<(Vec<EntityId>, String), TransactionError> {
    let found: Vec<_> = layers
        .iter()
        .map(|spec| drawing.layers().find(spec.name))
        .collect();
    let linetypes: Vec<_> = layers
        .iter()
        .map(|spec| spec.linetype.map(|name| drawing.linetypes().find(name)))
        .collect();
    let taken: Vec<String> = drawing
        .groups()
        .iter()
        .map(|(_, group)| group.name.clone())
        .collect();
    let mut transaction = drawing.transaction().in_space(space);
    if let Some(ltscale) = ltscale {
        transaction.set_ltscale(ltscale)?;
    }
    let mut layer = vec![None; layers.len()];
    // Linetypes loaded here, so two layers of one load it once.
    let mut loaded: Vec<(&str, cad_document::LinetypeId)> = Vec::new();
    for (i, spec) in layers.iter().enumerate() {
        if !entities.iter().any(|(on, _)| *on == i) {
            continue;
        }
        layer[i] = Some(match found[i] {
            Some(id) => id,
            None => {
                let mut made = Layer::named(spec.name);
                if let (Some(name), Some(id)) = (spec.linetype, linetypes[i]) {
                    let id = match (id, loaded.iter().find(|(n, _)| *n == name)) {
                        (Some(id), _) | (None, Some(&(_, id))) => id,
                        (None, None) => {
                            let id = transaction.add_linetype(
                                LinetypeDef::built_in_named(name).expect("a built-in linetype"),
                            )?;
                            loaded.push((name, id));
                            id
                        }
                    };
                    made.linetype = Linetype::Named(id);
                }
                transaction.add_layer(made)?
            }
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
