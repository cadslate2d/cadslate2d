//! GROUP, UNGROUP, and PICKSTYLE (#214): named sets of objects picked
//! together.
//!
//! `GROUP` with a selection already made groups it at once, unnamed;
//! otherwise it offers its options — `Create` (a name, or Enter for an
//! unnamed group, then the objects), `Add` and `Remove` members, `REName`,
//! `Selectable`, `Explode`, and `?` to list. A group is named by typing
//! its name; one that is unnamed, by its `*A` name. `UNGROUP` explodes the
//! groups of what is selected. `PICKSTYLE` sets whether picking a member
//! picks its group, as Ctrl+Shift+A switches it. What the group table
//! allows is decided by `cad-document`; this is the conversation.

use cad_core::EntityId;
use cad_document::{Group, GroupId, TransactionError};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{NOTHING_SELECTED, SELECT_OBJECTS, selection_expected};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const LIST: Keyword = Keyword::new("?");
const CREATE: Keyword = Keyword::new("Create");
const ADD: Keyword = Keyword::new("Add");
const REMOVE: Keyword = Keyword::new("Remove");
const RENAME: Keyword = Keyword::new("REName");
const SELECTABLE: Keyword = Keyword::new("Selectable");
const EXPLODE: Keyword = Keyword::new("Explode");
const OPTIONS: [Keyword; 7] = [LIST, ADD, REMOVE, EXPLODE, RENAME, SELECTABLE, CREATE];

/// "1 object", "3 objects".
fn objects(count: usize) -> String {
    if count == 1 {
        "1 object".to_owned()
    } else {
        format!("{count} objects")
    }
}

/// How a group is named in a message: `"Door"`, or `unnamed group *A1`.
fn describe(group: &Group) -> String {
    if group.is_unnamed() {
        format!("unnamed group {}", group.name)
    } else {
        format!("group \"{}\"", group.name)
    }
}

/// The group a typed name names, or the message that there is none.
fn find(ctx: &CommandContext<'_>, name: &str) -> Result<(GroupId, Group), String> {
    let groups = ctx.drawing.groups();
    groups
        .find(name)
        .and_then(|id| Some((id, groups.get(id)?.clone())))
        .ok_or_else(|| format!("There is no group \"{name}\"."))
}

/// Why the group table refused a change, in the user's terms.
fn explain(ctx: &CommandContext<'_>, error: TransactionError) -> String {
    match error {
        TransactionError::BadGroupName(error) => error.explain("group"),
        TransactionError::DuplicateGroupName(id) => format!(
            "There is already a group \"{}\".",
            ctx.drawing
                .groups()
                .get(id)
                .map_or_else(|| id.to_string(), |group| group.name.clone())
        ),
        TransactionError::EmptyGroup => "A group needs at least one object.".to_owned(),
        other => other.to_string(),
    }
}

