# CadSlate2D — Design Specification

**Status:** Implementation Baseline v5, annotated with M0 evidence (2026-10-04)  
**Date:** 2026-08-18  
**Product:** CadSlate2D  
**Tagline:** Classic 2D CAD. In your browser.

---

# 1. Purpose

CadSlate2D is a free, browser-native, local-first 2D CAD application inspired by the simplicity and interaction model of classic AutoCAD-era drafting.

The product thesis is intentionally narrow:

> **Build a precise, fast, command-driven 2D CAD program that runs in the browser without requiring installation, an account, or a cloud workspace.**

CadSlate2D is not intended to reproduce modern AutoCAD feature-for-feature. It is not a BIM suite, a 3D modeler, a cloud collaboration platform, or an AI-first design tool.

The goal is to preserve the parts of classic CAD that made drafting efficient:

- command-line workflows;
- keyboard aliases;
- object snaps;
- grip editing;
- precise coordinate entry;
- layers;
- blocks;
- predictable modify commands;
- model-space-first interaction;
- local files;
- minimal UI ceremony.

The desired reaction from an experienced CAD user is:

> **“This behaves like CAD.”**

---

# 2. Document Structure and Certainty Levels

This document separates **load-bearing decisions** from **provisional future intent**.

Every major design item belongs to one of four categories.

## LOCKED

A decision we intend to build against now.

Changing it requires a concrete implementation reason.

## PROVISIONAL

A likely direction, but not yet validated by implementation.

## EXPERIMENT

A design question that should be resolved by a focused spike or benchmark.

## FUTURE

A possible later feature or direction that must not constrain M0–M2 architecture prematurely.

The project should avoid treating M7 or M8 ideas with the same authority as M0 requirements.

---

# Part I — Locked Product and Architecture

# 3. Product Position

## 3.1 Product wedge

CadSlate2D is not differentiated merely by being CAD in a browser.

Its wedge is the combination of:

- free to use;
- browser-native;
- no installation;
- no mandatory account;
- no mandatory cloud workspace;
- local-first persistence;
- general-purpose 2D drafting;
- classic command-driven interaction;
- grip editing;
- keyboard muscle-memory compatibility;
- deliberate resistance to CAD-suite bloat.

The product should feel closer to:

> **a classic drafting workstation delivered as a website**

than:

> a SaaS design platform with CAD features.

## 3.2 Product principle

> **CAD should be boring.**

The software should help the user:

1. open or create a drawing;
2. draft or modify geometry;
3. save/export the result;
4. leave.

Features that complicate those steps need strong justification.

---

# 4. Locked Non-Goals for v1

CadSlate2D v1 does **not** include:

- 3D modeling;
- solid/surface modeling;
- assemblies;
- CAM;
- simulation;
- BIM;
- semantic building objects;
- project-management dashboards;
- mandatory collaboration;
- mandatory cloud documents;
- account-gated drafting;
- AI-first interaction;
- ribbon-first UI;
- full DWG compatibility;
- user-defined coordinate systems.

## 4.1 Coordinate-system decision

**LOCKED:** v1 uses a single world coordinate system.

No `UCS` command, saved UCS definitions, rotated user coordinate systems, or arbitrary working planes are required for v1.

Viewport transforms remain separate from drawing coordinates.

---

# 5. Default User Interface

The default interface should preserve a classic CAD mental model:

```text
┌ File Edit View Insert Format Tools Draw Dimension Modify ┐
├───────────────────────────────────────────────────────────┤
│ [Line] [Polyline] [Circle] [Arc] ...                     │
├───────────────────────────────────────────────────────────┤
│                                                           │
│                                                           │
│                         +                                 │
│                     MODEL SPACE                           │
│                                                           │
│                                                           │
├───────────────────────────────────────────────────────────┤
│ Command: _                                                │
├───────────────────────────────────────────────────────────┤
│ SNAP GRID ORTHO POLAR OSNAP OTRACK LWT MODEL              │
└───────────────────────────────────────────────────────────┘
```

Principles:

- the canvas owns most of the screen;
- the command line is always available;
- toolbar actions invoke the same command machinery as typed commands;
- keyboard interaction is a first-class product API;
- dialogs should remain narrow and task-specific;
- model space is the primary working surface.

---

# 6. Core Technology Stack

## LOCKED

```text
Frontend
  TypeScript
  React
  Vite

CAD core
  Rust
  WebAssembly
  wasm-bindgen

Rendering
  Rust prepares committed RenderScene data
  Rust prepares transient OverlayFrame data
  TypeScript owns WebGL2 context and issues GPU draw calls

Persistence
  local autosave/recovery
  browser import/export

Deployment
  static hosting
  no required backend
```

React owns UI presentation.

Rust owns authoritative CAD state and decides **what** should be rendered.

TypeScript owns browser graphics resources and decides **how prepared GPU buffers are submitted** to WebGL2.

Committed drawing geometry and transient interaction geometry have separate render lifetimes.

The browser/WASM boundary must remain coarse.

---

# 7. State Ownership

## 7.1 TypeScript owns browser/UI state

Examples:

- open dialogs;
- toolbar visibility;
- panel sizes;
- menu state;
- theme;
- recent-file presentation;
- browser capability indicators;
- WebGL2 context;
- GPU buffers;
- shader programs;
- browser graphics resource lifetime.

## 7.2 Rust owns CAD state

Examples:

- entities;
- layers;
- blocks;
- styles;
- command state;
- selection model;
- grips;
- object snaps;
- transient edit sessions;
- drawing settings;
- spatial index;
- undo/redo history;
- DXF document state;
- viewport-independent render preparation.

There must not be a second full drawing representation in React.

---

# 8. High-Level Architecture

```text
┌─────────────────────────────────────────────┐
│           Browser / TypeScript              │
│                                             │
│ React UI                                    │
│ menus / toolbars / dialogs                  │
│ command-line presentation                   │
│ pointer + keyboard event routing            │
│ file interaction                            │
│ WebGL2 context + GPU submission             │
└─────────────────────┬───────────────────────┘
                      │
                 coarse WASM API
                      │
┌─────────────────────▼───────────────────────┐
│          Interactive Rust/WASM Core         │
│                                             │
│ cad-input                                   │
│ cad-commands                                │
│ cad-document                                │
│ cad-geometry                                │
│ selection + grips                           │
│ object snaps                                │
│ spatial index                               │
│ transactions / undo                         │
│ transient edit sessions                     │
│ DXF import/export                           │
│ committed + overlay render preparation      │
└───────────────┬──────────────────┬──────────┘
                │                  │
          RenderScene         OverlayFrame
        committed/stable     transient/small
                │                  │
                └────────┬─────────┘
                         ▼
┌─────────────────────────────────────────────┐
│            TypeScript GPU Adapter           │
│                                             │
│ committed GPU buffers                       │
│ transient/overlay GPU buffers               │
│ buffer upload / reuse                       │
│ shader selection                            │
│ draw calls                                  │
│ WebGL2 resource ownership                   │
└─────────────────────┬───────────────────────┘
                      │
                      ▼
                   WebGL2
```

Future workers must not change ownership of the authoritative interactive document.

---

# 9. Repository Structure

