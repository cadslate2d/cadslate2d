import { describe, expect, test } from "vitest";
import type { AiContext, ProposalSummary } from "../cad/engine";
import { AiAssistant, STALE_PAGE, type AssistantHost, type SessionStore } from "./ai-assistant";
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
  /** The option each report named. */
  options: number[] = [];
  answer: AiAnswer | AiError = {
    request: 1,
    reply: "A circle.",
    intent: CIRCLE,
    alternatives: [],
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
  plan = "free";
  async account(session: string) {
    if (session !== "s1") throw new AiError("signed-out", "Sign in to use the AI.");
    return {
      email: "me@example.com",
      plan: this.plan,
      allowance: { ...allowance(100), plan: this.plan },
      billing: {
        available: true,
        cancelAtPeriodEnd: false,
        paymentFailed: false,
        manageable: this.plan === "pro",
      },
    };
  }
  pages: string[] = [];
  async checkout(_session: string, interval: "month" | "year") {
    if (this.plan === "pro") throw new AiError("invalid", "This account is on Pro already.");
    this.pages.push(`checkout ${interval}`);
    return `https://checkout.stripe.test/${interval}`;
  }
  async portal() {
    if (this.plan !== "pro") throw new AiError("invalid", "There is no subscription to manage.");
    this.pages.push("portal");
    return "https://billing.stripe.test/p";
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
  async reportOutcome(
    _session: string,
    request: number,
    outcome: "accepted" | "rejected",
    option?: number,
  ) {
    this.outcomes.push([request, outcome]);
    this.options.push(option ?? 0);
  }
  explained: [number, string][] = [];
  explanation: string | AiError = "Those lines are parallel, so they never meet.";
  async explain(_session: string, request: number, reason: string) {
    this.explained.push([request, reason]);
    if (this.explanation instanceof AiError) throw this.explanation;
    return this.explanation;
  }
}

class FakeHost implements AssistantHost {
  context: AiContext = { layers: ["0"], objects: [], parameters: [], leftOut: 0, groups: [] };
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
  opened: string[] = [];
  openPage(url: string) {
    this.opened.push(url);
  }
  stopped: string | undefined;
  proposalSummary(): ProposalSummary {
    return {
      headline: "Adds 1 object",
      added: 1,
      changed: 0,
      removed: 0,
      operations: ["CIRCLE"],
      ...(this.stopped ? { error: this.stopped } : {}),
    };
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
      parameters: [],
      leftOut: 0,
      groups: [],
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

  test("a part's parameters go with its objects, shown before they are sent (#274)", async () => {
    const { assistant, client, host } = await signedIn();
    host.context = {
      layers: ["0"],
      objects: [{ id: 1, kind: "line", geometry: "start 0, 0; end 120, 0", layer: "0" }],
      parameters: ["d1 = 120: horizontal distance, id 1 start to id 1 end"],
      leftOut: 0,
      groups: [],
    };
    await assistant.ask("make it 20 wider");
    expect(assistant.getSnapshot().consent?.context.parameters).toEqual([
      "d1 = 120: horizontal distance, id 1 start to id 1 end",
    ]);
    await assistant.sendWithContext();
    expect(client.asked[0]?.request.context?.parameters).toEqual([
      "d1 = 120: horizontal distance, id 1 start to id 1 end",
    ]);
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

    client.answer = {
      request: 3,
      reply: "A circle.",
      intent: CIRCLE,
      alternatives: [],
      allowance: allowance(97),
    };
    await assistant.ask("a circle");
    assistant.revise();
    expect(assistant.getSnapshot().revising).toBe("a circle");
    client.answer = {
      request: 4,
      reply: "A bigger one.",
      intent: CIRCLE,
      alternatives: [],
      allowance: allowance(96),
    };
    await assistant.ask("make it bigger");
    // The revision goes with the proposal it refines.
    expect(client.asked.at(-1)?.request).toMatchObject({
      prompt: "make it bigger",
      unit: "mm",
      previous: { prompt: "a circle", intent: CIRCLE },
    });
    // The revised proposal replaced the first, which was not taken.
    expect(host.rejected).toBe(2);
    expect(assistant.getSnapshot().proposal?.request).toBe(4);
    await Promise.resolve();
    expect(client.outcomes).toEqual([
      [1, "accepted"],
      [1, "rejected"],
      [3, "rejected"],
    ]);
    // Once taken or dropped, the next request revises nothing.
    assistant.accept();
    await assistant.ask("a circle");
    expect(client.asked.at(-1)?.request.previous).toBeUndefined();
  });

  test("each request carries the conversation before it, and what of it was drawn", async () => {
    const { assistant, client } = await signedIn();
    const RECTANGLE = {
      version: 1,
      operations: [{ op: "rectangle", corner: [0, 0], other: [1000, 3000] }],
    };
    client.answer = {
      request: 1,
      reply: "A rectangle.",
      intent: RECTANGLE,
      alternatives: [],
      allowance: allowance(99),
    };
    await assistant.ask("draw a rectangle, 10m x 30m, in centimeters");
    // The first request has no conversation before it.
    expect(client.asked[0]?.request.history).toBeUndefined();
    assistant.accept();

    client.answer = {
      request: 2,
      reply: "A circle.",
      intent: CIRCLE,
      alternatives: [],
      allowance: allowance(98),
    };
    await assistant.ask("a circle at the center of the rectangle, radius 100 cm");
    expect(client.asked[1]?.request.history).toEqual([
      {
        prompt: "draw a rectangle, 10m x 30m, in centimeters",
        reply: "A rectangle.",
        intent: RECTANGLE,
        accepted: true,
      },
    ]);
    assistant.reject();

    client.answer = {
      request: 3,
      reply: "Which one?",
      intent: null,
      alternatives: [],
      allowance: allowance(97),
    };
    await assistant.ask("the rectangle you've just drawn");
    expect(client.asked[2]?.request.history?.map((turn) => turn.accepted)).toEqual([true, false]);
    expect(client.asked[2]?.request.history?.[1]?.intent).toEqual(CIRCLE);

    // Only the last ten go, oldest dropped first.
    for (let n = 0; n < 12; n += 1) await assistant.ask(`request ${n}`);
    const sent = client.asked.at(-1)?.request.history ?? [];
    expect(sent).toHaveLength(10);
    expect(sent.at(-1)?.prompt).toBe("request 10");
    expect(sent[0]?.prompt).toBe("request 1");

    // Signing out forgets it.
    await assistant.signOut();
    await assistant.start("#ai-sign-in=good-token");
    await assistant.ask("a circle");
    expect(client.asked.at(-1)?.request.history).toBeUndefined();
  });

  test("an answer in a newer intent version than the page reads asks for a reload, not the AI (#347)", async () => {
    const { assistant, client, host } = await signedIn();
    host.stopped = "The proposal is not valid: version 14 is not read; this is version 13.";
    await assistant.ask("a circle");
    expect(host.rejected).toBe(1);
    expect(client.explained).toEqual([]);
    expect(assistant.getSnapshot().proposal).toBeUndefined();
    expect(assistant.getSnapshot().busy).toBe(false);
    expect(assistant.getSnapshot().turns.at(-1)).toEqual({ role: "notice", text: STALE_PAGE });
    // An older document the page does not read is the engine's ordinary refusal.
    host.stopped = "The proposal is not valid: version 0 is not read; this is version 13.";
    await assistant.ask("a circle");
    expect(client.explained).toHaveLength(1);
  });

  test("a proposal the engine stops is dropped, and the AI says why (#261)", async () => {
    const { assistant, client, host } = await signedIn();
    host.stopped = "operation 0: FILLET refused: The lines are parallel";
    client.answer = {
      request: 7,
      reply: "A fillet.",
      intent: CIRCLE,
      alternatives: [],
      allowance: allowance(99),
    };
    await assistant.ask("fillet these");
    expect(host.rejected).toBe(1);
    expect(client.explained).toEqual([[7, "operation 0: FILLET refused: The lines are parallel"]]);
    expect(assistant.getSnapshot()).toMatchObject({ busy: false, proposal: undefined });
    expect(assistant.getSnapshot().turns.slice(-2)).toEqual([
      { role: "ai", text: "A fillet." },
      { role: "ai", text: "Those lines are parallel, so they never meet." },
    ]);
    // Not a decision of the user's: nothing is reported.
    expect(client.outcomes).toEqual([]);

    // Without the AI, the engine's own words.
    client.explanation = new AiError("unavailable", "The AI is not available right now.");
    await assistant.ask("fillet these");
    expect(assistant.getSnapshot().turns.at(-1)).toEqual({
      role: "ai",
      text: "That could not be done: operation 0: FILLET refused: The lines are parallel.",
    });
  });

  test("a reply without a document proposes nothing", async () => {
    const { assistant, client, host } = await signedIn();
    client.answer = {
      request: 9,
      reply: "That is 3D: not possible.",
      intent: null,
      alternatives: [],
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

describe("plans (#9, ADR-0002)", () => {
  const needsPro = (feature: string) =>
    new AiError("plan", "Floor plans through the AI are part of Pro.", undefined, {
      plan: "pro",
      feature,
    });

  test("a feature the plan does not hold is refused with the plan it needs", async () => {
    const { assistant, client, host } = await signedIn();
    client.answer = needsPro("floor_plans");
    await assistant.ask("a two-bedroom house on a 12 × 20 m plot");
    expect(assistant.getSnapshot().error).toEqual({
      kind: "plan",
      message: "Floor plans through the AI are part of Pro.",
      needs: { plan: "pro", feature: "floor_plans" },
    });
    expect(host.proposed).toEqual([]);
    expect(assistant.getSnapshot().busy).toBe(false);
  });

  test("upgrade opens Checkout beside the app; coming back on Pro clears the refusal", async () => {
    const { assistant, client, host } = await signedIn();
    client.answer = needsPro("floor_plans");
    await assistant.ask("a two-bedroom house");
    await assistant.upgrade("year");
    expect(host.opened).toEqual(["https://checkout.stripe.test/year"]);
    // Back before paying: still Free, the refusal stays.
    await assistant.refresh();
    expect(assistant.getSnapshot().error?.kind).toBe("plan");
    // Paid: Pro, and the refusal goes.
    client.plan = "pro";
    await assistant.refresh();
    expect(assistant.getSnapshot()).toMatchObject({
      allowance: { plan: "pro" },
      billing: { manageable: true },
    });
    expect(assistant.getSnapshot().error).toBeUndefined();
    await assistant.manage();
    expect(host.opened.at(-1)).toBe("https://billing.stripe.test/p");
  });

  test("a refresh keeps a problem that is not the plan's, and a failed checkout says why", async () => {
    const { assistant, client, host } = await signedIn();
    client.answer = new AiError("unavailable", "The AI is not available right now.");
    await assistant.ask("a circle");
    await assistant.refresh();
    expect(assistant.getSnapshot().error?.kind).toBe("unavailable");
    await assistant.manage();
    expect(host.opened).toEqual([]);
    expect(assistant.getSnapshot().error).toMatchObject({
      kind: "invalid",
      message: "There is no subscription to manage.",
    });
  });
});

describe("options (#10)", () => {
  const circle = (radius: number) => ({
    version: 1,
    operations: [{ op: "circle", center: [0, 0], radius }],
  });
  const radiusOf = (intent: string) =>
    (JSON.parse(intent) as { operations: { radius: number }[] }).operations[0]?.radius;

  test("options are shown in turn, and the one accepted is reported", async () => {
    const { assistant, client, host } = await signedIn();
    client.answer = {
      request: 9,
      reply: "Three circles.",
      intent: circle(10),
      alternatives: [circle(20), circle(30)],
      allowance: allowance(99),
    };
    await assistant.ask("three options for a circle");
    expect(assistant.getSnapshot().proposal).toMatchObject({ request: 9, option: 0, options: 3 });
    expect(host.proposed.map(radiusOf)).toEqual([10]);

    assistant.showOption(1);
    expect(assistant.getSnapshot().proposal).toMatchObject({ option: 1 });
    // One option on the canvas at a time: the one before is taken off.
    expect(host.rejected).toBe(1);
    expect(host.proposed.map(radiusOf)).toEqual([10, 20]);
    assistant.showOption(-2); // round, to the last
    expect(assistant.getSnapshot().proposal?.option).toBe(2);
    expect(host.proposed.map(radiusOf).at(-1)).toBe(30);

    assistant.accept();
    expect(host.accepted).toBe(1);
    await Promise.resolve();
    expect(client.outcomes.at(-1)).toEqual([9, "accepted"]);
    expect(client.options.at(-1)).toBe(2);
  });

  test("a revision refines the option shown", async () => {
    const { assistant, client } = await signedIn();
    client.answer = {
      request: 9,
      reply: "Two circles.",
      intent: circle(10),
      alternatives: [circle(20)],
      allowance: allowance(99),
    };
    await assistant.ask("two options for a circle");
    assistant.showOption(1);
    assistant.revise();
    await assistant.ask("move it right");
    expect(client.asked.at(-1)?.request.previous).toEqual({
      prompt: "two options for a circle",
      intent: circle(20),
    });
  });

  test("on Free, one option and how many more Pro shows", async () => {
    const { assistant, client } = await signedIn();
    client.answer = {
      request: 9,
      reply: "Three circles.",
      intent: circle(10),
      alternatives: [],
      more: { count: 2, needs: { plan: "pro", feature: "alternatives" } },
      allowance: allowance(99),
    };
    await assistant.ask("three options for a circle");
    expect(assistant.getSnapshot().proposal).toMatchObject({
      option: 0,
      options: 1,
      more: { count: 2, needs: { plan: "pro" } },
    });
  });

  test("an option the engine stops says why where it is shown, and cannot be accepted", async () => {
    const { assistant, client, host } = await signedIn();
    client.answer = {
      request: 9,
      reply: "Two circles.",
      intent: circle(10),
      alternatives: [circle(20)],
      allowance: allowance(99),
    };
    await assistant.ask("two options");
    host.stopped = "operation 0: the circle does not fit";
    assistant.showOption(1);
    expect(assistant.getSnapshot().proposal).toMatchObject({
      option: 1,
      error: "operation 0: the circle does not fit",
    });
    assistant.accept();
    expect(host.accepted).toBe(0);
    expect(client.explained).toEqual([]);
  });
});
