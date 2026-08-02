import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import type { CursorCapabilityReport } from "../../domain/cursor";
import type { AbortSettings } from "../../domain/process";
import { resetBrowserStoreForTests } from "../../lib/tauri/browser-store";
import { SettingsPanel } from "./SettingsPanel";

function Harness() {
  const [capability, setCapability] = useState<CursorCapabilityReport | null>(
    null,
  );
  const [abortSettings, setAbortSettings] = useState<AbortSettings | null>({
    shortcut: "Ctrl+Shift+F12",
    registered: true,
    degraded: false,
    degradedAcknowledged: false,
    trayFallbackEnabled: true,
    secondPressForceMs: 3000,
    updatedAtUtc: "2026-08-02T09:00:00Z",
  });
  return (
    <SettingsPanel
      capability={capability}
      abortSettings={abortSettings}
      onCapabilityChange={setCapability}
      onAbortSettingsChange={setAbortSettings}
    />
  );
}

describe("SettingsPanel", () => {
  beforeEach(() => {
    resetBrowserStoreForTests();
  });

  it("probes capability and redacts secrets in dry-run preview", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.click(screen.getByTestId("cursor-reprobe"));
    await waitFor(() => {
      expect(screen.getByTestId("cursor-capability-status")).toHaveTextContent(
        "available",
      );
    });
    expect(screen.getByTestId("cursor-version")).toHaveTextContent("1.2.3");
    expect(screen.getByTestId("cursor-auth")).toHaveTextContent("ready");
    expect(screen.getByTestId("cursor-models").textContent).toContain(
      "composer-2.5",
    );

    await user.click(screen.getByTestId("cursor-dry-run"));
    await waitFor(() => {
      expect(screen.getByTestId("cursor-preview-spawned")).toHaveTextContent(
        "Spawned: no",
      );
    });
    const argv = screen.getByTestId("cursor-preview-argv").textContent ?? "";
    const stdin = screen.getByTestId("cursor-preview-stdin").textContent ?? "";
    expect(argv).not.toContain("demo-api-key-should-redact");
    expect(stdin).not.toContain("fixture-secret-value");
    expect(stdin).not.toContain("demo-api-key-should-redact");
  });

  it("saves configured CLI path and probes", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByTestId("cursor-cli-path-input");
    await user.clear(input);
    await user.type(input, "fixtures/cursor-cli/fake-agent.mjs");
    await user.click(screen.getByTestId("cursor-cli-path-save"));
    await waitFor(() => {
      expect(screen.getByTestId("cursor-cli-path-saved")).toHaveTextContent(
        "fake-agent",
      );
    });
    await waitFor(() => {
      expect(screen.getByTestId("cursor-capability-status")).toHaveTextContent(
        "available",
      );
    });
  });
});