```text
cadslate2d/
├── Cargo.toml
├── crates/
│   ├── cad-core/
│   ├── cad-input/
│   ├── cad-geometry/
│   ├── cad-document/
│   ├── cad-commands/
│   ├── cad-dxf/
│   ├── cad-render/
│   └── cad-wasm/
│
├── web/
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
│       ├── cad/
│       ├── renderer/
│       │   ├── webgl/
│       │   └── render-frame.ts
│       └── ui/
│
├── test-data/
│   ├── dxf/
│   ├── geometry/
│   └── screenshots/
│
├── benches/
│   └── generated-drawings/
│
└── docs/
    ├── DESIGN.md
    ├── roadmap.md
    └── design/
        ├── input.md
        ├── geometry.md
        ├── rendering.md
        ├── dxf.md
        └── future.md
```

This document is `docs/DESIGN.md`. Decisions are recorded in `docs/adr/`, milestone reviews in `docs/reviews/`.

Detailed subsystem documents should be written only when their milestone approaches.

---

# 10. Entity Identity and Storage

## 10.1 Logical identity

**LOCKED:**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EntityId(u64);
```

Rules:

- allocated monotonically;
- never reused during the lifetime of the drawing;
- deletion does not recycle the logical ID;
- undo of deletion restores the same `EntityId`;
- redo of creation restores the same `EntityId`;
- DXF handles are mapped separately.

Logical identity is stable.

Physical storage is not.

## 10.2 ID counter semantics

**LOCKED:**

`next_entity_id` is monotonic for the lifetime of the in-memory drawing.

It never decrements.

Example:

```text
Create A → EntityId(10)
Create B → EntityId(11)
Undo B
Create C → EntityId(12)
```

`EntityId(11)` is not reused for C.

Undo of a create removes the entity but does not roll back the allocator.

Redo re-inserts the entity using the `EntityId` already stored in the transaction.

If the user performs new work after undoing, the redo branch is discarded according to standard linear-history semantics.

Sparse IDs are acceptable.

## 10.3 Physical storage

**LOCKED CONCEPT, IMPLEMENTATION PROVISIONAL:**

CadSlate2D separates:

```text
EntityId
   stable semantic identity

EntitySlot
   mutable physical storage location
```

Conceptually:

```text
EntityId(734)
     │
     ▼
id_to_slot
     │
     ▼
dense storage slot 91
```

A deleted entity may free slot 91 for reuse, while `EntityId(734)` itself is never reused.

Undo may restore entity 734 into a different storage slot.

The concrete dense-store implementation remains open until benchmarked.

> **Resolved in M0:** dense vector with an ID-to-slot map — [ADR-0006](adr/0006-entity-store.md).

This permits:

- stable references;
- efficient undo;
- dense iteration for render preparation;
- storage compaction;
- independent DXF handle mapping.

---

# 11. Semantic Document Model

The drawing model should preserve CAD semantics.

Conceptually:

```rust
pub struct Drawing {
    entities: EntityStore,
    layers: LayerTable,
    blocks: BlockTable,
    linetypes: LinetypeTable,
    text_styles: TextStyleTable,
    dimension_styles: DimensionStyleTable,
    settings: DrawingSettings,
    spatial_index: SpatialIndex,
    history: History,
    next_entity_id: u64,
}
```

Initial semantic entity types:

```rust
pub enum Entity {
    Line(LineEntity),
    Polyline(PolylineEntity),
    Circle(CircleEntity),
    Arc(ArcEntity),
    Point(PointEntity),
    Text(TextEntity),
    MText(MTextEntity),
    Insert(BlockReferenceEntity),
    Dimension(DimensionEntity),
    Hatch(HatchEntity),
}
```

> **Resolved in M4:** an entity is stored as its geometry with its properties — layer, colour, linetype, lineweight — beside it; tables hold rows under stable IDs, and entities refer to rows by ID. One `Change` enum covers entities and table rows — [ADR-0013](adr/0013-entity-properties-and-tables.md).

A circle remains a circle in the document.

The renderer may approximate it.

The document must not.

---

# 12. Render Boundary and Lifetime Model

The rendering boundary is load-bearing and resolved before M0.

## 12.1 Ownership rule

**LOCKED:**

Rust does not own the WebGL2 context and does not issue per-draw WebGL calls through `web-sys`.

Instead:

```text
Rust cad-render
     ↓
RenderScene + OverlayFrame
     ↓
coarse WASM handoff
     ↓
TypeScript renderer adapter
     ↓
WebGL2 uploads + draw calls
```

Rust determines:

- visible CAD primitives;
- stroke geometry;
- colors/styles;
- selection state;
- transient previews;
- snap/grip overlays;
- draw ordering;
- batch grouping.

TypeScript determines:

- GPU buffer creation/reuse;
- shader compilation/linking;
- WebGL state;
- actual draw-call submission;
- graphics resource lifetime.

This preserves the coarse WASM-boundary rule.

## 12.2 Two render lifetimes

**LOCKED:**

Committed drawing geometry and transient interaction geometry are transferred, cached, and uploaded separately.

### `RenderScene`

Represents stable geometry derived from the canonical document.

It changes when canonical drawing state changes.

Examples:

- committed lines;
- committed polylines;
- committed circles/arcs once their render strategy is chosen;
- committed text;
- committed hatch;
- persistent entity styling.

Conceptually:

```rust
pub struct RenderScene {
    pub revision: SceneRevision,
    pub geometry: PackedGeometry,
    pub batches: Vec<DrawBatch>,
}
```

### `OverlayFrame`

Represents short-lived interaction and presentation geometry.

It may change on every pointer frame.

Examples:

- rubber-band LINE preview;
- MOVE/COPY/ROTATE previews;
- grip-edit preview;
- hover highlight;
- selection highlight where implemented as overlay geometry;
- snap marker;
- grips;
- crosshair or command-construction geometry where appropriate.

Conceptually:

```rust
pub struct OverlayFrame {
    pub geometry: PackedGeometry,
    pub batches: Vec<DrawBatch>,
}
```

The exact packed-buffer representation is PROVISIONAL.

The lifetime split is not.

> **Extended in M4:** committed geometry is drawn in the colours of its layers. Each committed instance carries a style index into a small style table that has its own revision, so recolouring a layer uploads the table and no geometry — [ADR-0014](adr/0014-colours-across-the-render-boundary.md).

> **Resolved in M2:** the reference grid depends on the view (which lines are visible, and how many), so it is `OverlayFrame` content, not a third lifetime. Its batches carry their own style roles, and the adapter draws those beneath the committed scene; everything else in the overlay is drawn on top. The grid shows the set spacing times 1, 2, 5, 10, … so that lines stay at least 12 CSS pixels apart.

## 12.3 Scene revision rule

**LOCKED:**

A canonical document mutation increments or otherwise changes the committed scene revision.

TypeScript only uploads a new committed `RenderScene` when the revision changes.

Pointer movement during an unchanged transient interaction must not force regeneration or re-upload of unchanged committed drawing geometry.

Conceptually:

```text
canonical Drawing changes
        ↓
scene revision changes
        ↓
new RenderScene
        ↓
TS refreshes committed GPU buffers

pointer moves
        ↓
transient state changes
        ↓
new OverlayFrame only
        ↓
