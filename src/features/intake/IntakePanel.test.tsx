import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import type { PreflightReport } from "../../domain/intake";
import { resetBrowserStoreForTests } from "../../lib/tauri/browser-store";
import { IntakePanel } from "./IntakePanel";

function Harness() {
  const [report, setReport] = useState<PreflightReport | null>(null);
  return (
    <IntakePanel
      report={report}
      onReportChange={setReport}
      onStart={() => undefined}
    />
  );
}

describe("IntakePanel", () => {
  beforeEach(() => {
    resetBrowserStoreForTests();
  });

  it("keeps Start disabled for blocked over-limit preflight", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.type(
      screen.getByTestId("intake-path-input"),
      "C:\\fixture\\over-limit",
    );
    await user.click(screen.getByTestId("intake-analyze"));

    await waitFor(() => {
      expect(screen.getByTestId("preflight-blockers")).toBeInTheDocument();
    });
    expect(screen.getByTestId("start-implementation")).toBeDisabled();
    expect(screen.getByTestId("trust-untrusted")).toBeDisabled();
  });
});
