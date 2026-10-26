// The account (#289), outside the AI panel: a chip in the header with the
// plan and the address, and a dialog over the canvas with the rest.
import { useEffect, useState, useSyncExternalStore } from "react";
import type { AiAssistant } from "../app/ai-assistant";
import { planName, Problem, resetDate, Upgrade } from "./AiPanel";

/** The signed-in account in the header: opens the account dialog. */
export function AccountButton({
  assistant,
  onOpen,
}: {
  assistant: AiAssistant;
  onOpen: () => void;
}) {
  const snapshot = useSyncExternalStore(assistant.subscribe, assistant.getSnapshot);
  if (snapshot.status !== "signed-in") return null;
  return (
    <button
      type="button"
      className="header__account"
      aria-label={`Account, ${snapshot.email ?? ""}`}
      onClick={() => {
        onOpen();
        void assistant.refresh();
      }}
    >
      {snapshot.allowance && (
        <span className="header__plan">{planName(snapshot.allowance.plan)}</span>
      )}
      <span data-testid="ai-email">{snapshot.email}</span>
    </button>
  );
}

function longDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });
}

/**
 * The account (#289), opened from the header: the plan and its
 * subscription, this month's actions, the way to Pro or to Stripe's
 * billing portal, signing out, and deletion. What it shows is the
 * service's; the subscription is changed at Stripe.
 */
export function AccountDialog({
  assistant,
  pricingUrl,
  privacyUrl,
  onClose,
}: {
  assistant: AiAssistant;
  pricingUrl?: string;
  privacyUrl?: string;
  onClose: () => void;
}) {
  const snapshot = useSyncExternalStore(assistant.subscribe, assistant.getSnapshot);
  const [deleting, setDeleting] = useState(false);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);
  // Signed out or deleted: nothing to show.
  useEffect(() => {
    if (snapshot.status !== "signed-in") onClose();
  }, [snapshot.status, onClose]);
  const { allowance, billing } = snapshot;
  const pro = allowance?.plan === "pro";
  return (
    <div className="account" role="dialog" aria-label="Account" data-testid="ai-account">
      <section className="ai__section">
        <h2 className="ai__subtitle">Account</h2>
        <p className="ai__note">{snapshot.email}</p>
        <dl className="ai__facts">
          <dt>Plan</dt>
          <dd data-testid="ai-plan">
            {allowance ? planName(allowance.plan) : "—"}
            {pro && billing?.periodEnd && (
              <>
                {" "}
                · {billing.cancelAtPeriodEnd ? "ends" : "renews"} on {longDate(billing.periodEnd)}
              </>
            )}
          </dd>
          {allowance && (
            <>
              <dt>This month</dt>
              <dd data-testid="ai-usage">
                {allowance.used} of {allowance.allowance} AI actions used · resets{" "}
                {resetDate(allowance.resetsAt)}
              </dd>
            </>
          )}
        </dl>
        {billing?.paymentFailed && (
          <p className="ai__error" role="status" data-testid="ai-payment-failed">
            The last payment failed. Stripe tries again; update the card in Manage billing to keep
            Pro.
          </p>
        )}
        {billing?.manageable ? (
          <div className="ai__actions">
            <button
              type="button"
              className="ai__button"
              disabled={snapshot.busy}
              onClick={() => void assistant.manage()}
            >
              Manage billing
            </button>
            <span className="ai__note">The card, invoices, and cancelling, at Stripe.</span>
          </div>
        ) : (
          !pro && <Upgrade assistant={assistant} snapshot={snapshot} pricingUrl={pricingUrl} />
        )}
        <Problem assistant={assistant} snapshot={snapshot} pricingUrl={pricingUrl} />
        {deleting ? (
          <div className="ai__confirm" data-testid="ai-delete-confirm">
            <span>
              Delete the account and every record of it?
              {billing?.manageable ? " Its subscription is cancelled first." : ""}
            </span>
            <button
              type="button"
              className="ai__button ai__button--danger"
              disabled={snapshot.busy}
              onClick={() => void assistant.deleteAccount().then(() => setDeleting(false))}
            >
              Delete
            </button>
            <button
              type="button"
              className="ai__button ai__button--quiet"
              onClick={() => setDeleting(false)}
            >
              Keep it
            </button>
          </div>
        ) : (
          <div className="ai__actions">
            <button
              type="button"
              className="ai__button ai__button--quiet"
              onClick={() => setDeleting(true)}
            >
              Delete account
            </button>
          </div>
        )}
      </section>
      <div className="ai__actions account__foot">
        <button
          type="button"
          className="ai__button ai__button--primary"
          autoFocus
          onClick={onClose}
        >
          Close
        </button>
        <button
          type="button"
          className="ai__button ai__button--quiet"
          onClick={() => void assistant.signOut()}
        >
          Sign out
        </button>
        {privacyUrl && (
          <a
            className="ai__link"
            href={privacyUrl}
            target="_blank"
            rel="noreferrer"
            data-testid="ai-privacy"
          >
            What is sent and kept
          </a>
        )}
      </div>
    </div>
  );
}