TS refreshes small overlay GPU buffers
```

This is required for large-drawing interaction performance.

> **Refined in M7:** a new `RenderScene` is not a whole one. The scene is packed in chunks of entity IDs; the drawing journals what each change touched, the chunks holding it are re-packed in place, and TypeScript uploads only the instance ranges that moved — [ADR-0022](adr/0022-scene-chunks.md). The rule stands: nothing committed changes without a change to the drawing.

## 12.4 WASM transfer rule

The important invariant is:

> **Render preparation crosses WASM in batches, never entity-by-entity and never GL-call-by-GL-call.**

A normal interaction frame should resemble:

```ts
if (scene.revision !== renderer.sceneRevision) {
    renderer.updateScene(scene);
}

renderer.updateOverlay(overlay);
renderer.draw();
```

rather than transferring the full drawing on every pointer move.

## 12.5 Curve representation remains open

**EXPERIMENT:**

The locked render boundary does not define curve-specific fields such as `curve_instances`.

Circle and arc representation remains intentionally unspecified until the M1 renderer experiment chooses among approaches such as:

- zoom-adaptive tessellated triangles;
- analytic shader/SDF primitives;
- a hybrid.

> **Resolved in M1:** analytic arc instances drawn under a covering strip — [ADR-0009](adr/0009-curve-rendering.md). (§23.3 says "M0 should evaluate"; the experiment ran at the start of M1 because §27 excludes circles and arcs from M0.)

A green M0 validates the committed/transient rendering skeleton for line geometry only.

It does not validate the eventual curve protocol.

## 12.6 Buffer lifetime and transport

**PROVISIONAL:**

Prefer reusable buffers and stable capacity to avoid reallocating large arrays every frame.

M0 should measure:

- allocation behavior;
- copy overhead;
- typed-array views vs copied arrays;
- committed-buffer reuse;
- transient-buffer churn;
- GPU upload cost.

The performance data should guide the final packed-buffer transport.

> **Resolved in M0:** typed-array views, one 16-byte instance per segment, reused GPU buffers — [ADR-0008](adr/0008-render-buffer-transport.md). View ownership and the stroke model are in [ADR-0003](adr/0003-view-ownership-and-viewport-independent-scene.md).

---

# 13. Input and Command System

The command system is a product-defining subsystem.

## 13.1 Dedicated crate

```text
cad-input
```

Responsibilities:

- tokenization;
- command aliases;
- coordinate parsing;
- keyword parsing;
- scalar parsing;
- angle parsing;
- unit parsing;
- direct-distance semantics;
- command history;
- parser diagnostics.

## 13.2 Input grammar

Conceptually:

```text
Input
├── CommandName
├── Keyword
├── Point
│   ├── AbsoluteCartesian
│   ├── RelativeCartesian
│   └── RelativePolar
├── Distance
├── Angle
├── EntitySelection
├── Text
├── Accept
└── Cancel
```

Examples:

```text
L             command alias
LINE          command name
100,200       absolute point
@500,0        relative Cartesian point
@500<45       relative polar point
500           scalar or direct-distance input
C             command or keyword, depending on context
```

## 13.3 Context-sensitive input

The active command state defines the expected input.

Conceptually:

```rust
pub enum ExpectedInput {
    Point,
    Distance,
    Angle,
    Entity,
    SelectionSet,
    Keyword(&'static [Keyword]),
    Text,
}
```

If the current prompt is:

```text
Specify next point or [Close/Undo]:
```

then `C` resolves to `Close`.

At the idle command prompt, `C` may resolve to `CIRCLE`.

This must be deterministic and thoroughly tested.

## 13.4 Keyboard semantics

Initial behavior:

```text
Enter      accept / confirm / default
Space      same as Enter
Escape     cancel active operation
Escape×2   return to idle and clear selection
Up         previous command/input history
Down       next command/input history

F3         OSNAP
F7         GRID
F8         ORTHO
F10        POLAR
```

> **Resolved in M2:** a pointer-derived point is decided in a fixed order — object snap, then ORTHO or POLAR, then the free cursor — and typed coordinates bypass all three. ORTHO and POLAR are mutually exclusive: switching one on switches the other off. OSNAP and GRID are on in a new session; ORTHO and POLAR are off. POLAR locks within 8 CSS pixels of a tracked direction; its increment is set with `POLARANG` (90° at start). The readout is shown in the status bar, since text in the drawing area arrives with M4.

Right-click-as-Enter is PROVISIONAL and may become a preference.

## 13.5 Dynamic input

Floating input boxes near the cursor are not required for M0–M2.

The command line is canonical.

Dynamic input may be layered on later without replacing the parser.

---

# 14. Coordinate Entry

Required early syntax:

```text
100,200       absolute Cartesian
@500,0        relative Cartesian
@500<45       relative polar
500           direct distance when context permits
```

Internal geometry is unit-independent.

Initial display/unit support may include:

- unitless;
- mm;
- cm;
- m;
- inches;
- feet.

Architectural feet-inch parsing is FUTURE but the parser must remain extensible enough to support it.

> **Resolved in M4:** the unit list is confirmed as unitless (the default), millimetres, centimetres, metres, inches, and feet. The drawing unit is a drawing setting, changed with `UNITS` in a transaction. It rescales nothing; it is what a typed length with a suffix — `mm`, `cm`, `m`, `in`, `ft` — is converted into. A bare number is so many drawing units, as before; a suffix in a unitless drawing is refused, not guessed at. Angles and factors take no unit. The suffix is the letters that end a number, which leaves the marks of feet-and-inches input free.

---

# 15. Command State Machines

Commands should be explicit state machines.

Example `LINE`:

```text
Idle
 ↓
AwaitFirstPoint
 ↓
AwaitNextPoint
 ↓
AwaitNextPoint
 ↓
Complete
```

Conceptually:

```rust
enum LineState {
    AwaitFirstPoint,
    AwaitNextPoint {
        previous: Point2,
        created: Vec<EntityId>,
    },
}
```

Commands consume typed or pointer input through the same state machine.

Toolbar buttons must not bypass the command architecture.

---

# 16. Selection and Grip Editing

Grip editing is a core v1 interaction requirement.

## 16.1 Selection

Initial selection behavior:

- click;
- multiple selection;
- shift add/remove;
- left-to-right window selection;
- right-to-left crossing selection;
- Escape clears selection.

## 16.2 Grip model

Selected entities expose editable grips.

Conceptually:

```rust
pub struct Grip {
    pub id: GripId,
    pub position: Point2,
    pub kind: GripKind,
}
```

Initial grip kinds:

```rust
pub enum GripKind {
    Endpoint,
    Midpoint,
    Center,
    Vertex,
    Quadrant,
}
```

There is no separate circle `Radius` grip kind.

For circles, quadrant grips are the radius-editing grips.

### Line grips

- endpoint;
- midpoint;
- endpoint.

### Circle grips

- center;
- four quadrant grips.

Dragging a quadrant grip changes the circle radius.

### Arc grips

- start endpoint;
- midpoint on arc;
- end endpoint;
- center.

The exact arc midpoint-edit semantics should match familiar CAD behavior and be validated during M2.

> **Resolved in M2:** dragging an arc's start, midpoint, or end re-fits the arc through the dragged point and the other two; the centre grip moves it. A grip edit runs as a command — [ADR-0010](adr/0010-grip-editing-and-arc-grip-semantics.md).

### Polyline grips

- vertex grips initially;
- segment midpoint grips later if useful.

## 16.3 Grip edit flow

```text
Select entity
      ↓
display grips
      ↓
activate grip
      ↓
start transient GripEdit session
      ↓
pointer move / coordinate input
      ↓
OSNAP against eligible committed geometry
      ↓
preview
      ↓
commit
      ↓
one transaction
```

Grip editing should reuse the same transient-edit and transaction machinery as commands.

---

# 17. Transient Edit Sessions

Interactive pointer movement must not mutate the canonical document on every frame.

Example `MOVE`:

```text
committed entity
      +
transient transform
      ↓
preview
```

Only on click/Enter:

```text
commit
  ↓
transaction
  ↓
document mutation
  ↓
spatial-index update
  ↓
undo record
```

Cancel discards the transient state.

The renderer mirrors the same lifecycle:

```text
canonical document      → RenderScene
transient edit session  → OverlayFrame
```

This applies to:

- LINE rubber-band preview;
- MOVE;
- COPY preview;
- ROTATE;
- SCALE;
- MIRROR;
- grip editing;
- circle/arc preview;
- OFFSET preview;
- other interactive modifications.

---

# 18. Object Snapping

Object snapping is part of the geometry/interaction core.

Initial modes:

- endpoint;
- midpoint;
- center;
- quadrant;
- intersection;
- perpendicular;
- tangent;
- nearest.

## 18.1 Snap pipeline

```text
cursor screen position
      ↓
world coordinate
      ↓
pixel snap radius → world search bounds
      ↓
spatial-index query
      ↓
candidate entities
      ↓
exact candidate snap points
      ↓
rank by mode + screen distance
      ↓
display marker
      ↓
commit point
```

> **Resolved in M2:** `cad_commands::snap`. The aperture is 10 CSS pixels. The closest snap point within it wins; ties go to Endpoint, Intersection, Midpoint, Center, Quadrant, in that order. Nearest is a fallback, taken only when no other snap is in reach. Hovering a circle or arc offers its centre, ranked behind any snap point actually under the cursor. Intersections are searched among at most 24 candidate entities. The snap is derived on demand from cursor, view, and drawing, so the marker and the committed point cannot disagree.
>
> **Extended in M3:** Perpendicular and Tangent, constructed from the command's base point and offered while the cursor is on the object they land on. Candidates rank in tiers: a snap point under the cursor, then a hovered construction, then a hovered centre, then Nearest; within a tier the closest to the cursor wins.

## 18.2 Transient-edit exclusion rule

**LOCKED:**

During a transient edit, source entities being modified must not be offered as snap targets at their old committed positions.

Conceptually:

```text
snap candidates =
    committed indexed geometry
  - transient source EntityIds
  + explicitly eligible command construction geometry
```

This prevents a moving endpoint from snapping back to its own stale location.

## 18.3 Snap tolerance

Snap radius is a display/picking tolerance, usually defined in pixels.

It must remain separate from geometric equality tolerances.

---

# 19. Transactions and Undo/Redo

All persisted drawing mutations must participate in one transaction system.

## 19.1 General rule

> **If a change affects the saved drawing, it must be representable inside a transaction.**

This includes:

- entities;
- layers;
- blocks;
- linetypes;
- text styles;
- dimension styles;
- drawing settings.

It does not include:

- viewport;
- open UI panels;
- theme;
- hover state.

## 19.2 Generalized change model

Conceptually:

```rust
pub enum Change {
    Entity(EntityChange),
    Layer(LayerChange),
    Block(BlockChange),
    Linetype(LinetypeChange),
    TextStyle(TextStyleChange),
    DimensionStyle(DimensionStyleChange),
    DrawingSettings(SettingsChange),
}
```

Example:

```rust
pub enum EntityChange {
    Insert {
        id: EntityId,
        entity: SharedEntity,
    },
    Remove {
        id: EntityId,
        entity: SharedEntity,
    },
    Replace {
        id: EntityId,
        before: SharedEntity,
        after: SharedEntity,
    },
}
```

The exact representation may evolve, but there must not be separate undo stacks for entities, layers, blocks, or styles.

## 19.3 Linear history rule

**LOCKED:**

Undo/redo is a standard linear history.

If the user:

```text
A
B
C
Undo C
Undo B
Create D
```

then the redo branch containing B/C is discarded.

This rule combines with monotonic entity IDs: abandoned IDs remain unused rather than being recycled.

## 19.4 Large entities

Dragging or editing a very large polyline must not produce dozens of full copies.

Rules:

- transient preview changes are not history;
- one committed gesture produces one transaction;
- shared immutable payloads or operation-specific deltas may be used where beneficial.

## 19.5 Derived state is not undo history

The spatial index and render caches are derived state.

Undo changes the canonical document and then reconciles or rebuilds affected derived state.

The history system must not store R-tree implementation details or renderer cache internals.

## 19.6 Block redefinition and derived invalidation

**LOCKED PRINCIPLE:**

Redefining a block is one semantic `BlockChange`.

It must not be expanded into a transaction containing one `EntityChange` per insert.

However, committing, undoing, or redoing a block redefinition may invalidate derived state for many block references.

For every affected `INSERT`, the system may need to recompute:

- world-space bounds;
- spatial-index entries;
- render cache entries;
- selection/hit-test geometry.

Conceptually:

```text
BlockChange
    ↓
BlockTable updated
    ↓
find dependent INSERT references
    ↓
invalidate/recompute derived bounds + render data
```

This preserves semantic undo while allowing large derived-state invalidation.

> **Resolved in M4:** a block definition is a row of the block table holding entity records of its own, behind an `Arc`; a reference is an entity holding the row's ID, an insertion point, a uniform scale, a rotation, and a mirror flag, and *no* geometry of its own — every question about its geometry is answered through the table. So redefining a block is one `TableChange::Replace` on one row, however many references place it; `Drawing::apply` then re-indexes every reference that reaches the row, directly or through other blocks, and reports them as replaced, and the scene is rebuilt from the shown revision. A redefinition of a block with ten thousand references is one change in history and ten thousand index updates — [ADR-0017](adr/0017-blocks.md).

## 19.7 Selection reconciliation

Selection is interaction state, but undo/redo should behave predictably.

### Undo create

- created entity disappears;
- it is removed from selection.

### Undo erase

- erased entity is restored with the same `EntityId`;
- restored entity becomes selected.

### Undo modify

- previous geometry is restored;
- affected entity remains selected when possible.

### Redo

- selection should continue following the affected working subject when it still exists.

The transaction executor may return a selection-reconciliation hint without storing selection inside persisted document history.

---

# 20. Spatial Index

Use an abstract spatial index for:

- object snaps;
- hover;
- selection;
- crossing/window selection;
- viewport culling;
- trim/extend candidate discovery.

Conceptual interface:

```rust
trait SpatialIndex {
    fn insert(&mut self, id: EntityId, bounds: Aabb);
    fn remove(&mut self, id: EntityId);
    fn update(&mut self, id: EntityId, bounds: Aabb);
    fn query(&self, bounds: Aabb) -> Vec<EntityId>;
    fn rebuild(&mut self, drawing: &Drawing);
}
```

An R-tree implementation is PROVISIONAL.

> **Resolved for M0–M1:** rstar R*-tree grown by insertion only, after a crash was found in its bulk loader — [ADR-0007](adr/0007-spatial-index.md). To be re-measured with M2 editing workloads.

Real editing workloads must be benchmarked.

Transient previews should avoid continuous index churn.

Periodic rebuild-on-idle remains an implementation option if incremental updates degrade performance.

---

# 21. Geometry Policy

Document geometry uses `f64`.

Geometry robustness is a first-class risk.

## 21.1 Tolerance classes

The project must distinguish:

```text
geometric linear tolerance
angular tolerance
parametric tolerance
display/picking tolerance
file-import tolerance
```

Examples:

```text
point coincidence       geometric linear
zero-length segment     geometric linear
parallelism             angular
point-on-curve           geometric distance
trim parameter equality parametric
orientation              robust predicate
OSNAP radius             display/picking
```

No global magic `EPSILON` should be used indiscriminately.

Tolerance values and semantics must be centralized and documented.

> **Extended in M3:** the geometric linear tolerance is 10⁻⁹ near the origin and widens to a few spacings of the floating-point numbers where coordinates are too large to be told apart that finely — [ADR-0012](adr/0012-linear-tolerance-far-from-the-origin.md). Intersections decide whether a computed point is on a segment or an arc by its position along the curve, not by measuring its distance from it.

## 21.2 Robust predicates

Orientation and similar topological decisions should use robust predicates where appropriate.

## 21.3 Regression rule

Every fixed geometry bug should add a permanent regression test.

---

# 22. Geometry Complexity Classes

The roadmap must distinguish transformations from hard computational geometry.

## Straightforward transforms

- MOVE;
- COPY;
- ROTATE;
- SCALE;
- MIRROR;
- ERASE.

## Narrow early geometry

- offset line;
- offset circle;
- simple trim against one cutting edge;
- simple extend;
- line-line fillet;
- chamfer.

## Dedicated robust-geometry work

- polyline offset;
- arc/bulge offset;
- self-intersection cleanup;
- multi-boundary trim;
- mixed-entity trim;
- arbitrary fillets;
- JOIN/BREAK across mixed geometry;
- complex STRETCH behavior.

These must not be hidden inside one “modify core” milestone.

---

# 23. Rendering Requirements

The document model is not the renderer.

Rendering pipeline:

```text
semantic document ─────────→ committed RenderScene ──┐
                                                    │
transient edit / overlays ─→ OverlayFrame ──────────┤
                                                    ▼
                                           TypeScript GPU adapter
                                                    ↓
                                                 WebGL2
```

## 23.1 Stroke rendering

CadSlate2D must not depend on platform-native wide `GL_LINES`.

> **Resolved in M6 (hatch):** a filled region is triangle instances — six floats, a style index each, no path distance — in a third committed buffer beside segments and arcs, cut by ear clipping from the boundary rings with the holes bridged in; the adapter draws fills before strokes. A fill of lines would depend on the zoom, which committed geometry may not — [ADR-0021](adr/0021-filled-regions.md).

CAD strokes should be expanded into triangle geometry to support:

- screen-space lineweight;
- correct joins;
- caps;
- miter limits;
- antialiasing.

## 23.2 Linetypes

Linetypes must remain stable across entity segments and zoom.

Render data should carry accumulated path distance where required.

> **Resolved in M4 (linetypes):** every committed instance carries the distance along its entity's path at which it begins — accumulated through a polyline's vertices by chord and by arc length, with the direction the path runs round an arc — and the fragment shader draws each dash as a capsule from that and the pattern. Patterns are dash and gap lengths in drawing units, multiplied by `LTSCALE`, and reach the shader as a small table beside the style table; the overlay is always continuous; a period under four pixels is drawn solid — [ADR-0015](adr/0015-linetype-rendering.md). The linetype table has `Continuous` as its permanent first row and loads the standard patterns by name; a linetype in use by an object, a layer, or the setting for new objects cannot be removed.

## 23.3 Curve rendering — EXPERIMENT

M0 should evaluate two approaches for circles/arcs when the renderer spike is performed.

### A. Zoom-adaptive tessellation

```text
semantic curve
    ↓
viewport scale
    ↓
screen-space chord-error target
    ↓
segments
```

If cached by zoom/tolerance bucket, use hysteresis to avoid cache thrash.

### B. Analytic shader/SDF rendering

Evaluate analytic circles/arcs for:

- visual quality;
- zoom stability;
- lineweight behavior;
- linetype compatibility;
- selection rendering;
- GPU/browser portability;
- implementation complexity.

**No curve-rendering strategy is locked before this experiment.**

## 23.4 Large-coordinate precision

Document coordinates remain `f64`.

GPU geometry may use camera-relative `f32` coordinates.

## 23.5 Text rendering

Text is a separate rendering subsystem.

Possible future approach:

```text
CAD text
   ↓
font/style resolution
   ↓
glyph metrics
   ↓
glyph atlas / SDF / MSDF
   ↓
GPU quads
```

Exact implementation is PROVISIONAL.

> **Resolved in M4:** text is drawn with a stroke font — Hershey Simplex, shipped in the `cad-text` crate — whose glyphs are polylines laid along the baseline, so a text is line instances drawn by the renderer every line uses: sharp at every zoom and angle, in its layer's colour, with nothing added to the renderer. Metrics are arithmetic over the glyph table, computed on the host. A glyph atlas is the step to take if drawings of a hundred thousand texts are ever the target — [ADR-0016](adr/0016-text-rendering.md).

---

# 24. File Strategy

## 24.1 DXF first

Initial interchange format:

> **DXF**

The exact initial DXF release target is PROVISIONAL.

> **Resolved in M0:** ASCII R12 writer, release-agnostic reader, hand-written — [ADR-0005](adr/0005-dxf-baseline.md).

> **Resolved in M4 (properties and tables):** the writer writes the layer and linetype tables, every entity's layer, colour, and linetype, and the settings for new objects in the header; the reader takes them from any release, makes the rows a file names but never defines, and reports it. Lineweights are read but not written — R12 has no group for them — and the export report names what was lost; raising the release is M5's decision ([ADR-0005](adr/0005-dxf-baseline.md), amended).

> **Resolved in M5 (release):** the writer writes R2000 — handles, the nine tables, the two spaces, the dictionary and layouts — so that lineweights, paragraphs (MTEXT), and polylines (LWPOLYLINE) are written as what they are and nothing the drawing holds is lost; the reader is unchanged ([ADR-0018](adr/0018-dxf-release-r2000.md)).

> **Resolved in M4 (blocks):** the writer writes a `BLOCKS` section with every definition and an `INSERT` for every reference — a mirrored reference as a negative Y scale — and the reader takes them from any release, nested, a definition placing one the file defines after it; a reference with a non-uniform scale, an array, or attributes comes in as far as the model holds and the rest is reported, and a file whose blocks place each other in a cycle is refused ([ADR-0005](adr/0005-dxf-baseline.md), amended).

The internal document must remain semantic rather than mirroring DXF group codes.

## 24.2 Unsupported DXF content

**LOCKED v1 policy:**

CadSlate2D does not guarantee lossless preservation of unsupported DXF records.

On import:

- unsupported content is counted and reported;
- supported geometry is imported;
- the user is warned before a potentially lossy export.

CadSlate2D must not silently imply lossless round-tripping when unsupported data exists.

> **Resolved in M5 (#157):** the import report is a panel that stays until dismissed or the drawing is replaced; a status-bar marker shows while an export would lose something; and the export asks first in a narrow dialog that names, line by line, what the file will not include. What is lossy is decided in Rust (`UiState.export_losses`) and shown by the shell.

Opaque passthrough is FUTURE.

## 24.3 DWG

DWG is explicitly deferred.

It must not influence M0–M2 architecture.

---

# 25. Local-First Persistence

CadSlate2D must work without an account.

Universal path:

```text
Open/import
   ↓
edit
   ↓
autosave local recovery state
   ↓
export / Save As
```

## 25.1 Persistence implementation

**PROVISIONAL:**

Evaluate browser-local persistence using:

- IndexedDB;
- OPFS async APIs;
- a hybrid strategy.

Do not introduce a Worker solely to obtain synchronous OPFS access unless profiling or later workload requirements justify the worker architecture independently.

> **Resolved in M5:** the autosave is one IndexedDB record — the drawing as DXF, its name, the time, and whether a file holds it — written by the host a second after the drawing's revision last changed, and offered back on start when no file holds it; measured against the OPFS async API at 10k, 50k, and 100k lines in Chromium and Firefox, both within a frame of main-thread time, IndexedDB chosen for being on every main thread ([ADR-0019](adr/0019-local-persistence.md)). No Worker.

## 25.2 OPFS/Worker coupling

Synchronous OPFS access handles are tied to Worker execution.

Therefore:

- main-thread persistence should use async browser APIs;
- a future Worker-based persistence implementation may use synchronous OPFS handles if the architecture already benefits from that Worker;
- persistence choice must not silently force the compute-worker roadmap forward.

The persistence and Worker decisions should be evaluated together before any synchronous OPFS design is adopted.

## 25.3 Desktop-like save

Persistent writable file handles are a capability enhancement, not a universal assumption.

Browsers without persistent writable handles must still receive a polished import/autosave/export workflow.

> **Resolved in M5 (#159):** files are behind one interface, `FileAccess`, with two implementations chosen by capability — `showOpenFilePicker`/`showSaveFilePicker` present, or not — never by user agent. With the API, `OPEN` keeps the handle, `SAVE` writes back to it (asking where only the first time, and again if the browser refuses), `SAVEAS` asks, and the handle rides in the autosave record so a recovered drawing knows its file. Without it, every save is a download under the drawing's name. The title and the status bar carry the file name and the unsaved marker in both.

---

# Part II — M0 Implementation Specification

# 26. M0: The Line

M0 is a vertical end-to-end slice.

The goal is not breadth.

The goal is to prove the architecture with one entity.

## 26.1 User story

A user can:

```text
Open CadSlate2D
      ↓
type L
      ↓
enter 0,0
      ↓
enter @1000<45
      ↓
Enter
      ↓
line appears
      ↓
select it
      ↓
undo
      ↓
redo
      ↓
export DXF
      ↓
refresh
      ↓
reopen DXF
      ↓
same line appears
```

## 26.2 Required M0 scope

### Browser shell

- React/Vite application;
- model-space canvas;
- command line;
- status area;
- keyboard routing;
- pointer routing;
- TypeScript-owned WebGL2 context.

### Core

- Rust workspace;
- WASM integration;
- `Drawing`;
- stable `EntityId`;
- monotonic non-decrementing ID allocator;
- dense-store abstraction;
- `LineEntity`.

### Render boundary

- Rust-generated committed `RenderScene`;
- Rust-generated transient `OverlayFrame`;
- coarse typed-array/buffer handoff;
- TypeScript GPU adapter;
- no Rust-issued per-frame WebGL draw calls;
- committed GPU buffers retained across pointer movement;
- transient/overlay buffers updated independently;
- basic GPU buffer reuse strategy.

### Input

- idle command prompt;
- `LINE`;
- `L` alias;
- absolute Cartesian points;
- relative Cartesian points;
- relative polar points;
- Enter;
- Space;
- Escape;
- simple history.

### Command

- LINE state machine;
- rubber-band preview;
- commit;
- cancel.

### Selection

- line click selection;
- basic selection highlight.

### Undo

- create transaction;
- undo create;
- redo create;
- same `EntityId` restored;
- ID allocator does not roll back;
- new work after undo truncates redo history.

### Spatial index

- line bounding-box insertion;
- click-query candidate lookup.

### Rendering

- pan;
- zoom;
- crosshair;
- line rendering;
- selection rendering.

### DXF

- minimal LINE import;
- minimal LINE export;
- reopen exported file.

## 26.3 M0 acceptance criteria

M0 is complete only when:

1. UI does not own drawing geometry.
2. Rust owns the line.
3. TypeScript owns the WebGL2 context.
4. Rust produces batched `RenderScene` and `OverlayFrame` data rather than issuing GL calls.
5. committed scene buffers are not regenerated or re-uploaded merely because the pointer moves.
6. the LINE rubber-band updates only transient/overlay render data.
7. `L` and `LINE` invoke the same command.
8. pointer and typed coordinates feed the same command state machine.
9. the preview is transient.
10. committed line receives a stable logical `EntityId`.
11. `next_entity_id` never decrements through undo.
12. selection uses an indexed candidate query.
13. undo removes the line.
14. redo restores it with the same logical identity.
15. creating new geometry after undo discards the redo branch without reusing IDs.
16. DXF export writes the line.
17. DXF import restores it.
18. refreshing the browser does not reveal a hidden server dependency.

---

# 27. M0 Explicit Non-Goals

Do not add:

- circles;
- arcs;
- polylines;
- trim;
- offset;
- grips;
- dimensions;
- layers UI;
- blocks;
- hatches;
- text;
- cloud save;
- accounts;
- plugins;
- scripting;
- WebGPU;
- worker architecture.

M0 proves the skeleton.

Nothing more.

---

# 28. M0 Architectural Questions to Learn From

M0 should intentionally answer:

- Is the WASM API coarse enough?
- Is the `RenderScene` / `OverlayFrame` split natural in practice?
- Is the coarse render transport efficient enough?
- Is TypeScript-side WebGL ownership clean?
- Does pointer movement avoid committed-scene regeneration and upload?
- Is input state comfortable to model in Rust?
- Is dense-store + stable-ID separation practical?
- Is the transaction API natural?
- Does transient preview remain clean?
- Does selection/index synchronization feel simple?
- Is DXF translation cleanly isolated?
- Does React remain free of CAD state duplication?
- Do typed-array views and committed/transient GPU buffer reuse avoid unnecessary frame allocations?

M0 may revise PROVISIONAL decisions.

It should not casually violate LOCKED product boundaries.

---

# Part III — Near-Term Roadmap

# 29. M1 — Input and Basic Entities

**Confidence:** High

Deliver:

- generalized command registry;
- aliases;
- parser diagnostics;
- bracketed keyword options;
- command/input history;
- direct-distance entry;
- `LINE`;
- `CIRCLE`;
- `ARC`;
- `RECTANG`;
- basic `PLINE`;
- `ERASE`;
- `MOVE`;
- `COPY`;
- selection windows;
- transient-preview framework;
- status toggles framework.

Exit condition:

> A user can create and edit a basic precise drawing primarily through keyboard and pointer interaction.

---

# 30. M2 — OSNAP, Grips, and Basic Modify

**Confidence:** Medium-high

## Snapping

- endpoint;
- midpoint;
- center;
- quadrant;
- intersection;
- nearest;
- ORTHO;
- GRID;
- basic POLAR.

## Grip editing

### Lines

- endpoint grips;
- midpoint grip.

### Circles

- center grip;
- four quadrant grips that edit radius.

### Arcs

- start endpoint grip;
- midpoint-on-arc grip;
- end endpoint grip;
- center grip.

### Polylines

- vertex grips.

### Interaction

- transient grip edits;
- typed-coordinate grip placement where appropriate;
- OSNAP-assisted grip placement.

## Basic modify

- `ROTATE`;
- `SCALE`;
- `MIRROR`;
- line offset;
- circle offset;
- simple trim;
- simple extend;
- basic line-line fillet;
- basic chamfer.

Exit condition:

> Ordinary drafting begins to feel recognizably like classic CAD.

---

# Part IV — Provisional Roadmap

Everything below is directional and may change after M0–M2.

# 31. M3 — Robust Modify Geometry

**Status:** PROVISIONAL

Potential work:

- polyline arc/bulge model;
- mixed line/arc intersections;
- pick-point-dependent trim;
- multi-boundary trim;
- robust extend;
- polyline offset;
- self-intersection cleanup;
- more complete fillet;
- more complete chamfer;
- `BREAK`;
- `JOIN`;
- `STRETCH`;
- geometry regression corpus;
- property-based stress testing.

The exact scope should be rewritten after real M2 geometry exists.

> **Resolved in M3:** all of the above was done, in the order set out at the M2 exit — see the [M3 exit review](reviews/m3-exit-review.md). Polyline offset is [ADR-0011](adr/0011-polyline-offset.md); the regression corpus and the invariant of each operation are in [`test-data/geometry/README.md`](../test-data/geometry/README.md); what each command supports is [`modify-matrix.md`](modify-matrix.md).

---

# 32. M4 — Drawing Organization

**Status:** PROVISIONAL

Potential work:

- layers;
- colors;
- linetypes;
- lineweights;
- blocks;
- insert;
- explode;
- text;
- multiline text;
- properties UI.

All persistent table changes must use the unified transaction system.

Block redefinition must remain one semantic block transaction while invalidating derived state for dependent inserts.

> **Resolved in M4 (layers):** a layer has a name, a colour, a linetype, a lineweight, and is on or off and locked or unlocked. Layer `0` cannot be removed or renamed; names are unique whatever their case. The current layer is a drawing setting, changed and undone like anything else in the drawing; it cannot be off. Objects on a layer that is off are neither drawn nor reachable by the pointer; objects on a locked layer are drawn and snapped to but not changed.

> **Rewritten at the M3 exit:** the order of work and what is left out are in the [roadmap](roadmap.md) and the issues of the M4 milestone.

> **Done (2026-10-06):** every item above is implemented; the [M4 review](reviews/m4-exit-review.md) links each criterion to its tests. Properties live beside geometry (ADR-0013); colours cross the boundary as a style table (ADR-0014); dashes are drawn by the shader (ADR-0015); text is a stroke font (ADR-0016); blocks are table rows resolved through the table (ADR-0017).

---

# 33. M5 — DXF Compatibility

**Status:** PROVISIONAL

Potential work:

- broader documented DXF versions;
- expanded import/export;
- compatibility matrix;
- unsupported-content warnings;
- round-trip fixture corpus;
- local recovery persistence;
- capability-enhanced save handles.

> **Rewritten at the M4 exit:** the order of work and what is left out are in the [roadmap](roadmap.md) and the issues of the M5 milestone; the [M4 review](reviews/m4-exit-review.md) says what M4 left for it.

> **Done (2026-10-06):** every item above is implemented; the [M5 review](reviews/m5-exit-review.md) links each criterion to its tests. The writer writes R2000 (ADR-0018); what every record becomes is [`dxf-matrix.md`](dxf-matrix.md), backed by a 45-file corpus; every quantity a file can grow is bounded (§43); the import report and the lossy-export warning are in the shell (§24.2); the drawing is kept in IndexedDB and offered back (ADR-0019), and saved back to its file where the browser can (§25.3).

---

# 34. M6 — Annotation and Hatch

**Status:** PROVISIONAL

Ordering dependency:

```text
stable text metrics
      ↓
dimension layout engine
      ↓
dimension rendering
      ↓
hatch / annotation refinement
```

Potential work:

1. text measurement stabilization;
2. improved text renderer;
3. dimension styles;
4. linear/aligned/angular/radius/diameter dimensions;
5. basic hatch;
6. solid fill;
7. hatch pattern support.

Dimensions must not begin before text metrics are reliable enough to place text, gaps, arrowheads, and extension lines consistently.

> **Rewritten at the M5 exit:** the order of work and what is left out are in the [roadmap](roadmap.md) and the issues of the M6 milestone; the [M5 review](reviews/m5-exit-review.md) says what M5 left for it. The "improved text renderer" is not taken up: ADR-0016's stroke font stands until rendering is measured on hardware (#110). The dimension entity and its layout engine are [ADR-0020](adr/0020-dimensions.md); filled regions and hatches are [ADR-0021](adr/0021-filled-regions.md).

> **Completed in M6:** text metrics, dimension styles, linear, aligned, angular, radius, and diameter dimensions, solid fill, and the classic hatch patterns, all saved and reopened through DXF — the [M6 review](reviews/m6-exit-review.md).

---

# 35. M7 — Rendering and Performance Hardening

**Status:** PROVISIONAL

Possible work:

- stroke refinement;
- cap/join correctness;
- linetype stability;
- chosen curve-rendering strategy;
- RenderScene/OverlayFrame packing optimization;
- committed/transient GPU-buffer reuse;
- text-atlas optimization;
- viewport culling;
- batching;
- 100k-entity benchmarks;
- fuzzing;
- crash recovery;
- worker experiments.

> **M7:** the worker experiment is [ADR-0024](adr/0024-autosave-in-slices.md): no worker — the one recurring main-thread cost, the autosave, is an export written in slices a turn of the event loop apart; the open and DXFOUT are user actions within a second at 100k. Fuzzing is `fuzz/` — libFuzzer targets for the DXF reader and the input grammar, run by `scripts/fuzz.sh` and weekly in CI, each finding a fixture and a fix (#177). Crash recovery is in [ADR-0019](adr/0019-local-persistence.md), extended: a panic in the engine is kept by its hook, every call after it is declined in TypeScript, the autosave is never written again, and a reload offers the drawing as last kept (#176). Committed GPU-buffer reuse is [ADR-0022](adr/0022-scene-chunks.md) — an edit re-packs one chunk and the frame uploads the ranges that moved. Viewport culling is not taken up: zoomed in, the frames are the cheaper ones, and what remains is the rasteriser's per-instance work, which chunk culling cannot cut on a drawing whose ID order does not follow its layout — [ADR-0023](adr/0023-viewport-culling.md), with the figures that would reopen it.

Worker architecture must be justified by profiling, not assumed.

> **Broken down at the M6 exit:** the order of work and the issues are in the [roadmap](roadmap.md) and the [M6 review](reviews/m6-exit-review.md). Every item above is decided by measurement, starting from a mixed drawing at 10k, 50k, and 100k entities against the §42 targets (#171).

---

# 36. M8 — Compatibility Expansion

**Status:** FUTURE

Possible work:

- SHX fonts;
- advanced dimensions;
- advanced hatch;
- advanced selection;
- attributes;
- xrefs;
- paper space;
- layouts;
- plotting;
- PDF export;
- scripting;
- plugins;
- user aliases;
- native packaging;
- eventual DWG strategy.

Nothing in M8 should constrain M0.

---

# Part V — Provisional Subsystem Notes

# 37. Worker Strategy

**Status:** PROVISIONAL

The authoritative interactive document remains with the input loop.

Possible later structure:

```text
Main thread
├ React
├ authoritative Rust/WASM core
├ command/input state
├ TypeScript WebGL adapter
└ interactive previews
      │
      ├───────────────┐
      ▼               ▼
render worker      compute/persistence worker
optional           expensive pure jobs / sync OPFS
```

Potential compute jobs:

- large DXF parsing;
- hatch;
- complex polyline offset;
- boundary discovery.

Workers should receive immutable inputs and return results that are committed atomically.

A persistence Worker should only be introduced if it is independently justified, not merely because synchronous OPFS access is available there.

---

# 38. Rendering Backend Evolution

**Status:** PROVISIONAL

Initial:

```text
TypeScript GPU adapter
        ↓
      WebGL2
```

Future possibility:

```text
TypeScript GPU adapter
        ↓
      WebGPU
```

The Rust document and command model must never depend on either.

The `RenderFrame` boundary should remain renderer-backend-neutral enough that a future WebGPU adapter does not require rewriting CAD semantics.

---

# 39. Native Project Format

**Status:** FUTURE

A native format may eventually exist, for example:

```text
.cslate
```

No native format is required for M0–M2.

---

# 40. DWG

**Status:** FUTURE

No decision is required now.

---

# 41. Testing Rules

## LOCKED

### Geometry

- unit tests;
- property tests where appropriate;
- regression test per fixed geometry bug.

### Commands

Headless command execution should be testable.

Example:

```text
Given empty drawing
When LINE receives 0,0 and 100,0
Then one line exists
And undo removes it
And redo restores the same EntityId
And next_entity_id never decreases
```

### Render boundary

Test:

- deterministic committed `RenderScene` generation from deterministic drawing/view state;
- deterministic `OverlayFrame` generation from deterministic transient state;
- pointer-only transient updates do not change the committed scene revision;
- no entity-by-entity JS/WASM getter loop;
- buffer lengths and batch ranges;
- renderer does not mutate CAD state.

### DXF

Maintain fixture files for:

- minimal valid drawings;
- malformed files;
- unsupported content;
- regression cases;
- exported fixtures from established CAD tools.

### Browser E2E

Use Playwright for:

- command entry;
- pointer interaction;
- selection;
- undo/redo;
- export;
- reopen.

### Visual regression

Later useful for:

- snap markers;
- grips;
- lineweights;
- joins/caps;
- linetypes;
- curves;
- text;
- dimensions;
- hatches.

---

# 42. Performance Principles

Targets remain goals, not promises until benchmarked.

Desired behavior on ordinary modern laptops:

- smooth pan/zoom;
- immediate command input;
- low-latency snap queries;
- smooth rubber-band and grip previews;
- no full-drawing scan on every pointer move;
- no fine-grained GL call traffic across the WASM boundary;
- no full committed-scene upload for transient pointer movement.

Directional scale targets:

```text
10k entities    effortless
50k entities    smooth normal editing
100k entities   usable
```

These must be validated with generated benchmark drawings.

---

# 43. Privacy and Security

## Privacy

Default:

- no account;
- no drawing upload;
- local persistence;
- no drawing-content telemetry.

## Security

Imported files are untrusted.

Parsers should eventually be hardened against:

- malformed counts;
- allocation bombs;
- recursion bombs;
- oversized entities;
- pathological input.

> **Resolved in M5 (#156):** every quantity a DXF file can grow is bounded by a field of `ImportLimits`, with a test that reaches each bound — bytes, entities, rows per table, groups per record, bytes per value, levels of block nesting — and reaching one is a structured error or a report line, never an allocation in proportion to a claimed count; the reader's work is proportional to the bytes read, with a time budget run by `scripts/stress.sh`; and files of every record kind with any groups are fuzzed to never panic and to round-trip what they import. The bounds and their tests are listed in [`docs/dxf-matrix.md`](dxf-matrix.md).

---

# 44. Licensing

The hard product requirement is:

> **CadSlate2D is free to use.**

Open-source licensing remains a separate governance decision.

Do not let a future DWG dependency accidentally determine the whole project model.

---

# 45. Product Constraints to Protect

CadSlate2D should resist drifting toward:

- mandatory accounts;
- organizations/workspaces;
- cloud-only documents;
- BIM;
- 3D authoring;
- AI chat sidebars;
- social features;
- project management;
- collaboration-first UX;
- marketplace features.

A recurring product question should be:

> **Does this make general-purpose 2D drafting better, or are we becoming a suite?**

---

# 46. Implementation Baseline and Change Governance

**LOCKED PROCESS RULE:**

This v5 document is the implementation baseline.

The design document is no longer expected to change in response to speculative review alone.

From this point forward:

```text
design baseline
      ↓
M0 implementation
      ↓
measurements / observed friction
      ↓
ADR when architecture must change
      ↓
design updated from evidence
```

Architecture changes after v5 should be supported by implementation evidence, profiling, compatibility findings, or a concrete failed assumption.

Examples:

```text
ADR-0001 Render buffer transport
ADR-0002 Entity store implementation
ADR-0003 Spatial-index choice
ADR-0004 Curve rendering strategy
```

M0 is expected to resolve PROVISIONAL and EXPERIMENT items through code.

It is not expected to reopen LOCKED product constraints without strong evidence.

---

# 47. Definition of a Successful v1

CadSlate2D v1 is successful when a user can:

1. open it without an account;
2. create a drawing;
3. enter precise geometry through commands and coordinates;
4. select and grip-edit common entities, including lines, circles, arcs, and polylines;
5. use reliable object snaps;
6. use documented core modify operations;
7. organize drawings with layers and blocks;
8. create text, dimensions, and basic hatches;
9. undo and redo both entity and non-entity drawing changes;
10. work comfortably with large ordinary 2D drawings;
11. import a documented DXF subset;
12. export valid DXF;
13. receive explicit warnings about unsupported content;
14. recover work locally after refresh or browser failure;
15. perform all core drafting without a server-side application.

At that point CadSlate2D is not a browser experiment.

It is a real 2D CAD program.

---

# 48. Core Principle

> **Classic CAD simplicity, rebuilt with a modern architecture.**

The browser is the delivery mechanism.

Rust and WebAssembly are implementation choices.

The product itself is:

- precision;
- speed;
- muscle memory;
- local control;
- predictable drafting;
- nothing unnecessary between the user and the drawing.
