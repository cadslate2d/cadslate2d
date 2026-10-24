// The AI assistant behind the panel (ADR-0037, #260): signing in by an
// emailed link, asking, the consent for drawing context, the proposal's
// Accept, Revise, and Reject, and every state where the AI cannot answer.
//
// It holds the exchange as text and the session token, nothing of the
// drawing: the proposal lives in the engine's session, the context is read
// from the engine when the user is asked about it and dropped after.

import type { AiContext, ProposalSummary } from "../cad/engine";
import {
  AiError,
  type AiClient,
  type AiErrorKind,
  type AiRequest,
  type Allowance,
} from "./ai-client";

/** What the assistant needs of the app. */
export interface AssistantHost {
  aiContext(): AiContext;
  /** The drawing's unit suffix, or empty. */
  unit(): string;
  propose(intent: string): void;
  acceptProposal(): void;
  rejectProposal(): void;
  proposalSummary(): ProposalSummary | undefined;
}

/** Somewhere to keep the session between visits: `localStorage`, or a map in tests. */
export interface SessionStore {
  get(): string | undefined;
  set(value: string | undefined): void;
}

export type AiStatus =
  | "unconfigured" // this build has no AI service
  | "starting"
  | "signed-out"
  | "link-sent"
  | "signed-in";

export interface Turn {
  readonly role: "you" | "ai" | "notice";
  readonly text: string;
}

export interface AiSnapshot {
  readonly status: AiStatus;
  readonly email?: string;
  readonly allowance?: Allowance;
  readonly turns: readonly Turn[];
  readonly busy: boolean;
  /** A request waiting for the user to send its context or not. */
  readonly consent?: { readonly prompt: string; readonly context: AiContext };
  /** The proposal on the canvas, and the service's record of it. */
  readonly proposal?: ProposalSummary & { readonly request: number };
  /** The request Revise will refine. */
  readonly revising?: string;
  /** Why the last call failed, and what kind of failure it was. */
  readonly error?: { readonly kind: AiErrorKind; readonly message: string };
}

interface Kept {
  session: string;
  email: string;
  expiresAt: string;
}

