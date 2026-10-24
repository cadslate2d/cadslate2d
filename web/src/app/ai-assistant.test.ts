import { describe, expect, test } from "vitest";
import type { AiContext, ProposalSummary } from "../cad/engine";
import { AiAssistant, type AssistantHost, type SessionStore } from "./ai-assistant";
import { AiError, type AiAnswer, type AiClient, type AiRequest, type Allowance } from "./ai-client";

const allowance = (remaining: number): Allowance => ({
  plan: "free",
  used: 100 - remaining,
  allowance: 100,
  remaining,
  resetsAt: "2026-11-01T00:00:00Z",
});

const CIRCLE = { version: 1, operations: [{ op: "circle", center: [0, 0], radius: 5 }] };

class FakeClient implements AiClient {
  links: string[] = [];
  asked: { session: string; request: AiRequest }[] = [];
  outcomes: [number, string][] = [];
  answer: AiAnswer | AiError = {
    request: 1,
    reply: "A circle.",
    intent: CIRCLE,
    allowance: allowance(99),
  };
  signedOut = 0;
  deleted = 0;
  valid = new Set(["good-token"]);
  async requestLink(email: string) {
    this.links.push(email);
  }
  async openSession(token: string) {
    if (!this.valid.has(token)) throw new AiError("signed-out", "That sign-in link has expired.");
    return { session: "s1", email: "me@example.com", expiresAt: "2999-01-01T00:00:00Z" };
  }
  async account(session: string) {
    if (session !== "s1") throw new AiError("signed-out", "Sign in to use the AI.");
    return { email: "me@example.com", plan: "free", allowance: allowance(100) };
  }
  async signOut() {
    this.signedOut += 1;
  }
  async deleteAccount() {
    this.deleted += 1;
  }
  async interpret(session: string, request: AiRequest) {
    this.asked.push({ session, request });
    if (this.answer instanceof AiError) throw this.answer;
    return this.answer;
  }
  async reportOutcome(_session: string, request: number, outcome: "accepted" | "rejected") {
    this.outcomes.push([request, outcome]);
  }
}

class FakeHost implements AssistantHost {
  context: AiContext = { layers: ["0"], objects: [], leftOut: 0 };
  proposed: string[] = [];
  accepted = 0;
  rejected = 0;
  aiContext() {
    return this.context;
  }
  unit() {
    return "mm";
  }
  propose(intent: string) {
    this.proposed.push(intent);
  }
  acceptProposal() {
    this.accepted += 1;
  }
  rejectProposal() {
    this.rejected += 1;
  }
  proposalSummary(): ProposalSummary {
    return { headline: "Adds 1 object", added: 1, changed: 0, removed: 0, operations: ["CIRCLE"] };
  }
}

class MemoryStore implements SessionStore {
  value: string | undefined;
  get() {
    return this.value;
  }
  set(value: string | undefined) {
    this.value = value;
  }
}

async function signedIn() {
  const client = new FakeClient();
  const host = new FakeHost();
  const store = new MemoryStore();
  const assistant = new AiAssistant(host, client, store);
  const rest = await assistant.start("#ai-sign-in=good-token");
  return { client, host, store, assistant, rest };
}

