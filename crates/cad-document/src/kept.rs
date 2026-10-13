//! What a file said that the drawing does not read, kept as the file's
//! own groups and written back (#184; ADR-0025, narrowed by #191): a
//! layout's `LAYOUT` object, and the records of its paper the drawing
//! has no entity for. They ride on the layout's row (`crate::Layout`).

/// One group of a DXF record as the file held it: a code and its value.
pub type RawGroup = (i32, String);

/// One entity of paper space as the file held it: its type name and its
/// groups after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawEntity {
    pub kind: String,
    pub groups: Vec<RawGroup>,
}
