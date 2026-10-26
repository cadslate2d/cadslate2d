// The AI panel (ADR-0037, #260): the one sanctioned "AI sidebar" of DESIGN
// §45, docked beside Layers and Properties. Signing in, the exchange, the
// consent for drawing context, and the proposal on the canvas with Accept,
// Revise, and Reject. It shows what the assistant holds — text — and never
// the drawing: the proposal lives in the engine's session.
import {
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import type { AiAssistant, AiSnapshot } from "../app/ai-assistant";

/** How many selected objects the consent lists before "and N more". */
const LISTED = 12;

export function resetDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    day: "numeric",
    month: "long",
    timeZone: "UTC",
  });
}

/**
 * Keys typed into the panel's fields are the panel's: the command line does
 * not see them. Keys pressed on its buttons go on to the command line, so
 * U after Accept undoes, as it would anywhere else.
 */
const keep = (event: KeyboardEvent) => {
  const target = event.target as HTMLElement;
  if (target.closest("input, textarea, select")) event.stopPropagation();
};

/** A plan's name as the panel shows it. */
export const planName = (plan: string) =>
  plan === "pro" ? "Pro" : plan === "free" ? "Free" : plan;

export function AiPanel({
  assistant,
  privacyUrl,
  pricingUrl,
}: {
  assistant: AiAssistant;
  privacyUrl?: string;
  /** The service's page of the plans and their prices (ADR-0002). */
  pricingUrl?: string;
}) {
  const snapshot = useSyncExternalStore(assistant.subscribe, assistant.getSnapshot);
  return (
    <aside className="ai" aria-label="AI Assistant" onKeyDown={keep} data-testid="ai-panel">
      <div className="ai__header">
        <h2 className="ai__title">AI Assistant</h2>
        {snapshot.status === "signed-in" && snapshot.allowance && (
          <span className="ai__allowance" data-testid="ai-allowance">
            {planName(snapshot.allowance.plan)} · {snapshot.allowance.remaining} of{" "}
            {snapshot.allowance.allowance} left · resets {resetDate(snapshot.allowance.resetsAt)}
          </span>
        )}
      </div>
      <div className="ai__body">
        <Body
          assistant={assistant}
          snapshot={snapshot}
          privacyUrl={privacyUrl}
          pricingUrl={pricingUrl}
        />
      </div>
    </aside>
  );
}

function Body({
  assistant,
  snapshot,
  privacyUrl,
  pricingUrl,
}: {
  assistant: AiAssistant;
  snapshot: AiSnapshot;
  privacyUrl?: string;
  pricingUrl?: string;
}) {
  const privacy = privacyUrl && (
    <a
      className="ai__link"
      href={privacyUrl}
      target="_blank"
      rel="noreferrer"
      data-testid="ai-privacy"
    >
      What is sent and kept
    </a>
  );
  switch (snapshot.status) {
    case "unconfigured":
      return (
        <p className="ai__note" data-testid="ai-unconfigured">
          This copy of CadSlate2D has no AI service. Everything else works without it; a build made
          with an AI service address (<code>VITE_AI_SERVICE_URL</code>) offers it here.
        </p>
      );
    case "starting":
      return <p className="ai__note">Checking your sign-in…</p>;
    case "signed-out":
      return <SignIn assistant={assistant} snapshot={snapshot} privacy={privacy} />;
    case "link-sent":
      return (
        <div className="ai__section" data-testid="ai-link-sent">
          <p className="ai__note">
            A sign-in link is on its way to <strong>{snapshot.email}</strong>. It works once, for 15
            minutes, in this browser.
          </p>
          <button type="button" className="ai__button" onClick={() => assistant.restartSignIn()}>
            Use another address
          </button>
        </div>
      );
    case "signed-in":
      return <Conversation assistant={assistant} snapshot={snapshot} pricingUrl={pricingUrl} />;
  }
}

function SignIn({
  assistant,
  snapshot,
  privacy,
}: {
  assistant: AiAssistant;
  snapshot: AiSnapshot;
  privacy: ReactNode;
}) {
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const email = new FormData(event.currentTarget).get("email");
    if (typeof email === "string" && email.trim()) void assistant.requestLink(email);
  };
  return (
    <form className="ai__section" onSubmit={submit} data-testid="ai-sign-in">
      <p className="ai__note">
        Describe what to draw or change, and the AI proposes it on the canvas for you to accept. It
        is optional: drafting needs no account. Sign in with your email to use it.
      </p>
      <label className="ai__label" htmlFor="ai-email">
        Email
      </label>
      <input
        id="ai-email"
        className="ai__input"
        name="email"
        type="email"
        autoComplete="email"
        required
        disabled={snapshot.busy}
      />
      <button type="submit" className="ai__button ai__button--primary" disabled={snapshot.busy}>
        Email me a sign-in link
      </button>
      <Problem assistant={assistant} snapshot={snapshot} />
      {privacy}
    </form>
  );
}

