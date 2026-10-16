//! FIND (#216): words found, or replaced, in every kind of text.
//!
//! The words to find, then what to put in their place, or Enter to find
//! only; `Match case` and `Whole words` at the first prompt. It looks in
//! the current space's texts, paragraphs, attribute definitions' defaults,
//! block references' attribute values, leader notes, and dimension texts
//! the drafter wrote. Finding selects what holds the words; replacing
//! changes every one in one transaction. What a locked layer holds is
//! found, never changed, and the message says so.
//!
//! A paragraph's words are its content: the drawing keeps no formatting
//! codes in it (#145), only line breaks, which a match never spans.

use cad_core::EntityId;
use cad_document::Entity;
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const MATCH_CASE: Keyword = Keyword::new("Match case");
const WHOLE_WORDS: Keyword = Keyword::new("Whole words");

/// How words are matched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Matching {
    pub(crate) case: bool,
    pub(crate) whole: bool,
}

/// Whether `c` is part of a word: a letter, a digit, or an underscore.
fn wordy(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The length in bytes of `find` matched at the start of `text`, or `None`.
fn match_at(text: &str, find: &str, case: bool) -> Option<usize> {
    let mut chars = text.chars();
    let mut length = 0;
    for wanted in find.chars() {
        let found = chars.next()?;
        let same = if case {
            found == wanted
        } else {
            found.to_lowercase().eq(wanted.to_lowercase())
        };
        if !same {
            return None;
        }
        length += found.len_utf8();
    }
    Some(length)
}

/// `text` with every match of `find` replaced by `with`, and how many
/// there were. Matches do not overlap, and are taken from the start; a
/// whole-word match has no word character on either side. An empty
/// `find` matches nothing.
pub(crate) fn replace_words(
    text: &str,
    find: &str,
    with: &str,
    matching: Matching,
) -> (String, usize) {
    if find.is_empty() {
        return (text.to_owned(), 0);
    }
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    let mut at = 0;
    while at < text.len() {
        let before = text[..at].chars().next_back();
        let matched = match_at(&text[at..], find, matching.case).filter(|length| {
            !matching.whole
                || (!before.is_some_and(wordy)
                    && !text[at + length..].chars().next().is_some_and(wordy))
        });
        match matched {
            Some(length) => {
                out.push_str(with);
                count += 1;
                at += length;
            }
            None => {
                let c = text[at..].chars().next().expect("inside the text");
                out.push(c);
                at += c.len_utf8();
            }
        }
    }
    (out, count)
}

/// `entity` with `replace` applied to every text it holds, and how many
/// matches there were; `None` for the entity when there were none.
pub(crate) fn replaced_in(
    entity: &Entity,
    replace: &dyn Fn(&str) -> (String, usize),
) -> (Option<Entity>, usize) {
    let mut count = 0;
    let mut swap = |text: &mut String| {
        let (new, found) = replace(text);
        if found > 0 {
            *text = new;
            count += found;
        }
    };
    let mut entity = entity.clone();
    match &mut entity {
        Entity::Text(text) => swap(&mut text.content),
        Entity::MText(text) => swap(&mut text.content),
        Entity::AttributeDefinition(definition) => swap(&mut definition.text.content),
        Entity::BlockRef(reference) => {
            for attribute in &mut reference.attributes {
                swap(&mut attribute.value);
            }
        }
        Entity::Leader(leader) => {
            if let Some(note) = &mut leader.text {
                swap(note);
            }
        }
        Entity::Dimension(dimension) => {
            if let Some(words) = &mut dimension.text_override {
                swap(words);
            }
        }
        Entity::Line(_)
        | Entity::Circle(_)
        | Entity::Arc(_)
        | Entity::Ellipse(_)
        | Entity::Spline(_)
        | Entity::Polyline(_)
        | Entity::Hatch(_)
        | Entity::Viewport(_)
        | Entity::Wipeout(_)
        | Entity::Point(_)
        | Entity::XLine(_)
        | Entity::Ray(_) => {}
    }
    ((count > 0).then_some(entity), count)
}

#[derive(Debug)]
enum State {
    Find,
    With(String),
}

#[derive(Debug)]
pub(crate) struct FindCommand {
    state: State,
    matching: Matching,
}

/// "1 object", "3 objects".
fn objects(count: usize) -> String {
    if count == 1 {
        "1 object".to_owned()
    } else {
        format!("{count} objects")
    }
}

impl FindCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Find,
            matching: Matching::default(),
        }
    }

    /// Finds `find`, and replaces it with `with` when there is one.
    fn run(&self, find: &str, with: Option<&str>, ctx: &mut CommandContext<'_>) -> Step {
        let matching = self.matching;
        let replace = |text: &str| replace_words(text, find, with.unwrap_or(find), matching);
        let found: Vec<(EntityId, Entity, usize)> = ctx
            .drawing
            .shown_in(ctx.space)
            .filter_map(|(id, entity)| match replaced_in(entity, &replace) {
                (Some(changed), count) => Some((id, changed, count)),
                (None, _) => None,
            })
            .collect();
        if found.is_empty() {
            ctx.message(format!("\"{find}\" is not found."));
            return Step::Finished;
        }
        let words: usize = found.iter().map(|(.., count)| count).sum();
        let Some(_) = with else {
            ctx.selection.clear();
            for (id, ..) in &found {
                ctx.selection.add(*id);
            }
            ctx.message(format!(
                "Found {} in {}.",
                if words == 1 {
                    "1 time".to_owned()
                } else {
                    format!("{words} times")
                },
                objects(found.len())
            ));
            return Step::Finished;
        };
        let (locked, open): (Vec<_>, Vec<_>) = found
            .into_iter()
            .partition(|(id, ..)| ctx.drawing.is_locked(*id));
        let changed_words: usize = open.iter().map(|(.., count)| count).sum();
        let changed = open.len();
        if changed > 0 {
            let mut transaction = ctx.transaction();
            for (id, entity, _) in open {
                transaction
                    .replace(id, entity)
                    .expect("live entities, each replaced once");
            }
            transaction.commit();
            ctx.message(format!(
                "Replaced {} in {}.",
                if changed_words == 1 {
                    "1 time".to_owned()
                } else {
                    format!("{changed_words} times")
                },
                objects(changed)
            ));
        }
        match locked.len() {
            0 => {}
            1 => ctx.message("1 object on a locked layer was left as it was."),
            several => ctx.message(format!(
                "{several} objects on locked layers were left as they were."
            )),
        }
        Step::Finished
    }
}

