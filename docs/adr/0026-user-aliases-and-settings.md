# ADR-0026: User aliases and settings live in the browser, restored in one call

**Status:** Accepted (2026-10-13)  
**Date:** 2026-10-13  
**Kind:** Design (#186)  
**Design refs:** §7.1, §13, §25, §45; ADR-0002, ADR-0019

## Context

The command aliases were the registry's alone, and the drafting toggles, the drawing unit, the autosave delay, and the theme started the same every time. A drafter's own aliases and the way they leave the toggles had nowhere to go. §25 asks for local-first persistence and §45 that nothing leave the browser; ADR-0019 put the autosave in IndexedDB.

## Decision

1. **The user's aliases are a second table under the built-in one.** `CommandAliases` holds the built-in names and, over them, the user's: an alias resolves through the built-in table first, then the user's, so a user alias can never shadow a built-in name or alias — a line that tries is refused by its number and name, and the rest stand. The text format is one line per alias, `alias, command` (acad.pgp's `*` is accepted, `;` starts a comment), edited through `ALIASEDIT` in the paragraph editor, exactly as MTEXT takes a paragraph. The command needs the registry's table, so `CommandContext` carries it.
2. **The settings are kept by the host, in `localStorage`, as one small JSON record** (`cadslate2d.settings`: the toggles' bits, the unit's suffix, the aliases text, the autosave delay, the theme). IndexedDB holds the autosave because it is bytes and must be durable; a record of five fields that a user types or clicks into being is what `localStorage` is for, and the shell's panel visibility already lives there. A record that does not parse is nothing kept.
3. **Restoring is one coarse call.** On start the host hands the engine the toggles, the unit, and the aliases together (`restore_settings`), never a setting at a time (ADR-0002). The unit goes to a drawing nothing has happened to, silently and outside the history; a drawing with content or history keeps its own. Refused aliases are said in the log.
4. **Keeping is a comparison after every UI refresh.** The engine's part of the settings is in the UI state (`toggles`, `unit`, `user_aliases`); the app compares it with what was last kept and writes only when something differs. The theme and the delay are the shell's and written when set.
5. **The theme is UI state (§7.1).** A `data-theme` attribute on the document selects the token set; the renderer reads the canvas palette again when the theme changes and lays its style table out anew. Rust knows no colours.

## Alternatives considered

- **A `.pgp` file the user opens.** Files are for drawings; a setting that must survive a reload has to be in the browser's storage anyway.
- **Replaying the toggles through `key()` on start.** Would log `<GRID off>` lines the user did not type and be five calls instead of one.
- **Keeping the settings in IndexedDB beside the autosave.** A version bump of the database and asynchronous reads for five fields; nothing gained.

## Consequences

- An alias defined, reloaded, and used; a conflict refused with its reason; the toggles as last left (`web/e2e/settings.spec.ts`, `crates/cad-commands/src/tests/aliases.rs`, `web/src/app/cad-app.test.ts`).
- One new WASM export, `restore_settings`, in the boundary snapshot.
- The settings panel is a dialog over the canvas; it changes the unit and the toggles through the command line and the key actions, never beside them (§15).

## Revisit when

- Settings grow past what a user can read in one record — then a schema with a version, and IndexedDB.
- A second theme is not enough and themes become user-defined.