export function Problem({
  assistant,
  snapshot,
  pricingUrl,
}: {
  assistant: AiAssistant;
  snapshot: AiSnapshot;
  pricingUrl?: string;
}) {
  if (!snapshot.error) return null;
  const onFree = snapshot.allowance?.plan === "free";
  return (
    <div className="ai__error" role="alert" data-testid="ai-error" data-kind={snapshot.error.kind}>
      <p className="ai__error-text">
        {snapshot.error.message}
        {snapshot.error.kind === "limit" && snapshot.allowance && (
          <>
            {" "}
            They come back on {resetDate(snapshot.allowance.resetsAt)}
            {onFree ? "; Pro has more each month." : "."}
          </>
        )}
      </p>
      {(snapshot.error.kind === "plan" || (snapshot.error.kind === "limit" && onFree)) && (
        <Upgrade assistant={assistant} snapshot={snapshot} pricingUrl={pricingUrl} />
      )}
    </div>
  );
}

/** The way to Pro: Stripe Checkout, opened beside the app (ADR-0002). */
export function Upgrade({
  assistant,
  snapshot,
  pricingUrl,
}: {
  assistant: AiAssistant;
  snapshot: AiSnapshot;
  pricingUrl?: string;
}) {
  const plans = pricingUrl && (
    <a className="ai__link" href={pricingUrl} target="_blank" rel="noreferrer">
      Plans and prices
    </a>
  );
  if (snapshot.billing && !snapshot.billing.available) {
    return (
      <p className="ai__note" data-testid="ai-upgrade">
        Pro cannot be bought here yet. {plans}
      </p>
    );
  }
  return (
    <div className="ai__upgrade" data-testid="ai-upgrade">
      <div className="ai__actions">
        <button
          type="button"
          className="ai__button ai__button--primary"
          disabled={snapshot.busy}
          onClick={() => void assistant.upgrade("month")}
        >
          Upgrade to Pro, monthly
        </button>
        <button
          type="button"
          className="ai__button"
          disabled={snapshot.busy}
          onClick={() => void assistant.upgrade("year")}
        >
          Yearly
        </button>
      </div>
      <p className="ai__note">Payment opens in a new tab; your drawing stays here. {plans}</p>
    </div>
  );
}

