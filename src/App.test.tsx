import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { resetBrowserStoreForTests } from "./lib/tauri/browser-store";

describe("App shell", () => {
  beforeEach(() => {
    resetBrowserStoreForTests();
  });

  it("renders the three-pane shell with Run gated", async () => {
    render(<App />);
    expect(screen.getByTestId("tiamat-shell")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Tiamat" })).toBeInTheDocument();
    expect(screen.getByTestId("intake-panel")).toBeInTheDocument();
    expect(screen.getByTestId("output-panel")).toBeInTheDocument();
    expect(screen.getByTestId("activity-log")).toBeInTheDocument();
    expect(screen.getByTestId("start-run")).toBeDisabled();

    await waitFor(() => {
      expect(screen.getAllByTestId("log-event").length).toBeGreaterThan(0);
    });
  });

  it("filters log events by search", async () => {
    const user = userEvent.setup();
    render(<App />);

    await waitFor(() => {
      expect(screen.getAllByTestId("log-event").length).toBeGreaterThan(3);
    });

    await user.type(screen.getByTestId("log-search"), "phase.");

    await waitFor(() => {
      const events = screen.getAllByTestId("log-event");
      expect(events.length).toBeGreaterThan(0);
      for (const event of events) {
        const type = event.getAttribute("data-type") ?? "";
        const msg = event.textContent ?? "";
        expect(type.includes("phase.") || msg.includes("phase.")).toBe(true);
      }
    });
  });

  it("gates Run until trust, output folder, and paths are ready", async () => {
    const user = userEvent.setup();
    render(<App />);

    await waitFor(() => {
      expect(screen.getByTestId("start-run")).toBeDisabled();
    });

    await user.type(
      screen.getByTestId("intake-path-input"),
      "C:\\fixture\\secret-project",
    );
    await user.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() => {
      expect(screen.getByTestId("preflight-summary")).toBeInTheDocument();
    });
    expect(screen.getByTestId("start-run")).toBeDisabled();
    expect(document.body.textContent ?? "").not.toContain("AKIAIOSFODNN7EXAMPLE");

    await user.click(screen.getByTestId("trust-ack").querySelector("input")!);
    await user.type(
      screen.getByTestId("output-path-input"),
      "C:\\fixture\\output",
    );
    await user.click(screen.getByRole("button", { name: "Set" }));

    await waitFor(() => {
      expect(screen.getByTestId("start-run")).not.toBeDisabled();
    });
  });
});
