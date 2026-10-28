# The AI assistant

Describe what to draw or change, in plain words, in any language; the assistant answers with a drawing you can accept, reject, or refine. It is one more way to type commands — the same commands the command line, the menus, and the toolbar run — and nothing else in CadSlate2D depends on it ([ADR-0037](adr/0037-ai-input-method.md)).

## How it works

1. **You ask.** The panel (View › AI Assistant, open by default) takes a request: a thing to draw, a change to the selected objects, a revision of what it just proposed.
2. **The model answers with operations, never with geometry.** Its answer is a document of operations — *a circle here, a plate with these holes, a floor plan from this brief, a sheet of these views* — that the program validates, then runs as ordinary commands on a copy of the drawing ([the intent IR](../crates/cad-intent/src/lib.rs), [`test-data/intent/`](../test-data/intent/README.md)).
3. **The program reviews the result** before you see it: a sheet whose views have no room, a label over another, a view too small to read, a hatch with no outline, a datum used along the wrong axis. Findings go back to the model, which answers again, up to three times.
4. **You see a proposal.** It is drawn on the canvas in its own colour, its steps listed. **Accept** applies it as one undo step; **Reject** drops it; **Revise** keeps it while your next request refines it.

Everything the assistant can do, a person can do by typing: every operation compiles to a registered command. What it cannot do — 3D, rendering, printing, files, settings — it says so.

## What you can ask

The requests below are from the program's own test corpus ([`test-data/ai/`](../test-data/ai/README.md)): each is asked of the model in English and in Portuguese, its answer recorded, and the drawing it makes checked on every build.

### Drafting

- *Draw a circle of radius 25 centred at 100,100*
- *A 120 by 80 rectangle with its lower left corner at the origin, and a hole of diameter 10 at each corner, 10 in from both edges*
- *Six holes of diameter 8 evenly spaced on a circle of radius 50 around the origin, the first at 50,0*
- *Draw a smooth curve through 0,0, 50,30, 100,0 and 150,30*
- *Add a horizontal dimension from 0,0 to 120,0, its line 15 below*
- *Draw a line 2 inches long from the origin along the x axis* — units convert into the drawing's
- *Draw a symmetric Warren roof truss as lines: span 1200, rise 200 at the centre, eight panels of 150*

With objects selected, the request is about them:

- *Move these 25 mm up* · *Copy this 100 to the right* · *Rotate this 90 degrees about its lower left corner*
- *Fillet these two lines with a radius of 10* · *Trim the part of the vertical line below the horizontal one*
- *Make these holes diameter 6* · *Put these on the Walls layer* · *Make these red*
- *Make it radius 40 instead* — a revision of the proposal on the canvas

### Parts, constraints, and parametric edits

- *A round flange centred at 0,0: 150 outside, a 50 bore, and eight Ø12 holes on a 110 pitch circle*
- *Draw a horizontal slot 10 mm wide with its centres 40 mm apart, centred at 100,50*
- *Make this plate 20 mm wider without moving the holes relative to the edges* — the part is parametric; its constraints hold
- *Make these two lines parallel* · *Constrain this line to 80 mm long*
- *Draw a circle of radius 10 tangent to both lines, in the corner between them*
- *Show me three options for a 120 × 80 mm plate …: square corners, corners rounded to 10, corners chamfered 5*
- *Draw a straight staircase in plan for a 280 cm floor-to-floor height, 90 cm wide, sized by Blondel's rule*

### Floor plans

- *A floor plan on a 10 × 25 m plot: 3 bedrooms, one of them a suite with its own bathroom, a kitchen connected to the living room, about 120 m² in all*
- *A small office on a 15 × 15 m plot: a 15 m² reception, four 10 m² offices, an 18 m² meeting room, and a 3 m² WC*
- *Make the kitchen 14 m²* · *Put the bathroom next to both bedrooms* · *Show me another option*
- *Add 150 cm windows on all the outside walls, in every room that has one*
- *A two-storey house on a 12 × 15 m plot, 3 m floor to floor: … a 12 m² stair hall with the stair to the upper floor*

A brief that cannot be built is refused with the reason, and what would make it possible ([ADR-0039](adr/0039-layout-synthesis.md)).

### Elevations, sections, views, and isometrics

- *Draw the front elevation of this plan with a gable roof at 30 degrees*
- *Cut a section through this plan along x = 7500, from the front to the back*
- *Draw the standard views of a block 100 wide, 50 high and 40 deep with a 10 mm hole right through it from the front, in first-angle projection*
- *Draw, in isometric, the outline of the top face of a box 60 long along x, 40 deep along y, at a height of 30*
- *Add an A3 landscape title block at 1:1 titled BRACKET, drawing number M-001*

### Whole drawing sheets

- *Draw a drawing sheet for an L-bracket on an A4 sheet at 1:1 in third-angle projection, with front, left side and top views. The bracket is 80 mm wide, 60 mm high and 40 mm deep, of 5 mm plate. Dimension the width, height and depth. Add a bill of materials — part 1, Bracket, 1 off, steel S235 — and balloon it.*
- *Draw a specification sheet for a horizontal end-suction centrifugal pump set, in mm, on an A3 sheet at 1:5 in first-angle projection … Views: a front elevation, a side view from the left, a plan from above, and section A-A along the shaft. Number the parts in a bill of materials … and dimension the baseplate's length, the shaft height and the overall height.*
- *Generate a 2D CAD drawing sheet for a concrete dam spillway H02 with an ogee crest and a stilling basin … Views: a top plan; longitudinal section A-A along the centreline; transverse section B-B at X=28.5 m; an upstream elevation; an enlarged end-sill detail. Concrete hatching, dimensions of the clear width and the basin length, section markers and a title block.*

A sheet is composed by the program, not the model ([ADR-0041](adr/0041-composed-sheets.md)): the model draws each view in its own coordinates, true size; the program places the views in projection, aligned, never overlapping, draws the section markers, the bill of materials, the balloons and the notes, scales a detail, and centres the finished sheet on its paper. Positions shared between views are declared once as *datums*, so the views agree. The sheet review checks the structure — every view in its frame, every section marked, every balloon's part in the table — and what the drawing's conventions ask: lineweights on layers, legible text, nothing left on layer 0.

## Your drawing stays yours

- The request is sent. The selected objects and your layer names go with it **only when you agree, for that request**, after seeing exactly what would go. Nothing is sent in the background.
- The assistant needs an account — sign in with your email; a link arrives that works once — and the hosted service. Its privacy page, linked from the panel, says what it keeps and for how long. Signing out, and deleting the account with every record of it, are in the panel.
- The allowance of AI actions per month, and when it resets, are shown in the panel.
- A build with no service configured, signed out, over the allowance, or offline: the panel says why, and everything else works as before.

## For contributors

- The operations a model may write, and how each compiles to a command: [`crates/cad-intent/`](../crates/cad-intent/src/lib.rs); the JSON Schema a model is handed is snapshot-tested in [`test-data/intent/`](../test-data/intent/README.md).
- The corpus — the requests, what should come of each, and the model's recorded answers — and how it is scored: [`test-data/ai/README.md`](../test-data/ai/README.md). `make check` replays it; the service's repository asks the model afresh every night.
- Decisions: [ADR-0037](adr/0037-ai-input-method.md) (the AI as an input method), [ADR-0038](adr/0038-constraints.md) (constraints), [ADR-0039](adr/0039-layout-synthesis.md) (floor plans), [ADR-0040](adr/0040-general-purpose-drawings.md) (elevations, sections, views, isometrics), [ADR-0041](adr/0041-composed-sheets.md) (sheets).
