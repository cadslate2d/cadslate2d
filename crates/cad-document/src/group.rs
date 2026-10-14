//! Groups (#214): named sets of objects that are picked together.
//!
//! A group is a row of the drawing's group table — a name, a description,
//! whether it is selectable, and its members' IDs — changed in the same
//! transactions and the same history as everything else. It holds no
//! geometry: its members stay where they are, each in its own record, and
//! a member that is removed leaves its groups in the same transaction; a
//! group left with no members goes with it. An unnamed group has a name
//! of the form `*A1`, as other programs give one, which no drafter can
//! type: it is found by its members, not its name.
//!
//! Unlike the other tables, the group table has no permanent first row:
//! a new drawing has no groups.

use cad_core::EntityId;

use crate::names::Named;
use crate::table::{Table, TableChange, TableId};

/// A group of objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// Its name; `*A` and a number for an unnamed group.
    pub name: String,
    /// What the drafter wrote about it; empty by default.
    pub description: String,
    /// Whether picking one member picks the whole group.
    pub selectable: bool,
    /// Its members, in the order they joined. Never empty, never repeated.
    pub members: Vec<EntityId>,
}

impl Group {
    /// The prefix of an unnamed group's name.
    pub const UNNAMED_PREFIX: &'static str = "*A";

    /// A selectable group called `name`, holding `members`.
    pub fn named(name: impl Into<String>, members: Vec<EntityId>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            selectable: true,
            members,
        }
    }

    /// A selectable unnamed group; the transaction names it `*A` and the
    /// first number no group has.
    pub fn unnamed(members: Vec<EntityId>) -> Self {
        Self::named("", members)
    }

    /// Whether this is an unnamed group: one named `*A` and digits.
    pub fn is_unnamed(&self) -> bool {
        unnamed_number(&self.name).is_some()
    }

    /// Whether `id` is a member.
    pub fn contains(&self, id: EntityId) -> bool {
        self.members.contains(&id)
    }
}

/// The number in an unnamed group's name, `*A12` → 12.
pub(crate) fn unnamed_number(name: &str) -> Option<u32> {
    let digits = name.strip_prefix(Group::UNNAMED_PREFIX)?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

impl Named for Group {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type GroupId = TableId<Group>;
pub type GroupTable = Table<Group>;
/// A reversible change to one group: made, removed, or — renamed, given
/// or losing a member — replaced.
pub type GroupChange = TableChange<Group>;

/// A new drawing's groups: none.
pub(crate) fn initial_groups() -> GroupTable {
    Table::empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unnamed_group_is_named_star_a_and_a_number() {
        assert_eq!(unnamed_number("*A1"), Some(1));
        assert_eq!(unnamed_number("*A207"), Some(207));
        for name in ["*A", "*Ax", "A1", "*B1", "door", "*A-1"] {
            assert_eq!(unnamed_number(name), None, "{name}");
        }
        assert!(Group::named("*A3", vec![]).is_unnamed());
        assert!(!Group::named("door", vec![]).is_unnamed());
    }

    #[test]
    fn a_new_drawing_has_no_groups_and_a_group_is_found_by_name() {
        let mut table = initial_groups();
        assert!(table.is_empty());
        let id = table.allocate();
        assert_ne!(id, GroupId::FIRST, "no group takes the first ID");
        table.apply(&TableChange::Insert {
            id,
            row: Group::named("Door", vec![EntityId::from_raw(4)]),
        });
        assert_eq!(table.find("door"), Some(id));
        assert!(table.get(id).unwrap().contains(EntityId::from_raw(4)));
    }
}