/// Makes one change through the group table, or explains the refusal.
fn transact(
    ctx: &mut CommandContext<'_>,
    change: impl FnOnce(&mut cad_document::TransactionBuilder<'_>) -> Result<String, TransactionError>,
) {
    let mut transaction = ctx.transaction();
    match change(&mut transaction) {
        Ok(done) => {
            transaction.commit();
            ctx.message(done);
        }
        Err(error) => {
            drop(transaction);
            let message = explain(ctx, error);
            ctx.message(message);
        }
    }
}

/// The selection, in the order the session holds it.
fn selected(ctx: &CommandContext<'_>) -> Vec<EntityId> {
    ctx.selection.iter().collect()
}

#[derive(Debug)]
enum State {
    Option,
    /// The new group's name; Enter for an unnamed one.
    CreateName,
    /// The objects of the group being made.
    CreateObjects(Option<String>),
    /// Which group, for an option that takes one.
    Which(Keyword),
    /// The objects to add.
    AddObjects(GroupId),
    /// Objects to take out, one pick at a time.
    RemoveObjects(GroupId),
    /// The new name.
    NewName(GroupId),
}

#[derive(Debug)]
pub(crate) struct GroupCommand {
    state: State,
}

impl GroupCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Option,
        }
    }

    fn create(ctx: &mut CommandContext<'_>, name: Option<String>) -> Step {
        let members = selected(ctx);
        if members.is_empty() {
            ctx.message(NOTHING_SELECTED);
            return Step::Finished;
        }
        let count = members.len();
        let group = match name {
            Some(name) => Group::named(name, members),
            None => Group::unnamed(members),
        };
        let mut transaction = ctx.transaction();
        match transaction.add_group(group) {
            Ok(id) => {
                transaction.commit();
                // The table names an unnamed group: say what it called it.
                if let Some(group) = ctx.drawing.groups().get(id).cloned() {
                    ctx.message(format!(
                        "Made the {} of {}.",
                        describe(&group),
                        objects(count)
                    ));
                }
                ctx.selection.clear();
            }
            Err(error) => {
                drop(transaction);
                let message = explain(ctx, error);
                ctx.message(message);
            }
        }
        Step::Finished
    }

    fn list(ctx: &mut CommandContext<'_>) {
        let lines: Vec<String> = ctx
            .drawing
            .groups()
            .iter()
            .map(|(_, group)| {
                let mut line = format!(
                    "  {}: {}",
                    if group.is_unnamed() {
                        format!("{} (unnamed)", group.name)
                    } else {
                        format!("\"{}\"", group.name)
                    },
                    objects(group.members.len())
                );
                if !group.selectable {
                    line.push_str(", not selectable");
                }
                line
            })
            .collect();
        if lines.is_empty() {
            ctx.message("The drawing has no groups.");
        }
        for line in lines {
            ctx.message(line);
        }
    }
}

impl Command for GroupCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            Self::create(ctx, None)
        }
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Option => Prompt::new("Enter a group option").with_default("Create"),
            State::CreateName => Prompt::new("Enter a group name").with_default("unnamed"),
            State::CreateObjects(_) | State::AddObjects(_) => Prompt::new(SELECT_OBJECTS),
            State::Which(_) => Prompt::new("Enter the group's name"),
            State::RemoveObjects(_) => Prompt::new("Select an object to remove from the group"),
            State::NewName(_) => Prompt::new("Enter its new name"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            State::CreateName | State::Which(_) | State::NewName(_) => ExpectedInput::text(),
            State::CreateObjects(_) | State::AddObjects(_) => selection_expected(),
            State::RemoveObjects(_) => ExpectedInput::object(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Option);
        self.state = match (state, input) {
            (State::Option, CommandInput::Keyword(LIST)) => {
                Self::list(ctx);
                State::Option
            }
            (State::Option, CommandInput::Keyword(CREATE) | CommandInput::Accept) => {
                State::CreateName
            }
            (State::Option, CommandInput::Keyword(option)) => State::Which(option),
            (State::CreateName, CommandInput::Text(name)) => State::CreateObjects(Some(name)),
            (State::CreateName, CommandInput::Accept) => State::CreateObjects(None),
            (State::CreateObjects(name), CommandInput::Accept) => return Self::create(ctx, name),
            (State::Which(option), CommandInput::Text(name)) => match find(ctx, &name) {
                Err(message) => {
                    ctx.message(message);
                    State::Which(option)
                }
                Ok((id, group)) => match option {
                    ADD => State::AddObjects(id),
                    REMOVE => State::RemoveObjects(id),
                    RENAME => State::NewName(id),
                    SELECTABLE => {
                        let selectable = !group.selectable;
                        let named = describe(&group);
                        transact(ctx, |transaction| {
                            transaction.replace_group(
                                id,
                                Group {
                                    selectable,
                                    ..group
                                },
                            )?;
                            Ok(if selectable {
                                format!("The {named} is selectable.")
                            } else {
                                format!("The {named} is not selectable.")
                            })
                        });
                        return Step::Finished;
                    }
                    _ => {
                        // Explode: the group goes, its objects stay.
                        let named = describe(&group);
                        let count = group.members.len();
                        transact(ctx, |transaction| {
                            transaction.remove_group(id)?;
                            Ok(format!(
                                "The {named} is exploded: its {} stay.",
                                objects(count)
                            ))
                        });
                        return Step::Finished;
                    }
                },
            },
            (State::Which(_), CommandInput::Accept) => return Step::Finished,
            (State::AddObjects(id), CommandInput::Accept) => {
                let adding = selected(ctx);
                if let Some(mut group) = ctx.drawing.groups().get(id).cloned() {
                    let before = group.members.len();
                    for member in adding {
                        if !group.members.contains(&member) {
                            group.members.push(member);
                        }
                    }
                    let added = group.members.len() - before;
                    let named = describe(&group);
                    transact(ctx, |transaction| {
                        transaction.replace_group(id, group)?;
                        Ok(format!("{} added to the {named}.", objects(added)))
                    });
                    ctx.selection.clear();
                }
                return Step::Finished;
            }
            (State::RemoveObjects(id), CommandInput::Pick { entity, .. }) => {
                if let Some(mut group) = ctx.drawing.groups().get(id).cloned() {
                    if group.contains(entity) {
                        group.members.retain(|member| *member != entity);
                        let named = describe(&group);
                        if group.members.is_empty() {
                            ctx.message(format!(
                                "That is the last object of the {named}: explode it instead."
                            ));
                        } else {
                            let left = group.members.len();
                            transact(ctx, |transaction| {
                                transaction.replace_group(id, group)?;
                                Ok(format!("Removed: the {named} has {} left.", objects(left)))
                            });
                        }
                    } else {
                        ctx.message("That object is not in the group.");
                    }
                }
                State::RemoveObjects(id)
            }
            (State::RemoveObjects(_), CommandInput::Accept) => return Step::Finished,
            (State::NewName(id), CommandInput::Text(name)) => {
                if let Some(group) = ctx.drawing.groups().get(id).cloned() {
                    let named = describe(&group);
                    let renamed = name.clone();
                    transact(ctx, |transaction| {
                        transaction.replace_group(id, Group { name, ..group })?;
                        Ok(format!("The {named} is renamed \"{renamed}\"."))
                    });
                }
                return Step::Finished;
            }
            (State::NewName(_), CommandInput::Accept) => return Step::Finished,
            (state, _) => state,
        };
        Step::Continue
    }
}