/** The fragment a sign-in link opens the app with. */
const SIGN_IN = /(?:^#|&)ai-sign-in=([A-Za-z0-9_-]+)/;

export class AiAssistant {
  #snapshot: AiSnapshot;
  readonly #listeners = new Set<() => void>();
  #kept: Kept | undefined;
  /** The request and document of the proposal on the canvas, for a revision. */
  #proposed: { prompt: string; intent: unknown } | undefined;

  constructor(
    readonly host: AssistantHost,
    readonly client: AiClient | undefined,
    readonly store: SessionStore,
  ) {
    this.#snapshot = { status: client ? "starting" : "unconfigured", turns: [], busy: false };
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  readonly getSnapshot = (): AiSnapshot => this.#snapshot;

  #set(changes: Partial<AiSnapshot>): void {
    this.#snapshot = { ...this.#snapshot, ...changes };
    for (const listener of this.#listeners) listener();
  }

  #fail(error: unknown): void {
    const failure =
      error instanceof AiError
        ? error
        : new AiError("unavailable", "Something went wrong with the AI. Try again.");
    if (failure.kind === "signed-out") {
      this.#forget();
      this.#set({ status: "signed-out", email: undefined, allowance: undefined });
    }
    this.#set({
      busy: false,
      error: { kind: failure.kind, message: failure.message },
      ...(failure.allowance ? { allowance: failure.allowance } : {}),
    });
  }

  #keep(kept: Kept): void {
    this.#kept = kept;
    this.store.set(JSON.stringify(kept));
  }

  #forget(): void {
    this.#kept = undefined;
    this.store.set(undefined);
  }

  /**
   * Starts: a sign-in link's token in `hash` is spent for a session, or a
   * kept session is checked. Returns the hash without the token, for the
   * address bar.
   */
  async start(hash: string): Promise<string> {
    if (!this.client) return hash;
    const token = SIGN_IN.exec(hash)?.[1];
    const rest = hash.replace(SIGN_IN, "").replace(/^#&/, "#").replace(/^#$/, "");
    try {
      if (token) {
        const opened = await this.client.openSession(token);
        this.#keep(opened);
      } else {
        const text = this.store.get();
        const kept = text ? (JSON.parse(text) as Kept) : undefined;
        if (!kept || Date.parse(kept.expiresAt) <= Date.now()) {
          this.#forget();
          this.#set({ status: "signed-out" });
          return rest;
        }
        this.#kept = kept;
      }
      await this.#refreshAccount();
    } catch (error) {
      this.#set({ status: "signed-out" });
      if (token) this.#fail(error);
    }
    return rest;
  }

  async #refreshAccount(): Promise<void> {
    if (!this.client || !this.#kept) return;
    const account = await this.client.account(this.#kept.session);
    this.#set({
      status: "signed-in",
      email: account.email,
      allowance: account.allowance,
      error: undefined,
    });
  }

  /** Emails a sign-in link to `email`. */
  async requestLink(email: string): Promise<void> {
    if (!this.client) return;
    this.#set({ busy: true, error: undefined });
    try {
      await this.client.requestLink(email.trim());
      this.#set({ status: "link-sent", email: email.trim(), busy: false });
    } catch (error) {
      this.#fail(error);
    }
  }

  /** Back to the sign-in form, from "check your email". */
  restartSignIn(): void {
    this.#set({ status: "signed-out", error: undefined });
  }

  /**
   * Asks: with objects selected, the user is shown what would be sent with
   * the request and decides; with none, the request goes alone.
   */
  async ask(prompt: string): Promise<void> {
    const text = prompt.trim();
    if (!text || this.#snapshot.busy || this.#snapshot.status !== "signed-in") return;
    const context = this.host.aiContext();
    if (context.objects.length > 0) {
      this.#set({ consent: { prompt: text, context }, error: undefined });
      return;
    }
    await this.#send(text, undefined);
  }

  /** The user agreed to send the context with the waiting request. */
  async sendWithContext(): Promise<void> {
    const consent = this.#snapshot.consent;
    if (!consent) return;
    this.#set({ consent: undefined });
    const { layers, objects } = consent.context;
    await this.#send(consent.prompt, { layers, objects });
  }

  /** The user declined: the request goes without the context. */
  async sendWithout(): Promise<void> {
    const consent = this.#snapshot.consent;
    if (!consent) return;
    this.#set({ consent: undefined });
    await this.#send(consent.prompt, undefined);
  }

  /** Neither: the request is dropped. */
  cancelConsent(): void {
    this.#set({ consent: undefined });
  }

  async #send(prompt: string, context: AiRequest["context"]): Promise<void> {
    if (!this.client || !this.#kept) return;
    // A revision goes with the proposal it refines, which the answer replaces.
    const revised = this.#snapshot.revising !== undefined ? this.#proposed : undefined;
    const unit = this.host.unit();
    this.#set({
      busy: true,
      error: undefined,
      turns: [...this.#snapshot.turns, { role: "you", text: prompt }],
    });
    try {
      const answer = await this.client.interpret(this.#kept.session, {
        prompt,
        ...(unit ? { unit } : {}),
        ...(context ? { context } : {}),
        ...(revised ? { previous: revised } : {}),
      });
      // A new answer replaces the proposal waiting: that one was not taken.
      const previous = this.#snapshot.proposal;
      if (previous) {
        this.host.rejectProposal();
        void this.#report(previous.request, "rejected");
      }
      const turns = [...this.#snapshot.turns, { role: "ai" as const, text: answer.reply }];
      if (answer.intent) {
        this.host.propose(JSON.stringify(answer.intent));
        const summary = this.host.proposalSummary();
        if (summary?.error) {
          // The engine stopped it: nothing is proposed, and the AI says why.
          this.host.rejectProposal();
          this.#proposed = undefined;
          this.#set({
            turns,
            allowance: answer.allowance,
            revising: undefined,
            proposal: undefined,
          });
          await this.#explain(answer.request, summary.error);
          return;
        }
        this.#proposed = { prompt, intent: answer.intent };
        this.#set({
          turns,
          busy: false,
          allowance: answer.allowance,
          revising: undefined,
          proposal: summary ? { ...summary, request: answer.request } : undefined,
        });
      } else {
        this.#proposed = undefined;
        this.#set({
          turns,
          busy: false,
          allowance: answer.allowance,
          revising: undefined,
          proposal: undefined,
        });
      }
    } catch (error) {
      this.#fail(error);
    }
  }

  /** Asks the AI why the engine stopped the proposal of `request`. */
  async #explain(request: number, reason: string): Promise<void> {
    if (!this.client || !this.#kept) return;
    let text: string;
    try {
      text = await this.client.explain(this.#kept.session, request, reason);
    } catch {
      // Without the AI's words, the engine's own.
      text = `That could not be done: ${reason}.`;
    }
    this.#set({ busy: false, turns: [...this.#snapshot.turns, { role: "ai", text }] });
  }

  async #report(request: number, outcome: "accepted" | "rejected"): Promise<void> {
    if (!this.client || !this.#kept) return;
    // The record is for evaluation: a lost report changes nothing for the user.
    await this.client.reportOutcome(this.#kept.session, request, outcome).catch(() => undefined);
  }

  /** Applies the proposal to the drawing, as one undo. */
  accept(): void {
    const proposal = this.#snapshot.proposal;
    if (!proposal) return;
    this.host.acceptProposal();
    this.#proposed = undefined;
    this.#set({
      proposal: undefined,
      revising: undefined,
      turns: [...this.#snapshot.turns, { role: "notice", text: "Accepted: U undoes it." }],
    });
    void this.#report(proposal.request, "accepted");
  }

  /** Drops the proposal; the drawing never changed. */
  reject(): void {
    const proposal = this.#snapshot.proposal;
    if (!proposal) return;
    this.host.rejectProposal();
    this.#proposed = undefined;
    this.#set({
      proposal: undefined,
      revising: undefined,
      turns: [...this.#snapshot.turns, { role: "notice", text: "Rejected: nothing changed." }],
    });
    void this.#report(proposal.request, "rejected");
  }

  /**
   * Keeps the proposal on the canvas for the next request to refine: what
   * the user types next is sent as a revision of the request that made it.
   */
  revise(): void {
    const proposal = this.#snapshot.proposal;
    if (!proposal) return;
    const last = [...this.#snapshot.turns].reverse().find((turn) => turn.role === "you");
    this.#set({ revising: last?.text ?? "" });
  }

  async signOut(): Promise<void> {
    const kept = this.#kept;
    this.#forget();
    this.#dropProposal();
    this.#set({
      status: "signed-out",
      email: undefined,
      allowance: undefined,
      turns: [],
      error: undefined,
    });
    if (kept && this.client) await this.client.signOut(kept.session).catch(() => undefined);
  }

  /** Deletes the account and every record of it, then signs out. */
  async deleteAccount(): Promise<void> {
    if (!this.client || !this.#kept) return;
    this.#set({ busy: true, error: undefined });
    try {
      await this.client.deleteAccount(this.#kept.session);
      this.#forget();
      this.#dropProposal();
      this.#set({
        status: "signed-out",
        email: undefined,
        allowance: undefined,
        turns: [],
        busy: false,
      });
    } catch (error) {
      this.#fail(error);
    }
  }

  #dropProposal(): void {
    if (this.#snapshot.proposal) this.host.rejectProposal();
    this.#proposed = undefined;
    this.#set({ proposal: undefined, revising: undefined, consent: undefined });
  }
}

/** The browser's `localStorage`, when it lets us. */
export const browserSessionStore: SessionStore = {
  get() {
    try {
      return localStorage.getItem("cadslate2d.ai.session") ?? undefined;
    } catch {
      return undefined;
    }
  },
  set(value) {
    try {
      if (value === undefined) localStorage.removeItem("cadslate2d.ai.session");
      else localStorage.setItem("cadslate2d.ai.session", value);
    } catch {
      // A browser that keeps nothing signs in each visit.
    }
  },
};
