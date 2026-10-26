// The AI service's routes, as the app calls them (ADR-0037, #260). Behind an
// interface so the assistant can be tested with a fake, and the end-to-end
// tests can answer the same requests from a stand-in service.

/** What is left of the month's AI actions. */
export interface Allowance {
  readonly plan: string;
  readonly used: number;
  readonly allowance: number;
  readonly remaining: number;
  /** ISO 8601: when the allowance resets. */
  readonly resetsAt: string;
}

/** The account's subscription, as the service learned it from Stripe (ADR-0002). */
export interface Billing {
  /** Whether plans can be bought from this service. */
  readonly available: boolean;
  /** Stripe's status of the subscription, if there was one: `active`, `past_due`, `canceled`, … */
  readonly status?: string;
  /** ISO 8601: when the period paid for ends. */
  readonly periodEnd?: string;
  /** Whether the subscription ends then. */
  readonly cancelAtPeriodEnd: boolean;
  readonly paymentFailed: boolean;
  /** Whether there is a subscription to manage in the billing portal. */
  readonly manageable: boolean;
}

export interface AiAccount {
  readonly email: string;
  readonly plan: string;
  readonly allowance: Allowance;
  readonly billing: Billing;
}

/** What a refused feature needs: the plan, and the feature's name. */
export interface PlanNeed {
  readonly plan: string;
  readonly feature: string;
}

/** A signed-in session: the token the service gave, kept by the browser. */
export interface AiSession {
  readonly session: string;
  readonly email: string;
  /** ISO 8601. */
  readonly expiresAt: string;
}

/** What may go with a request, after the user agreed (the engine's `AiContext`). */
export interface RequestContext {
  readonly layers: readonly string[];
  readonly objects: readonly {
    readonly id: number;
    readonly kind: string;
    readonly geometry: string;
    readonly layer?: string;
  }[];
  /** The parameters on those objects, by name, value, and what they measure (#274). */
  readonly parameters?: readonly string[];
}

export interface AiRequest {
  readonly prompt: string;
  readonly unit?: string;
  readonly context?: RequestContext;
  /** The proposal this request revises (#261): its request and its document. */
  readonly previous?: { readonly prompt: string; readonly intent: unknown };
}

export interface AiAnswer {
  /** The service's record of this request, for reporting the user's decision. */
  readonly request: number;
  readonly reply: string;
  /** The intent document, or null when the request is not one the operations can do. */
  readonly intent: unknown;
  /** The other options, when the request asked for them and the plan holds them (#10). */
  readonly alternatives: readonly unknown[];
  /** The options the plan does not hold: how many, and the plan they need. */
  readonly more?: { readonly count: number; readonly needs: PlanNeed };
  readonly allowance: Allowance;
}

/** Why a call did not succeed, as the panel explains it. */
export type AiErrorKind =
  | "signed-out" // the session ended or was never there
  | "limit" // the month's allowance is spent
  | "plan" // the account's plan does not hold the feature
  | "offline" // the service could not be reached
  | "unavailable" // the service or the model is not available right now
  | "invalid"; // the service refused what was sent

export class AiError extends Error {
  constructor(
    readonly kind: AiErrorKind,
    message: string,
    readonly allowance?: Allowance,
    readonly needs?: PlanNeed,
  ) {
    super(message);
    this.name = "AiError";
  }
}

export interface AiClient {
  requestLink(email: string): Promise<void>;
  openSession(token: string): Promise<AiSession>;
  account(session: string): Promise<AiAccount>;
  signOut(session: string): Promise<void>;
  deleteAccount(session: string): Promise<void>;
  interpret(session: string, request: AiRequest): Promise<AiAnswer>;
  /** What the user did with a proposal, and which option they accepted (0, the first). */
  reportOutcome(
    session: string,
    request: number,
    outcome: "accepted" | "rejected",
    option?: number,
  ): Promise<void>;
  /** Why the engine stopped the proposal of `request`, in the request's language (#261). */
  explain(session: string, request: number, reason: string): Promise<string>;
  /** Stripe Checkout's address for Pro, paid each `interval`. */
  checkout(session: string, interval: "month" | "year"): Promise<string>;
  /** Stripe's billing portal's address, for the card, invoices, and cancelling. */
  portal(session: string): Promise<string>;
}

interface WireAllowance {
  plan: string;
  used: number;
  allowance: number;
  remaining: number;
  resets_at: string;
}

interface WireBilling {
  available: boolean;
  status?: string | null;
  period_end?: string | null;
  cancel_at_period_end?: boolean;
  payment_failed?: boolean;
  manageable?: boolean;
}

