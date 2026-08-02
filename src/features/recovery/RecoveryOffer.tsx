import type { RecoveryOffer } from "../../domain/recovery";
import { recoveryBannerText } from "../../domain/recovery";
import "./RecoveryOffer.css";

interface Props {
  offer: RecoveryOffer;
  busy?: boolean;
  onResume: () => void;
  onCancel: () => void;
}

export function RecoveryOfferBanner({
  offer,
  busy = false,
  onResume,
  onCancel,
}: Props) {
  if (!offer.requiresUserChoice) {
    return null;
  }
  return (
    <section
      className="recovery-offer"
      data-testid="recovery-offer"
      data-status={offer.status}
      data-resume-allowed={offer.resumeAllowed ? "yes" : "no"}
      aria-live="polite"
    >
      <div className="recovery-offer__body">
        <h2 className="recovery-offer__title">Startup recovery</h2>
        <p className="recovery-offer__reason" data-testid="recovery-reason">
          {recoveryBannerText(offer)}
        </p>
        <ul className="recovery-offer__meta">
          <li>Interrupted attempts: {offer.interruptedAttemptCount}</li>
          <li>Unreconciled side effects: {offer.unreconciledSideEffects}</li>
          <li>DB integrity: {offer.dbIntegrityOk ? "ok" : "failed"}</li>
          <li>Low disk: {offer.lowDisk ? "yes" : "no"}</li>
        </ul>
      </div>
      <div className="recovery-offer__actions">
        <button
          type="button"
          data-testid="recovery-resume"
          disabled={busy || !offer.resumeAllowed}
          onClick={onResume}
        >
          Resume
        </button>
        <button
          type="button"
          data-testid="recovery-cancel"
          disabled={busy}
          onClick={onCancel}
        >
          Cancel run
        </button>
      </div>
    </section>
  );
}
