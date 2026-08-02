import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import type { PreflightReport } from "../../domain/intake";
import { resetBrowserStoreForTests } from "../../lib/tauri/browser-store";
import { getIntakePreflight } from "../../lib/tauri/commands";
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

  it("removes one path and re-runs preflight on the remainder", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.type(
      screen.getByTestId("intake-path-input"),
      "C:\\fixture\\notes-a.md",
    );
    await user.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => {
      expect(screen.getByTestId("preflight-summary")).toBeInTheDocument();
    });

    await user.clear(screen.getByTestId("intake-path-input"));
    await user.type(
      screen.getByTestId("intake-path-input"),
      "C:\\fixture\\notes-b.md",
    );
    await user.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() => {
      const list = screen.getByTestId("intake-paths");
      expect(list.querySelectorAll("li")).toHaveLength(2);
    });

    await user.click(screen.getByTestId("intake-remove-0"));

    await waitFor(() => {
      const list = screen.getByTestId("intake-paths");
      expect(list.querySelectorAll("li")).toHaveLength(1);
      expect(list).toHaveTextContent("C:\\fixture\\notes-b.md");
      expect(list).not.toHaveTextContent("C:\\fixture\\notes-a.md");
    });
    await waitFor(() => {
      expect(screen.getByTestId("preflight-summary")).toBeInTheDocument();
    });
  });

  it("clears all paths and drops preflight summary", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.type(
      screen.getByTestId("intake-path-input"),
      "C:\\fixture\\notes-a.md",
    );
    await user.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => {
      expect(screen.getByTestId("preflight-summary")).toBeInTheDocument();
    });

    await user.click(screen.getByTestId("intake-clear-all"));

    await waitFor(() => {
      expect(screen.queryByTestId("intake-paths")).not.toBeInTheDocument();
      expect(screen.queryByTestId("preflight-summary")).not.toBeInTheDocument();
      expect(screen.queryByTestId("intake-clear-all")).not.toBeInTheDocument();
    });
    await expect(getIntakePreflight()).resolves.toBeNull();
  });
});