const billingOf = (wire: WireBilling | undefined): Billing => ({
  available: wire?.available ?? false,
  ...(wire?.status ? { status: wire.status } : {}),
  ...(wire?.period_end ? { periodEnd: wire.period_end } : {}),
  cancelAtPeriodEnd: wire?.cancel_at_period_end ?? false,
  paymentFailed: wire?.payment_failed ?? false,
  manageable: wire?.manageable ?? false,
});

const allowanceOf = (wire: WireAllowance): Allowance => ({
  plan: wire.plan,
  used: wire.used,
  allowance: wire.allowance,
  remaining: wire.remaining,
  resetsAt: wire.resets_at,
});

/** The service at `base`, over `fetch`. */
export function httpAiClient(service: URL, fetcher: typeof fetch = globalThis.fetch): AiClient {
  // The base is a directory: `https://ai.example/` + `v1/…`.
  const base = service.pathname.endsWith("/") ? service : new URL(`${service.href}/`);
  const call = async (
    path: string,
    init: { method: string; session?: string; body?: unknown },
  ): Promise<unknown> => {
    const headers: Record<string, string> = {};
    if (init.body !== undefined) headers["Content-Type"] = "application/json";
    if (init.session) headers["Authorization"] = `Bearer ${init.session}`;
    let response: Response;
    try {
      response = await fetcher(new URL(path.replace(/^\//, ""), base), {
        method: init.method,
        headers,
        body: init.body === undefined ? undefined : JSON.stringify(init.body),
      });
    } catch {
      throw new AiError("offline", "The AI service cannot be reached. Check the connection.");
    }
    const text = await response.text();
    const json: unknown = text ? (JSON.parse(text) as unknown) : undefined;
    if (response.ok) return json;
    const body = (json ?? {}) as { error?: string; allowance?: WireAllowance; needs?: PlanNeed };
    const message = body.error ?? `The AI service answered ${response.status}.`;
    switch (response.status) {
      case 401:
        throw new AiError("signed-out", message);
      case 429:
        throw new AiError(
          "limit",
          message,
          body.allowance ? allowanceOf(body.allowance) : undefined,
        );
      case 403:
        throw new AiError("plan", message, undefined, body.needs ?? { plan: "pro", feature: "" });
      case 400:
      case 404:
      case 409:
      case 422:
        throw new AiError("invalid", message);
      default:
        throw new AiError("unavailable", message);
    }
  };
  return {
    async requestLink(email) {
      await call("/v1/auth/link", { method: "POST", body: { email } });
    },
    async openSession(token) {
      const opened = (await call("/v1/auth/session", { method: "POST", body: { token } })) as {
        session: string;
        email: string;
        expires_at: string;
      };
      return { session: opened.session, email: opened.email, expiresAt: opened.expires_at };
    },
    async account(session) {
      const account = (await call("/v1/account", { method: "GET", session })) as {
        email: string;
        plan: string;
        allowance: WireAllowance;
        billing?: WireBilling;
      };
      return {
        email: account.email,
        plan: account.plan,
        allowance: allowanceOf(account.allowance),
        billing: billingOf(account.billing),
      };
    },
    async signOut(session) {
      await call("/v1/auth/sign-out", { method: "POST", session, body: {} });
    },
    async deleteAccount(session) {
      await call("/v1/account", { method: "DELETE", session });
    },
    async interpret(session, request) {
      const answer = (await call("/v1/interpret", { method: "POST", session, body: request })) as {
        request: number;
        reply: string;
        intent: unknown;
        alternatives?: unknown[];
        more?: { count: number; needs: PlanNeed };
        allowance: WireAllowance;
      };
      return {
        request: answer.request,
        reply: answer.reply,
        intent: answer.intent,
        alternatives: answer.alternatives ?? [],
        ...(answer.more ? { more: answer.more } : {}),
        allowance: allowanceOf(answer.allowance),
      };
    },
    async reportOutcome(session, request, outcome, option = 0) {
      await call(`/v1/requests/${request}/outcome`, {
        method: "POST",
        session,
        body: { outcome, option },
      });
    },
    async explain(session, request, reason) {
      const explained = (await call(`/v1/requests/${request}/explain`, {
        method: "POST",
        session,
        body: { reason },
      })) as { reply: string };
      return explained.reply;
    },
    async checkout(session, interval) {
      const opened = (await call("/v1/billing/checkout", {
        method: "POST",
        session,
        body: { interval },
      })) as { url: string };
      return opened.url;
    },
    async portal(session) {
      const opened = (await call("/v1/billing/portal", { method: "POST", session, body: {} })) as {
        url: string;
      };
      return opened.url;
    },
  };
}
