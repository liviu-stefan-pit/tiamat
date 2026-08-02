import type { CliConnectionState } from "../../domain/cursor";

interface CliStatusLightProps {
  state: CliConnectionState;
  probing?: boolean;
  onRefresh?: () => void;
}

export function CliStatusLight({
  state,
  probing = false,
  onRefresh,
}: CliStatusLightProps) {
  return (
    <button
      type="button"
      className={`cli-status-light cli-status-light--${state.kind}${
        probing ? " cli-status-light--probing" : ""
      }`}
      data-testid="cli-status-light"
      data-kind={state.kind}
      title={`${state.detail} (click to refresh)`}
      aria-label={`${state.label}. ${state.detail}. Click to refresh.`}
      onClick={() => onRefresh?.()}
    >
      <span className="cli-status-dot" aria-hidden="true" />
      <span className="cli-status-label">{state.label}</span>
    </button>
  );
}