describe("the AI assistant (#260)", () => {
  test("a build without the service has no AI, and starting does nothing", async () => {
    const assistant = new AiAssistant(new FakeHost(), undefined, new MemoryStore());
    expect(assistant.getSnapshot().status).toBe("unconfigured");
    expect(await assistant.start("#ai-sign-in=x")).toBe("#ai-sign-in=x");
    await assistant.ask("a circle");
    expect(assistant.getSnapshot().turns).toEqual([]);
  });

  test("signs in from the link's token, keeps the session, and takes the token off the address", async () => {
    const { assistant, store, rest } = await signedIn();
    expect(rest).toBe("");
    expect(assistant.getSnapshot()).toMatchObject({
      status: "signed-in",
      email: "me@example.com",
      allowance: { remaining: 100 },
    });
    expect(JSON.parse(store.value ?? "{}")).toMatchObject({ session: "s1" });

    // The next visit finds the kept session.
    const again = new AiAssistant(new FakeHost(), new FakeClient(), store);
    expect(await again.start("#other")).toBe("#other");
    expect(again.getSnapshot().status).toBe("signed-in");
  });

  test("a spent link says so; an expired kept session is forgotten", async () => {
    const assistant = new AiAssistant(new FakeHost(), new FakeClient(), new MemoryStore());
    await assistant.start("#ai-sign-in=spent");
    expect(assistant.getSnapshot()).toMatchObject({
      status: "signed-out",
      error: { kind: "signed-out", message: "That sign-in link has expired." },
    });
    const store = new MemoryStore();
    store.value = JSON.stringify({ session: "old", email: "x", expiresAt: "2000-01-01T00:00:00Z" });
    const expired = new AiAssistant(new FakeHost(), new FakeClient(), store);
    await expired.start("");
    expect(expired.getSnapshot().status).toBe("signed-out");
    expect(store.value).toBeUndefined();
  });

  test("asks for a link and waits for the email", async () => {
    const client = new FakeClient();
    const assistant = new AiAssistant(new FakeHost(), client, new MemoryStore());
    await assistant.start("");
    await assistant.requestLink(" me@example.com ");
    expect(client.links).toEqual(["me@example.com"]);
    expect(assistant.getSnapshot()).toMatchObject({ status: "link-sent", email: "me@example.com" });
  });

  test("without a selection a request goes alone and its document is proposed", async () => {
    const { assistant, client, host } = await signedIn();
    await assistant.ask("a circle");
    expect(client.asked).toEqual([{ session: "s1", request: { prompt: "a circle", unit: "mm" } }]);
    expect(host.proposed).toEqual([JSON.stringify(CIRCLE)]);
    expect(assistant.getSnapshot()).toMatchObject({
      turns: [
        { role: "you", text: "a circle" },
        { role: "ai", text: "A circle." },
      ],
      proposal: { headline: "Adds 1 object", request: 1 },
      allowance: { remaining: 99 },
    });
  });

  test("with a selection the user sees what would go and decides; nothing goes before", async () => {
    const { assistant, client, host } = await signedIn();
    host.context = {
      layers: ["0", "HOLES"],
      objects: [{ id: 4, kind: "circle", geometry: "center 30, 30; radius 8", layer: "HOLES" }],
      leftOut: 0,
    };
    await assistant.ask("move these up 25");
    expect(client.asked).toEqual([]);
    expect(assistant.getSnapshot().consent?.context.objects).toHaveLength(1);

    await assistant.sendWithout();
    expect(client.asked[0]?.request.context).toBeUndefined();

    await assistant.ask("move these up 25");
    await assistant.sendWithContext();
    expect(client.asked[1]?.request.context).toEqual({
      layers: ["0", "HOLES"],
      objects: [{ id: 4, kind: "circle", geometry: "center 30, 30; radius 8", layer: "HOLES" }],
    });

    await assistant.ask("and again");
    assistant.cancelConsent();
    expect(client.asked).toHaveLength(2);
    expect(assistant.getSnapshot().consent).toBeUndefined();
  });

  test("accept, reject, and revise act on the proposal and report what the user did", async () => {
    const { assistant, client, host } = await signedIn();
    await assistant.ask("a circle");
    assistant.accept();
    expect(host.accepted).toBe(1);
    expect(assistant.getSnapshot().proposal).toBeUndefined();

    await assistant.ask("a circle");
    assistant.reject();
    expect(host.rejected).toBe(1);

    client.answer = { request: 3, reply: "A circle.", intent: CIRCLE, allowance: allowance(97) };
    await assistant.ask("a circle");
    assistant.revise();
    expect(assistant.getSnapshot().revising).toBe("a circle");
    client.answer = {
      request: 4,
      reply: "A bigger one.",
      intent: CIRCLE,
      allowance: allowance(96),
    };
    await assistant.ask("make it bigger");
    expect(client.asked.at(-1)?.request.prompt).toBe(
      'Revise the previous request "a circle" as follows: make it bigger',
    );
    // The revised proposal replaced the first, which was not taken.
    expect(host.rejected).toBe(2);
    expect(assistant.getSnapshot().proposal?.request).toBe(4);
    await Promise.resolve();
    expect(client.outcomes).toEqual([
      [1, "accepted"],
      [1, "rejected"],
      [3, "rejected"],
    ]);
  });

  test("a reply without a document proposes nothing", async () => {
    const { assistant, client, host } = await signedIn();
    client.answer = {
      request: 9,
      reply: "That is 3D: not possible.",
      intent: null,
      allowance: allowance(99),
    };
    await assistant.ask("extrude it");
    expect(host.proposed).toEqual([]);
    expect(assistant.getSnapshot().proposal).toBeUndefined();
    expect(assistant.getSnapshot().turns.at(-1)).toEqual({
      role: "ai",
      text: "That is 3D: not possible.",
    });
  });

  test("at the limit, offline, or signed out, it says why", async () => {
    const { assistant, client, store } = await signedIn();
    client.answer = new AiError(
      "limit",
      "You have used this month's 100 AI actions.",
      allowance(0),
    );
    await assistant.ask("a circle");
    expect(assistant.getSnapshot()).toMatchObject({
      error: { kind: "limit" },
      allowance: { remaining: 0 },
      status: "signed-in",
    });
    client.answer = new AiError("offline", "The AI service cannot be reached.");
    await assistant.ask("a circle");
    expect(assistant.getSnapshot().error?.kind).toBe("offline");
    client.answer = new AiError("signed-out", "Sign in to use the AI.");
    await assistant.ask("a circle");
    expect(assistant.getSnapshot().status).toBe("signed-out");
    expect(store.value).toBeUndefined();
  });

  test("signs out and deletes the account, dropping a waiting proposal", async () => {
    const { assistant, client, host, store } = await signedIn();
    await assistant.ask("a circle");
    await assistant.signOut();
    expect(client.signedOut).toBe(1);
    expect(host.rejected).toBe(1);
    expect(store.value).toBeUndefined();
    expect(assistant.getSnapshot()).toMatchObject({ status: "signed-out", turns: [] });

    const second = await signedIn();
    await second.assistant.deleteAccount();
    expect(second.client.deleted).toBe(1);
    expect(second.assistant.getSnapshot().status).toBe("signed-out");
  });
});
