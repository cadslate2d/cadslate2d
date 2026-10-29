// Help › Tutorial: the basics of drafting here, then how to get the most
// from the AI, as data the dialog shows. A step may be tried: a drawing
// step runs its command as a menu's macro does (ADR-0034), an AI example is
// put in the assistant's prompt, unsent. Every command named is checked
// against the core, and every example is one of the AI corpus's requests.
import type { Macro } from "../app/macro";

export interface TutorialStep {
  readonly title: string;
  /** Paragraphs. Text between backticks is shown as typed input. */
  readonly text: readonly string[];
  /** Keys or input and what they do, as a short table. */
  readonly keys?: readonly (readonly [string, string])[];
  /** A command to run from the step's "Try it". */
  readonly command?: Macro;
  /** Requests to put in the AI's prompt, each with its own "Try it". */
  readonly examples?: readonly string[];
}

export interface TutorialChapter {
  readonly id: "drawing" | "ai";
  readonly title: string;
  readonly intro: string;
  readonly steps: readonly TutorialStep[];
}

export const DRAWING: TutorialChapter = {
  id: "drawing",
  title: "Drawing basics",
  intro:
    "CadSlate2D works like the classic command-driven CAD programs: you type a command, it asks for what it needs at the command line, and you answer by clicking or typing. The menus, the toolbar, and the panels run the same commands.",
  steps: [
    {
      title: "Type a command",
      text: [
        "Start typing anywhere: the letters go to the command line at the bottom. Press Enter or Space to run it. Most commands have a short alias — `L` for LINE, `C` for CIRCLE, `M` for MOVE.",
        "Read the prompt: it says what the command wants next. Options in brackets, like `[Close/Undo]`, are chosen by typing their capital letter.",
      ],
      keys: [
        ["Enter / Space", "Accept or finish; at an empty prompt, repeat the last command"],
        ["Escape", "Cancel the command; again, clear the selection"],
        ["Up / Down", "Recall earlier input"],
      ],
      command: { command: "LINE" },
    },
    {
      title: "Give points precisely",
      text: [
        "Click to place a point, or type it. Typed coordinates are exact: that is how a drawing gets its true sizes.",
        "With a line started, move the pointer the way it should go and type a length: the next point is that far along the pointer's direction.",
      ],
      keys: [
        ["`100,50`", "The point at x 100, y 50"],
        ["`@50,0`", "50 to the right of the last point"],
        ["`@80<30`", "80 away at 30°, counter-clockwise from the right"],
        ["`120`", "120 toward the pointer (direct distance)"],
        ["`25mm`, `2.5m`", "A length in a unit, converted into the drawing's"],
      ],
      command: { command: "LINE" },
    },
    {
      title: "Let the aids find points",
      text: [
        "Object snaps catch ends, middles, centres, and crossings as the pointer passes near them; a marker shows which. ORTHO keeps lines horizontal or vertical; POLAR locks onto angles; tracking lines from points you rest on line things up. The status bar switches each of them too.",
      ],
      keys: [
        ["F3", "Object snaps on or off"],
        ["F8", "ORTHO"],
        ["F10", "POLAR"],
        ["F11", "Object snap tracking"],
        ["F7 / F9", "Grid / grid snap"],
      ],
      command: { command: "CIRCLE" },
    },
    {
      title: "Select, then change",
      text: [
        "Click an object to select it, Shift+click to take it out. Drag on empty space for a box: left to right takes what is wholly inside, right to left also what it touches.",
        "Selected objects show grips: take one and put it somewhere else to stretch or move the object. Or give a modify command — MOVE, COPY, ROTATE, OFFSET, TRIM, FILLET — before or after selecting.",
      ],
      command: { command: "OFFSET" },
    },
    {
      title: "Organise and annotate",
      text: [
        "Put each kind of line on its own layer — walls, dimensions, centre lines — with its colour, linetype, and lineweight, from the Layers panel or the LAYER command. The Properties panel shows and changes what is selected.",
        "Dimension what you drew with DIMLINEAR, DIMRADIUS, and the rest; add words with TEXT and MTEXT; fill areas with HATCH.",
      ],
      command: { command: "DIMLINEAR" },
    },
    {
      title: "Move around, undo, and keep your work",
      text: [
        "Scroll to zoom about the pointer, drag with the middle button to pan, double-click it to see everything. ZOOM and PAN do the same by command.",
        "Every change can be undone. The drawing is autosaved in this browser and offered back if the tab closes; save it to a DXF or DWG file of your own, or plot it to PDF, from the File menu.",
      ],
      keys: [
        ["Ctrl+Z / Ctrl+Y", "Undo / redo"],
        ["Ctrl+S", "Save"],
        ["Ctrl+P", "Plot to PDF"],
      ],
      command: { command: "ZOOM", steps: ["E"] },
    },
  ],
};

