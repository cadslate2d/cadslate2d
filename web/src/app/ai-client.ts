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

export interface AiAccount {
  readonly email: string;
  readonly plan: string;
  readonly allowance: Allowance;
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
}

export interface AiRequest {
  readonly prompt: string;
  readonly unit?: string;
  readonly context?: RequestContext;
}

export interface AiAnswer {
  /** The service's record of this request, for reporting the user's decision. */
  readonly request: number;
  readonly reply: string;
  /** The intent document, or null when the request is not one the operations can do. */
  readonly intent: unknown;
  readonly allowance: Allowance;
}

/** Why a call did not succeed, as the panel explains it. */
export type AiErrorKind =
  | "signed-out" // the session ended or was never there
  | "limit" // the month's allowance is spent
  | "offline" // the service could not be reached
  | "unavailable" // the service or the model is not available right now
  | "invalid"; // the service refused what was sent

export class AiError extends Error {
  constructor(
    readonly kind: AiErrorKind,
    message: string,
    readonly allowance?: Allowance,
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
  reportOutcome(session: string, request: number, outcome: "accepted" | "rejected"): Promise<void>;
}

interface WireAllowance {
  plan: string;
  used: number;
  allowance: number;
  remaining: number;
  resets_at: string;
}

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
    const body = (json ?? {}) as { error?: string; allowance?: WireAllowance };
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
      case 400:
      case 404:
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
      };
      return {
        email: account.email,
        plan: account.plan,
        allowance: allowanceOf(account.allowance),
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
        allowance: WireAllowance;
      };
      return {
        request: answer.request,
        reply: answer.reply,
        intent: answer.intent,
        allowance: allowanceOf(answer.allowance),
      };
    },
    async reportOutcome(session, request, outcome) {
      await call(`/v1/requests/${request}/outcome`, { method: "POST", session, body: { outcome } });
    },
  };
}
