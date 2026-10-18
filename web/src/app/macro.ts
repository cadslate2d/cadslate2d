// Menu macros (ADR-0034): a command and its answers, exactly as typed, as
// AutoCAD's `^C^C_arc \_c`. A pause waits for the user's own answer before
// the rest is typed. The host runs them; nothing here touches the drawing.

/** AutoCAD's `\`: wait for the user to answer the question the command is asking. */
export const PAUSE = { pause: "answer" } as const;
/** Wait while the command asks for objects, until the selection is done; none, when there is a selection already. */
export const SELECTION = { pause: "selection" } as const;

export type MacroStep = string | typeof PAUSE | typeof SELECTION;

/** A command, by name or alias, and what to type after it. */
export interface Macro {
  readonly command: string;
  readonly steps?: readonly MacroStep[];
}

/** A macro waiting on the user: what the command was asking when it paused, and what is left to type. */
export interface PausedMacro {
  readonly command: string | undefined;
  readonly prompt: string;
  readonly kind: "answer" | "selection";
  readonly rest: readonly MacroStep[];
}

/** What the command is doing now: its name and its question. */
export interface MacroView {
  readonly command: string | undefined;
  readonly prompt: string;
}

/** Whether the command is asking for objects to act on. */
export function selecting(prompt: string): boolean {
  return prompt.startsWith("Select objects") || prompt.startsWith("Remove objects");
}

/**
 * Whether a paused macro goes on: `drop` when its command has ended or
 * another has started; `resume` once the user has answered — the command
 * asks something else, or the selection is done; `wait` otherwise.
 */
export function pauseState(paused: PausedMacro, now: MacroView): "wait" | "resume" | "drop" {
  if (now.command === undefined || now.command !== paused.command) return "drop";
  if (paused.kind === "selection") return selecting(now.prompt) ? "wait" : "resume";
  return now.prompt === paused.prompt ? "wait" : "resume";
}

/** How a macro reads in a tooltip: `ARC \ C`, as AutoCAD writes it. */
export function macroText(macro: Macro): string {
  const steps = (macro.steps ?? []).map((step) => (typeof step === "string" ? step : "\\"));
  return [macro.command, ...steps].join(" ");
}
