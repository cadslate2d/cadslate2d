# ADR-0010: Grip editing as a command, and what arc grips hold fixed

**Status:** Accepted (2026-10-04)  
**Date:** 2026-10-04  
**Kind:** Evidence (resolves an item DESIGN §16.2 left to M2)  
**Design refs:** §16.2, §16.3, §17, §18.2, §19

## Context

DESIGN §16 fixes the grip kinds and the grips of each entity, and asks for two things to be settled in M2:

1. **How a grip edit runs.** §16.3 says it "should reuse the same transient-edit and transaction machinery as commands".
2. **What arc grips do.** "The exact arc midpoint-edit semantics should match familiar CAD behavior and be validated during M2."

An arc has four grips: start, midpoint on the arc, end, and centre. Three behaviours are in common use for the first three:

| | What the dragged grip does | Held fixed |
| --- | --- | --- |
| **A. Stretch (re-fit)** | The arc is re-fitted through the dragged point | The other two of {start, midpoint, end} |
| **B. Lengthen** | An endpoint slides along the same circle; only its angle changes | Centre, radius, the other end |
| **C. Radius** | The midpoint grip changes the radius | Centre, both angles |

## Decision

### 1. A grip edit is a command

Clicking a grip starts `GripEditCommand`, an ordinary `Command` state machine, with the grip as its argument. It is not registered by name; the session starts it when a click lands on a grip.

It follows that a grip edit has, with no code of its own:

- typed absolute, relative, and polar coordinates, and direct distance — relative to where the grip started;
- object snaps, with the edited entity excluded through `Command::transient_sources` (§18.2);
- ORTHO and POLAR, from where the grip started;
- a transient preview in the overlay, with the document untouched until commit (§17);
- exactly one transaction and one undo step, and Escape to cancel.

The edit itself is a pure function in `cad-document`: `Entity::grip_moved(index, to) -> Option<Entity>`. `None` means the result would not be a valid entity, and the command refuses the point and keeps the grip in hand.

Grip-specific rules in the session:

- A grip under the cursor takes a click before entity picking and box selection. Shift+click remains a selection click.
- A grip can be clicked and then placed with a second click, or pressed, dragged, and released.
- Enter drops the grip at the pointer.
- The entity stays selected after the edit and after undoing it.
- Grips are shown at the idle prompt and during a grip edit, not during other commands, and not when more than 100 entities are selected.

### 2. Arc grips re-fit the arc (behaviour A)

An arc is treated as *the arc through its start, its midpoint, and its end*.

| Grip | Dragging it | Held fixed |
| --- | --- | --- |
| Start or end | Re-fits the arc through the new endpoint | The midpoint and the other end |
| Midpoint | Re-fits the arc through the new midpoint | Both ends |
| Centre | Moves the arc | Radius and both angles |

A position in line with the two fixed points has no arc through it and is refused. Dragging the midpoint across the chord flips the arc to the other side; the result is stored counter-clockwise like every arc, so which end is "start" may swap.

### Comparison with established tools

| Tool | Endpoint grip | Midpoint grip | Centre grip |
| --- | --- | --- | --- |
| AutoCAD (default *Stretch* grip mode) | A — the arc is re-fitted; the other end and the midpoint stay | A — both ends stay | move |
| AutoCAD (grip menu alternatives) | B — *Lengthen* | C — *Radius* | — |
| LibreCAD, QCAD | B — the end slides along the circle | C, where a midpoint grip exists | move |
| **CadSlate2D** | **A** | **A** | **move** |

CadSlate2D positions itself as classic command-driven CAD, and A is the default in the tool that audience knows. A also needs no mode: one rule covers three grips.

**How this was validated.** The comparison is against the documented and widely described behaviour of those tools. It was not checked hands-on in each of them from this development environment. The behaviour of CadSlate2D itself is pinned by tests (`cad-document/src/grips.rs`, `cad-commands/src/tests/grips.rs`). A maintainer with those tools at hand should confirm the table, the AutoCAD rows above all; if it is wrong, this ADR is superseded, not edited.

## Evidence

Grip-dragging a vertex of a polyline (`cargo bench -p cad-benchmarks --bench grips`, native, one pointer frame = session update + overlay description + packing, with every vertex's grip on screen):

| Vertices | Per drag frame | Commit + undo |
| --- | --- | --- |
| 100 | 3.5 µs | 1.5 µs |
| 1,000 | 18 µs | 11 µs |
| 10,000 | 173 µs | 124 µs |

The preview of a vertex drag is only the two segments that meet at the vertex (`Entity::grip_preview`), so the polyline is copied once, at commit. Nearly all of the 173 µs is packing 10,000 grip squares.

## Alternatives considered

- **A separate GripEdit session type beside commands.** Rejected: it would duplicate point acquisition, snapping, previews, cancel, and undo grouping — the very machinery §16.3 asks to reuse.
- **Behaviour B for endpoints** (angle only). Rejected as the default: it cannot move an endpoint to a point off the circle, which is what a drafter dragging an endpoint to a snap point wants. It is the natural candidate for a later grip menu.
- **Behaviour C for the midpoint** (radius only). Rejected as the default for the same reason; also a later grip-menu candidate.
- **A `Radius` grip kind for circles.** Excluded by §16.2: the quadrant grips edit the radius.

## Consequences

- A new entity type gets grip editing by implementing `grips`, `grip_moved`, and — only if it can be very large — `grip_preview`.
- `Command` gained `grip()`. A command that returns a grip keeps the grips on screen and lets Enter commit at the pointer.
- With one mode per grip there is no grip menu yet. Lengthen and Radius, and AutoCAD's Stretch → Move → Rotate cycling on Enter, are not implemented.
- A grip sits at a line's midpoint, where users tend to click to select. A second click there now takes the grip and moves the line — the same as in classic CAD, and Escape puts it back.

## Revisit when

- A maintainer checks the AutoCAD row hands-on and finds a difference.
- A grip menu (Lengthen, Radius, add/remove vertex) is scheduled.
- Polyline arc segments arrive (M3) and vertex grips need bulge-aware semantics.
