# ADR-0034: A menu item is a macro over the command line, and may pause for the user

**Status:** Accepted (2026-10-18)  
**Date:** 2026-10-18  
**Kind:** Design (#246)  
**Design refs:** §5, §7.1, §13.4, §15; ADR-0002

## Context

DESIGN §5 draws a classic menu bar, and §15 says nothing in the shell may bypass the command architecture. The toolbar already obeys this: a button names a command and answers, and the host types them (`CadApp.run`).

AutoCAD 2007/2008's menus are macros of the same kind. `^C^C_zoom _w` cancels what is running and types `zoom`, then `w`. Many items cannot be written as a fixed list of answers, because the user must answer first:
- Draw › Arc › Start, Center, End is `^C^C_arc \_c`. `\` waits for the start point, since `Center` is offered only after it.
- Draw › Circle › Center, Diameter is `^C^C_circle \_d`.
- Tools › Draw Order › Bring to Front asks for objects first, unless some are selected, then answers `_f`.

## Decision

1. **An item names what it does, and the shell carries it out.** The menu model (`web/src/ui/menu-model.ts`) is data. An item runs a macro, shows or hides a panel of the shell, or opens a dialog of the shell. Whether it is greyed or checked is a pure function of the UI snapshot and the panels shown. A menu has no other logic.
2. **A macro is a command and its steps** (`web/src/app/macro.ts`). `CadApp.runMacro` starts the command, which cancels the active one as typing it does, then types each step. The log shows each step, as if typed. Nothing reaches the drawing except through the command line.
3. **A step may pause for the user,** as AutoCAD's `\` does. There are two kinds:
   - `PAUSE` waits until the command asks something other than what it asked when the macro paused. The user has answered.
   - `SELECTION` waits while the command asks for objects. With a selection already made, the command does not ask, and the macro goes straight on.

   A paused macro is checked each time the UI state is refreshed. It is dropped when its command ends or another starts, so a cancelled or replaced command never receives stray answers (`pauseState`).
4. **Focus stays on the command line.** The menu bar never takes the browser's focus. While its menus have the keyboard — opened by Alt and a letter, by Alt alone, or by the pointer — they take every key ahead of the command line, except the browser's chords. Once they close, keys reach the command line again (#248). The moves are AutoCAD's and Windows':
   - the arrows, Home, and End move, passing over separators and greyed items;
   - a letter runs what it underlines;
   - Esc closes one level;
   - Tab leaves.

   They are a pure function of the menus (`stepMenu`).

## Alternatives considered

- **A Rust-side macro language,** such as AutoCAD's `^C^C`, `\` and `;` in a string. One more grammar in the core, when the host already types answers through the one entry point. The pause needs only what the shell sees: the command's name and its prompt.
- **Items without pauses only.** Half of the Draw menu's cascades would be missing or would behave differently from AutoCAD, defeating the purpose.
- **A pause that waits for one input event.** A selection takes any number of picks, and a typed point and a clicked point arrive by different paths. Waiting for the question to change covers both and needs nothing new from the engine.

## Consequences

- A menu item and the same keys typed are indistinguishable to the core. Undo, the log, and repeat-last-command treat them alike.
- A pause cannot tell apart two answers to the same prompt. `LINE`'s "next point" is asked again after each point, for example. Macros pause only where the question changes, and the contents' test runs every macro without a pause against the real engine.
- The menu bar replaces the header's panel switches. They become checked items of View, and Settings becomes Tools › Options.
