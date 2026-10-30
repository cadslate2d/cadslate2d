//! A request's requirements, as the model writes them beside its document
//! (#362): each in the request's own words, with a check the engine runs
//! on the drawing as drawn. An answer is held to what was asked item by
//! item by the engine, not by the model's word — fluent output otherwise
//! hides a drawing that does not do what was asked.
//!
//! Lengths are in the document's `units`, as its operations' are.

use serde::{Deserialize, Serialize};

/// One thing a request requires.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    /// The requirement in the request's own words: what the user reads.
    pub says: String,
    pub check: Check,
}

/// What the engine checks the drawing for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    /// The view of this `id` is on the sheet — at `scale`, as a drafter
    /// writes it (`"1:50"`), when given.
    View {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<String>,
    },
    /// A dimension reads `value` — in the view of id `view`, when given.
    Dimension {
        value: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view: Option<String>,
    },
    /// A text says `contains`, case aside — in the view `view`, when given.
    Text {
        contains: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view: Option<String>,
    },
    /// `count` closed outlines `width` × `height`, either way round — closed
    /// polylines and circles, in the view `view` when given: three baffle
    /// blocks, fifteen parcels, four holes.
    Outlines {
        count: u32,
        width: f64,
        height: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view: Option<String>,
    },
    /// The layer `name` has something on it.
    Layer { name: String },
}

/// The JSON Schema of a requirement, for the reply the model writes.
#[cfg(feature = "schema")]
pub fn requirement_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Requirement)).expect("a schema is JSON")
}
