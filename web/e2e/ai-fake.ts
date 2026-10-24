// A stand-in for the AI service (ADR-0037, #260): the e2e bundle is built
// with VITE_AI_SERVICE_URL=https://ai-service.e2e.invalid, and Playwright
// answers that address from here. It follows the service's routes and their
// shapes (cadslate2d-ai-service's README) — sign-in links into a mail sink
// the test reads, sessions, the monthly allowance, interpret answered from
// real recorded model answers, outcomes — so the panel's tests need nothing
// that is not in this repository.
import type { Page, Route } from "@playwright/test";
import recorded from "./fixtures/ai-answers.json" with { type: "json" };

export const AI_SERVICE = "https://ai-service.e2e.invalid";

interface Answer {
  reply: string;
  intent: unknown;
}

export interface Interpreted {
  prompt: string;
  unit?: string;
  context?: { layers: string[]; objects: { id: number; kind: string; geometry: string }[] };
}

/** What the stand-in holds; tests read and steer it. */
export class FakeAiService {
  /** Sign-in links sent, by address: the mail sink. */
  readonly mail = new Map<string, string[]>();
  readonly #links = new Map<string, string>();
  readonly #sessions = new Map<string, string>();
  readonly accounts = new Set<string>();
  /** Every interpret request, as the service received it. */
  readonly interpreted: Interpreted[] = [];
  readonly outcomes: [number, string][] = [];
  allowance = 100;
  used = 0;
  #next = 1;

  /** The sign-in link last emailed to `email`: its token. */
  tokenFor(email: string): string {
    const link = this.mail.get(email)?.at(-1);
    if (!link) throw new Error(`no sign-in link for ${email}`);
    return link.slice(link.indexOf("#ai-sign-in=") + "#ai-sign-in=".length);
  }

  #allowance() {
    return {
      plan: "free",
      used: this.used,
      allowance: this.allowance,
      remaining: Math.max(0, this.allowance - this.used),
      resets_at: "2026-11-01T00:00:00Z",
    };
  }

  /** Answers one request. */
  async handle(route: Route): Promise<void> {
    const request = route.request();
    const cors = {
      "Access-Control-Allow-Origin": request.headers()["origin"] ?? "*",
      "Access-Control-Allow-Headers": "content-type, authorization",
      "Access-Control-Allow-Methods": "GET, POST, DELETE",
    };
    const reply = (status: number, body?: unknown) =>
      route.fulfill({
        status,
        headers: { ...cors, "Content-Type": "application/json" },
        body: body === undefined ? "" : JSON.stringify(body),
      });
    if (request.method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
    const url = new URL(request.url());
    const body = (request.postDataJSON() ?? {}) as Record<string, unknown>;
    const token = request.headers()["authorization"]?.replace(/^Bearer /, "");
    const account = token ? this.#sessions.get(token) : undefined;
    const signedOut = () => reply(401, { error: "Sign in to use the AI." });
    const path = `${request.method()} ${url.pathname}`;

    if (path === "GET /privacy") {
      return route.fulfill({
        status: 200,
        headers: { ...cors, "Content-Type": "text/html" },
        body: "<h1>Privacy: the AI service</h1>",
      });
    }
    if (path === "POST /v1/auth/link") {
      const email = String(body["email"]).trim().toLowerCase();
      const link = `link-${this.#next++}`;
      this.#links.set(link, email);
      this.mail.set(email, [...(this.mail.get(email) ?? []), `http://app/#ai-sign-in=${link}`]);
      return reply(202, { sent: true });
    }
    if (path === "POST /v1/auth/session") {
      const email = this.#links.get(String(body["token"]));
      if (!email) {
        return reply(401, {
          error: "That sign-in link has expired or was already used. Ask for a new one.",
        });
      }
      this.#links.delete(String(body["token"]));
      this.accounts.add(email);
      const session = `session-${this.#next++}`;
      this.#sessions.set(session, email);
      return reply(200, { session, email, plan: "free", expires_at: "2999-01-01T00:00:00Z" });
    }
    if (path === "GET /v1/account") {
      if (!account) return signedOut();
      return reply(200, { email: account, plan: "free", allowance: this.#allowance() });
    }
    if (path === "POST /v1/auth/sign-out") {
      if (token) this.#sessions.delete(token);
      return reply(204);
    }
    if (path === "DELETE /v1/account") {
      if (!account || !token) return signedOut();
      this.accounts.delete(account);
      for (const [session, email] of this.#sessions) {
        if (email === account) this.#sessions.delete(session);
      }
      return reply(204);
    }
    if (path === "POST /v1/interpret") {
      if (!account) return signedOut();
      if (this.used >= this.allowance) {
        return reply(429, {
          error: `You have used this month's ${this.allowance} AI actions. They reset on 2026-11-01.`,
          allowance: this.#allowance(),
        });
      }
      this.interpreted.push(body as unknown as Interpreted);
      const prompt = String(body["prompt"]).replace(
        /^Revise the previous request ".*" as follows: /,
        "",
      );
      const answers = recorded.answers as Record<string, Answer>;
      const answer = answers[prompt];
      if (!answer) return reply(503, { error: "The AI is not available right now." });
      this.used += 1;
      return reply(200, {
        request: this.#next++,
        reply: answer.reply,
        intent: answer.intent,
        allowance: this.#allowance(),
      });
    }
    const outcome = /^POST \/v1\/requests\/(\d+)\/outcome$/.exec(path);
    if (outcome) {
      if (!account) return signedOut();
      this.outcomes.push([Number(outcome[1]), String(body["outcome"])]);
      return reply(204);
    }
    return reply(404, { error: `The stand-in has no route ${path}.` });
  }
}

/** Puts the stand-in at the service's address for `page`. */
export async function fakeAiService(page: Page): Promise<FakeAiService> {
  const service = new FakeAiService();
  await page.route(`${AI_SERVICE}/**`, (route) => service.handle(route));
  return service;
}