export const AI: TutorialChapter = {
  id: "ai",
  title: "Drawing with the AI",
  intro:
    "The AI assistant turns a request in plain words into a drawing, as a proposal you accept or reject. It runs the same commands you would, so everything it draws can be edited like anything else. It is optional: sign in from its panel to use it.",
  steps: [
    {
      title: "Say what, how big, and where",
      text: [
        "Give the numbers: sizes, positions, counts, the unit if it is not the drawing's. A request that leaves something open is drawn the simplest way, and its reply says what it assumed.",
      ],
      examples: [
        "Draw a circle of radius 25 centred at 100,100",
        "A 120 by 80 rectangle with its lower left corner at the origin, and a hole of diameter 10 at each corner, 10 in from both edges",
        "Six holes of diameter 8 evenly spaced on a circle of radius 50 around the origin, the first at 50,0",
      ],
    },
    {
      title: "Look before you accept",
      text: [
        "The answer is drawn on the canvas in its own colour, its steps listed in the panel. Accept applies it as one step, so one undo takes it back; Reject leaves no trace; Revise keeps it while your next request changes it — `make it radius 40 instead`.",
      ],
    },
    {
      title: "Talk about what is selected",
      text: [
        "Select objects first, then say what to do with them. You are shown what would be sent with the request — the selected objects and your layer names — and decide each time; nothing of the drawing is sent without that.",
      ],
      examples: [
        "Move these 25 mm up",
        "Fillet these two lines with a radius of 10",
        "Put these on the Walls layer",
      ],
    },
    {
      title: "Ask for parts that stay consistent",
      text: [
        "Plates, slots, hole patterns, and flanges come out constrained: change one dimension later and the rest follows. Ask for options when you are deciding between shapes.",
      ],
      examples: [
        "A round flange centred at 0,0: 150 outside, a 50 bore, and eight Ø12 holes on a 110 pitch circle",
        "Draw a horizontal slot 10 mm wide with its centres 40 mm apart, centred at 100,50",
        "Show me three options for a 120 × 80 mm plate with its lower left corner at the origin and four Ø8 holes 10 mm in from the corners: square corners, corners rounded to 10 mm, or 10 mm chamfers",
      ],
    },
    {
      title: "Floor plans from a brief",
      text: [
        "Give the plot, the rooms with their areas, and which rooms must connect; the program lays the plan out and draws its walls, doors, and labels. Then revise it in words, or ask for its elevations and sections.",
      ],
      examples: [
        "A floor plan on a 10 × 25 m plot: 3 bedrooms, one of them a suite with its own bathroom, a kitchen connected to the living room, about 120 m² in all",
        "A small office on a 15 × 15 m plot: a 15 m² reception, four 10 m² offices, an 18 m² meeting room, and a 3 m² WC",
      ],
    },
    {
      title: "Whole drawing sheets",
      text: [
        "Name the paper, the scale, the projection, and the views; list the parts for a bill of materials; say what to dimension. The program places the views in projection, marks the sections, numbers the parts, and centres the sheet; it checks the result before you see it.",
        "Write a specification in full when you have one. The assistant keeps the scales you give, and says what does not fit rather than quietly changing it.",
      ],
      examples: [
        "Draw a drawing sheet for an L-bracket on an A4 sheet at 1:1 in third-angle projection, with front, left side and top views. The bracket is 80 mm wide, 60 mm high and 40 mm deep, of 5 mm plate. Dimension the width, height and depth. Add a bill of materials — part 1, Bracket, 1 off, steel S235 — and balloon it.",
        "Draw the standard views of a block 100 wide, 50 high and 40 deep with a 10 mm hole right through it from the front at x 60, z 25, in first-angle projection, its front view's lower left corner at the origin",
      ],
    },
    {
      title: "Get better answers",
      text: [
        "Say what is fixed and what may change. Use the drawing's terms — layers, views, scales like `1:50`. Ask in any language. If an answer is close, revise it rather than starting over. If it cannot do something — 3D, rendering, files — it says so.",
      ],
    },
  ],
};

export const CHAPTERS: readonly TutorialChapter[] = [DRAWING, AI];