impl Command for FindCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Find => Prompt::new("Text to find"),
            State::With(_) => Prompt::new("Replace with").with_default("find only"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Find => ExpectedInput::text().with_keywords(&[MATCH_CASE, WHOLE_WORDS]),
            State::With(_) => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Find, CommandInput::Keyword(MATCH_CASE)) => {
                self.matching.case = !self.matching.case;
                ctx.message(if self.matching.case {
                    "Case must match."
                } else {
                    "Case need not match."
                });
            }
            (State::Find, CommandInput::Keyword(_)) => {
                self.matching.whole = !self.matching.whole;
                ctx.message(if self.matching.whole {
                    "Whole words only."
                } else {
                    "Parts of words too."
                });
            }
            (State::Find, CommandInput::Text(find)) => self.state = State::With(find),
            (State::Find, CommandInput::Accept) => return Step::Finished,
            (State::With(find), CommandInput::Text(with)) => {
                let find = find.clone();
                return self.run(&find, Some(&with), ctx);
            }
            (State::With(find), CommandInput::Accept) => {
                let find = find.clone();
                return self.run(&find, None, ctx);
            }
            _ => {}
        }
        Step::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_replaced_by_case_and_by_whole_word_as_asked() {
        let any = Matching::default();
        let case = Matching {
            case: true,
            whole: false,
        };
        let whole = Matching {
            case: false,
            whole: true,
        };
        let both = Matching {
            case: true,
            whole: true,
        };
        // (text, find, with, matching, expected, count)
        let table: &[(&str, &str, &str, Matching, &str, usize)] = &[
            ("Door to door", "door", "gate", any, "gate to gate", 2),
            ("Door to door", "door", "gate", case, "Door to gate", 1),
            ("doors and door", "door", "gate", whole, "doors and gate", 1),
            (
                "Door, door_2, DOOR.",
                "door",
                "x",
                whole,
                "x, door_2, x.",
                2,
            ),
            ("Door, door.", "Door", "x", both, "x, door.", 1),
            ("aaa", "aa", "b", any, "ba", 1),
            ("Straße STRASSE", "straße", "road", any, "road STRASSE", 1),
            ("Ölfass ÖLFASS", "ölfass", "drum", any, "drum drum", 2),
            (
                "first\nsecond",
                "first second",
                "x",
                any,
                "first\nsecond",
                0,
            ),
            ("anything", "", "x", any, "anything", 0),
            ("12 mm", "mm", "cm", whole, "12 cm", 1),
        ];
        for (text, find, with, matching, expected, count) in table {
            assert_eq!(
                replace_words(text, find, with, *matching),
                ((*expected).to_owned(), *count),
                "{text:?} / {find:?} / {matching:?}"
            );
        }
    }
}