function Conversation({
  assistant,
  snapshot,
  pricingUrl,
}: {
  assistant: AiAssistant;
  snapshot: AiSnapshot;
  pricingUrl?: string;
}) {
  const [text, setText] = useState("");
  // Up and Down step through the requests already made, as a command
  // line's history does: `recalled` is how far back, 0 the draft.
  const [recalled, setRecalled] = useState(0);
  const [draft, setDraft] = useState("");
  const asked = snapshot.turns.filter((turn) => turn.role === "you").map((turn) => turn.text);
  const atLimit = snapshot.allowance?.remaining === 0;
  // The newest of the exchange stays in view as it grows.
  const scroll = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const scroller = scroll.current;
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
  }, [snapshot.turns.length, snapshot.proposal, snapshot.consent, snapshot.busy, snapshot.error]);
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!text.trim()) return;
    void assistant.ask(text);
    setText("");
    setRecalled(0);
  };
  return (
    <>
      <div className="ai__scroll" ref={scroll}>
        <ol className="ai__turns" data-testid="ai-turns" aria-live="polite">
          {snapshot.turns.map((turn, index) => (
            <li key={index} className={`ai__turn ai__turn--${turn.role}`} data-role={turn.role}>
              {turn.text}
            </li>
          ))}
          {snapshot.busy && <li className="ai__turn ai__turn--notice">Thinking…</li>}
        </ol>

        {snapshot.proposal && (
          <section className="ai__proposal" aria-label="Proposal" data-testid="ai-proposal">
            {snapshot.proposal.options > 1 && (
              <div className="ai__options" data-testid="ai-options">
                <button
                  type="button"
                  className="ai__button ai__button--quiet"
                  aria-label="Previous option"
                  onClick={() => assistant.showOption(-1)}
                >
                  ‹
                </button>
                <span className="ai__options-count">
                  Option {snapshot.proposal.option + 1} of {snapshot.proposal.options}
                </span>
                <button
                  type="button"
                  className="ai__button ai__button--quiet"
                  aria-label="Next option"
                  onClick={() => assistant.showOption(1)}
                >
                  ›
                </button>
              </div>
            )}
            <p className="ai__proposal-headline">{snapshot.proposal.headline}</p>
            {snapshot.proposal.operations.length > 0 && (
              <ol className="ai__steps">
                {snapshot.proposal.operations.map((step, index) => (
                  <li key={index}>{step}</li>
                ))}
              </ol>
            )}
            {snapshot.proposal.error && <p className="ai__error">{snapshot.proposal.error}</p>}
            <div className="ai__actions">
              <button
                type="button"
                className="ai__button ai__button--primary"
                disabled={snapshot.proposal.error !== undefined}
                onClick={() => assistant.accept()}
              >
                Accept
              </button>
              <button type="button" className="ai__button" onClick={() => assistant.revise()}>
                Revise
              </button>
              <button type="button" className="ai__button" onClick={() => assistant.reject()}>
                Reject
              </button>
            </div>
            {snapshot.revising !== undefined && (
              <p className="ai__note" data-testid="ai-revising">
                Say how to change it: the next request refines this one.
              </p>
            )}
            {snapshot.proposal.more && (
              <div className="ai__more" data-testid="ai-more">
                <p className="ai__note">
                  Pro shows {snapshot.proposal.more.count} more{" "}
                  {snapshot.proposal.more.count === 1 ? "option" : "options"} for this request.
                </p>
                <Upgrade assistant={assistant} snapshot={snapshot} pricingUrl={pricingUrl} />
              </div>
            )}
          </section>
        )}

        {snapshot.consent && (
          <section
            className="ai__consent"
            aria-label="Send drawing context?"
            data-testid="ai-consent"
          >
            <p className="ai__note">
              Send these with your request, so the AI can act on them? They are used for this
              request only, and not kept.
            </p>
            <ul className="ai__context">
              {snapshot.consent.context.objects.slice(0, LISTED).map((object) => (
                <li key={object.id}>
                  <strong>{object.kind}</strong> {object.geometry}
                  {object.layer ? ` · layer ${object.layer}` : ""}
                </li>
              ))}
              {snapshot.consent.context.objects.length > LISTED && (
                <li>and {snapshot.consent.context.objects.length - LISTED} more</li>
              )}
              {snapshot.consent.context.leftOut > 0 && (
                <li>{snapshot.consent.context.leftOut} more selected are left out</li>
              )}
              {snapshot.consent.context.parameters.map((parameter) => (
                <li key={parameter}>
                  <strong>parameter</strong> {parameter}
                </li>
              ))}
              <li>
                <strong>layers</strong> {snapshot.consent.context.layers.join(", ")}
              </li>
            </ul>
            <div className="ai__actions">
              <button
                type="button"
                className="ai__button ai__button--primary"
                onClick={() => void assistant.sendWithContext()}
              >
                Send with these
              </button>
              <button
                type="button"
                className="ai__button"
                onClick={() => void assistant.sendWithout()}
              >
                Send without
              </button>
              <button
                type="button"
                className="ai__button"
                onClick={() => assistant.cancelConsent()}
              >
                Cancel
              </button>
            </div>
          </section>
        )}

        <Problem assistant={assistant} snapshot={snapshot} pricingUrl={pricingUrl} />
        {atLimit && !snapshot.error && snapshot.allowance && (
          <div className="ai__error" role="status" data-testid="ai-limit">
            <p className="ai__error-text">
              You have used this month&apos;s {snapshot.allowance.allowance} AI actions. They come
              back on {resetDate(snapshot.allowance.resetsAt)}
              {snapshot.allowance.plan === "free" ? "; Pro has more each month" : ""}. Drafting is
              unaffected.
            </p>
            {snapshot.allowance.plan === "free" && (
              <Upgrade assistant={assistant} snapshot={snapshot} pricingUrl={pricingUrl} />
            )}
          </div>
        )}
      </div>
      <div className="ai__footer">
        <form className="ai__ask" onSubmit={submit}>
          <label className="ai__label" htmlFor="ai-prompt">
            {snapshot.revising !== undefined
              ? "How should it change?"
              : "What should be drawn or changed?"}
          </label>
          <textarea
            id="ai-prompt"
            className="ai__input ai__prompt"
            rows={3}
            value={text}
            disabled={snapshot.busy || snapshot.consent !== undefined || atLimit}
            placeholder="A 120 × 80 plate with a hole of Ø10 at each corner"
            onChange={(event) => setText(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                event.currentTarget.form?.requestSubmit();
                setRecalled(0);
                return;
              }
              // On the first line, Up recalls the request before; on the
              // last, Down the one after, and then what was being typed.
              const field = event.currentTarget;
              const before = field.value.slice(0, field.selectionStart);
              const after = field.value.slice(field.selectionEnd);
              const back =
                event.key === "ArrowUp" && !before.includes("\n") && recalled < asked.length;
              const forward = event.key === "ArrowDown" && !after.includes("\n") && recalled > 0;
              if (!back && !forward) return;
              event.preventDefault();
              if (recalled === 0) setDraft(text);
              const next = recalled + (back ? 1 : -1);
              setRecalled(next);
              setText(next === 0 ? draft : (asked[asked.length - next] ?? ""));
            }}
          />
          <button
            type="submit"
            className="ai__button ai__button--primary"
            disabled={snapshot.busy || snapshot.consent !== undefined || atLimit || !text.trim()}
          >
            Ask
          </button>
        </form>
      </div>
    </>
  );
}
