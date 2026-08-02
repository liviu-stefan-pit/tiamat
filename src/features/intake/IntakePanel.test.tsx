import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import type { PreflightReport } from "../../domain/intake";
import { resetBrowserStoreForTests } from "../../lib/tauri/browser-store";
import { IntakePanel } from "./IntakePanel";

function Harness() {
  const [report, setReport] = useState<PreflightReport | null>(null);
  const [paths, setPaths] = useState<string[]>([]);
  return (
    <IntakePanel
      report={report}
      onReportChange={setReport}
      selectedPaths={paths}
      onPathsChange={setPaths}
    />
  );
}

describe("IntakePanel", () => {
  beforeEach(() => {
    resetBrowserStoreForTests();
  });

  it("shows blockers for over-limit preflight and hides trust", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.type(
      screen.getByTestId("intake-path-input"),
      "C:\\fixture\\over-limit",
    );
    await user.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() => {
      expect(screen.getByTestId("preflight-summary")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("trust-ack")).not.toBeInTheDocument();
    expect(
      screen.getByText(/Inventory truncated: file count would exceed limit/),
    ).toBeInTheDocument();
  });
});
