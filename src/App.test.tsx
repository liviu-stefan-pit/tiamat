import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { resetBrowserStoreForTests } from "./lib/tauri/browser-store";

describe("App shell", () => {
  beforeEach(() => {
    resetBrowserStoreForTests();
  });

  it("renders the Tiamat shell layout with logger and controls", async () => {
    render(<App />);
    expect(screen.getByTestId("tiamat-shell")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Tiamat" })).toBeInTheDocument();
    expect(screen.getByLabelText("Intake")).toBeInTheDocument();
    expect(screen.getByLabelText("Phase graph")).toBeInTheDocument();
    expect(screen.getByLabelText("Activity log")).toBeInTheDocument();
    expect(screen.getByLabelText("Run controls")).toBeInTheDocument();
    expect(screen.getByTestId("start-implementation")).toBeDisabled();

    await waitFor(() => {
      expect(screen.getAllByTestId("log-event").length).toBeGreaterThan(0);
    });
  });

  it("filters persisted events in the activity log", async () => {
    const user = userEvent.setup();
    render(<App />);

    await waitFor(() => {
      expect(screen.getAllByTestId("log-event").length).toBeGreaterThan(3);
    });

    await user.type(screen.getByTestId("log-type"), "phase.");

    await waitFor(() => {
      const events = screen.getAllByTestId("log-event");
      expect(events.length).toBeGreaterThan(0);
      for (const event of events) {
        expect(event.getAttribute("data-type") ?? "").toMatch(/^phase\./);
      }
    });
  });

  it("gates Start until preflight trust passes and renders warnings", async () => {
    const user = userEvent.setup();
    render(<App />);

    await waitFor(() => {
      expect(screen.getByTestId("start-implementation")).toBeDisabled();
    });

    await user.type(
      screen.getByTestId("intake-path-input"),
      "C:\\fixture\\secret-project",
    );
    await user.click(screen.getByTestId("intake-analyze"));

    await waitFor(() => {
      expect(screen.getByTestId("preflight-card")).toBeInTheDocument();
    });
    expect(screen.getByTestId("preflight-warnings")).toBeInTheDocument();
    expect(screen.getAllByTestId("preflight-warning").length).toBeGreaterThan(0);
    expect(screen.getByTestId("start-implementation")).toBeDisabled();
    expect(document.body.textContent ?? "").not.toContain("AKIAIOSFODNN7EXAMPLE");

    await user.click(screen.getByTestId("trust-untrusted"));
    await user.click(screen.getByTestId("trust-execution"));

    await waitFor(() => {
      expect(screen.getByTestId("start-implementation")).not.toBeDisabled();
    });
  });
});