/// `UNGROUP`: the groups the selected objects are in, exploded.
#[derive(Debug)]
pub(crate) struct UngroupCommand;

impl UngroupCommand {
    fn ungroup(ctx: &mut CommandContext<'_>) -> Step {
        let picked = selected(ctx);
        let mut groups: Vec<GroupId> = Vec::new();
        for entity in picked {
            for (id, _) in ctx.drawing.groups_of(entity) {
                if !groups.contains(&id) {
                    groups.push(id);
                }
            }
        }
        if groups.is_empty() {
            ctx.message("The selection holds no group.");
            return Step::Finished;
        }
        let count = groups.len();
        transact(ctx, |transaction| {
            for id in groups {
                transaction.remove_group(id)?;
            }
            Ok(if count == 1 {
                "1 group exploded.".to_owned()
            } else {
                format!("{count} groups exploded.")
            })
        });
        Step::Finished
    }
}

impl Command for UngroupCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            Self::ungroup(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        Prompt::new(SELECT_OBJECTS)
    }

    fn expected(&self) -> ExpectedInput {
        selection_expected()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Accept => Self::ungroup(ctx),
            _ => Step::Continue,
        }
    }
}

/// `PICKSTYLE`: 1 picks a member's whole group, 0 picks the one object.
#[derive(Debug)]
pub(crate) struct PickStyleCommand {
    /// Whether group selection was on as the command found it.
    on: bool,
}

impl PickStyleCommand {
    pub(crate) fn new() -> Self {
        Self { on: true }
    }
}

impl Command for PickStyleCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.on = ctx.settings.group_selection;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Enter new value for PICKSTYLE (0 or 1)").with_default(if self.on {
            "1"
        } else {
            "0"
        })
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::text()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let on = match input {
            CommandInput::Text(value) => match value.trim() {
                "0" => false,
                "1" => true,
                _ => {
                    ctx.message("PICKSTYLE is 0 or 1.");
                    return Step::Continue;
                }
            },
            CommandInput::Accept => return Step::Finished,
            _ => return Step::Continue,
        };
        ctx.settings.group_selection = on;
        ctx.message(if on {
            "<Group selection on>"
        } else {
            "<Group selection off>"
        });
        Step::Finished
    }
}
