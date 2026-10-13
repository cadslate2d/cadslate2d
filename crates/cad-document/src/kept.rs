//! Layouts kept through a round trip (#184; ADR-0025).
//!
//! A file from another program may carry paper space: layouts, each with
//! a block of entities — viewports, title blocks — the drawing has no
//! entity for. They are not drawn, but they are not lost either: the
//! drawing keeps them as the file's own groups, opaque, and the writer
//! puts them back, with their handles made to fit the file it writes.
//! They are not part of the history: nothing edits them.

/// One group of a DXF record as the file held it: a code and its value.
pub type RawGroup = (i32, String);

/// One entity of paper space as the file held it: its type name and its
/// groups after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawEntity {
    pub kind: String,
    pub groups: Vec<RawGroup>,
}

/// A layout kept whole: its LAYOUT object, the paper-space block it lays
/// out, and that block's entities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeptLayout {
    /// The layout's name, as its tab shows it.
    pub name: String,
    /// The name of its paper-space block: `*Paper_Space`, `*Paper_Space0`, …
    pub block_name: String,
    /// The handle of that block's record in the file, which the LAYOUT
    /// object names and the writer maps to the record it writes.
    pub block_record_handle: String,
    /// The LAYOUT object's groups after its type.
    pub layout: Vec<RawGroup>,
    /// The entities of its block, in order.
    pub entities: Vec<RawEntity>,
}

impl KeptLayout {
    /// Whether the layout has anything on its paper. Every file carries an
    /// empty default layout; that one is kept too, but nothing is said of
    /// it.
    pub fn has_content(&self) -> bool {
        !self.entities.is_empty()
    }
}
