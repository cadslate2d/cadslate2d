//! Tables: the named things a drawing's entities refer to — layers now,
//! linetypes, text styles, and blocks later (DESIGN §11, ADR-0013).
//!
//! A row is identified by a [`TableId`] that never changes and is never
//! reused, exactly like an entity's ID (§10.2). Entities refer to rows by
//! ID, so renaming a layer changes one row and no entity. Every table
//! starts with one row that is always there — layer `0` is the first of
//! them — so a default reference is always valid.
//!
//! Tables change only through [`TableChange`]s inside a transaction, in the
//! same history as entities (§19.2).

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// The stable identity of a row in the table of `Row`s.
pub struct TableId<Row> {
    raw: u32,
    row: PhantomData<fn() -> Row>,
}

impl<Row> TableId<Row> {
    /// The row every table of this kind starts with and never loses.
    pub const FIRST: Self = Self::from_raw(0);

    pub const fn from_raw(raw: u32) -> Self {
        Self {
            raw,
            row: PhantomData,
        }
    }

    pub const fn raw(self) -> u32 {
        self.raw
    }
}

// Manual impls: derives would put bounds on `Row`, which is only a marker.
impl<Row> Clone for TableId<Row> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Row> Copy for TableId<Row> {}
impl<Row> PartialEq for TableId<Row> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}
impl<Row> Eq for TableId<Row> {}
impl<Row> PartialOrd for TableId<Row> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<Row> Ord for TableId<Row> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}
impl<Row> Hash for TableId<Row> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}
impl<Row> fmt::Debug for TableId<Row> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.raw)
    }
}
impl<Row> fmt::Display for TableId<Row> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

/// A reversible change to one row of a table. The same three shapes as an
/// entity change, and undone the same way.
#[derive(Clone, Debug, PartialEq)]
pub enum TableChange<Row> {
    Insert {
        id: TableId<Row>,
        row: Row,
    },
    Remove {
        id: TableId<Row>,
        row: Row,
    },
    Replace {
        id: TableId<Row>,
        before: Row,
        after: Row,
    },
}

impl<Row: Clone> TableChange<Row> {
    pub fn id(&self) -> TableId<Row> {
        match self {
            TableChange::Insert { id, .. }
            | TableChange::Remove { id, .. }
            | TableChange::Replace { id, .. } => *id,
        }
    }

    /// The change that exactly undoes this one.
    pub fn inverse(&self) -> Self {
        match self.clone() {
            TableChange::Insert { id, row } => TableChange::Remove { id, row },
            TableChange::Remove { id, row } => TableChange::Insert { id, row },
            TableChange::Replace { id, before, after } => TableChange::Replace {
                id,
                before: after,
                after: before,
            },
        }
    }
}

/// The rows of one kind, by stable ID, in the order they were made.
#[derive(Clone, Debug, PartialEq)]
pub struct Table<Row> {
    rows: BTreeMap<u32, Row>,
    /// The next ID to hand out. Monotonic: an ID is never reused, even
    /// after its row is removed or its creation undone.
    next: u32,
}

impl<Row> Table<Row> {
    /// A table holding only its permanent first row.
    pub(crate) fn starting_with(first: Row) -> Self {
        Self {
            rows: BTreeMap::from([(TableId::<Row>::FIRST.raw(), first)]),
            next: 1,
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Never true: the first row is always there.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn contains(&self, id: TableId<Row>) -> bool {
        self.rows.contains_key(&id.raw())
    }

    pub fn get(&self, id: TableId<Row>) -> Option<&Row> {
        self.rows.get(&id.raw())
    }

    /// Every row, in ID order — the order the rows were made in.
    pub fn iter(&self) -> impl Iterator<Item = (TableId<Row>, &Row)> {
        self.rows
            .iter()
            .map(|(raw, row)| (TableId::from_raw(*raw), row))
    }

    /// Hands out a fresh ID. It is consumed even if nothing is stored under it.
    pub(crate) fn allocate(&mut self) -> TableId<Row> {
        let id = TableId::from_raw(self.next);
        self.next += 1;
        id
    }
}

impl<Row: Clone> Table<Row> {
    /// Applies a change that is valid against the table as it stands.
    pub(crate) fn apply(&mut self, change: &TableChange<Row>) {
        match change {
            TableChange::Insert { id, row } => {
                let displaced = self.rows.insert(id.raw(), row.clone());
                debug_assert!(displaced.is_none(), "transactions never insert a live row");
            }
            TableChange::Remove { id, .. } => {
                let removed = self.rows.remove(&id.raw());
                debug_assert!(removed.is_some(), "transactions only remove live rows");
            }
            TableChange::Replace { id, after, .. } => {
                let replaced = self.rows.insert(id.raw(), after.clone());
                debug_assert!(replaced.is_some(), "transactions only replace live rows");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Names = Table<&'static str>;
    type NameId = TableId<&'static str>;

    #[test]
    fn a_table_starts_with_its_permanent_first_row() {
        let table = Names::starting_with("0");
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
        assert_eq!(table.get(NameId::FIRST), Some(&"0"));
        assert!(table.contains(NameId::FIRST));
        assert!(!table.contains(NameId::from_raw(1)));
    }

    #[test]
    fn ids_are_handed_out_in_order_and_never_twice() {
        let mut table = Names::starting_with("0");
        let (a, b) = (table.allocate(), table.allocate());
        assert_eq!((a.raw(), b.raw()), (1, 2));
        // Storing and removing a row does not give its ID back.
        table.apply(&TableChange::Insert {
            id: a,
            row: "walls",
        });
        table.apply(&TableChange::Remove {
            id: a,
            row: "walls",
        });
        assert_eq!(table.allocate().raw(), 3);
    }

    #[test]
    fn changes_apply_and_their_inverses_undo_them() {
        let mut table = Names::starting_with("0");
        let id = table.allocate();
        let original = table.clone();

        let insert = TableChange::Insert { id, row: "walls" };
        table.apply(&insert);
        assert_eq!(table.get(id), Some(&"walls"));

        let rename = TableChange::Replace {
            id,
            before: "walls",
            after: "doors",
        };
        table.apply(&rename);
        assert_eq!(table.get(id), Some(&"doors"));
        assert_eq!(rename.id(), id);

        table.apply(&rename.inverse());
        assert_eq!(table.get(id), Some(&"walls"));
        table.apply(&insert.inverse());
        assert_eq!(table, original);
        assert_eq!(insert.inverse().inverse(), insert);
    }

    #[test]
    fn rows_are_listed_in_the_order_they_were_made() {
        let mut table = Names::starting_with("0");
        let (a, b) = (table.allocate(), table.allocate());
        table.apply(&TableChange::Insert {
            id: b,
            row: "second",
        });
        table.apply(&TableChange::Insert {
            id: a,
            row: "first",
        });
        let rows: Vec<&str> = table.iter().map(|(_, row)| *row).collect();
        assert_eq!(rows, vec!["0", "first", "second"]);
    }

    #[test]
    fn ids_of_different_tables_are_different_types_with_the_same_first() {
        let layer = TableId::<&'static str>::FIRST;
        let other = TableId::<u8>::FIRST;
        assert_eq!(layer.raw(), other.raw());
        assert_eq!(format!("{layer:?} {layer}"), "#0 0");
    }
}
