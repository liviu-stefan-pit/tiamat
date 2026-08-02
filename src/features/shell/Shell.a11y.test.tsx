import { render } from "@testing-library/react";
import { axe } from "jest-axe";
import { describe, expect, it } from "vitest";
import App from "../../App";
import { resetBrowserStoreForTests } from "../../lib/tauri/browser-store";

describe("Shell accessibility", () => {
  beforeEach(() => {
    resetBrowserStoreForTests();
  });

  it("has no serious automated accessibility violations on the main shell", async () => {
    const { container } = render(<App />);
    const results = await axe(container, {
      rules: {
        // React Flow canvas uses presentation nodes; keep shell landmarks/labels.
        "color-contrast": { enabled: false },
      },
    });
    expect(results).toHaveNoViolations();
  }, 20_000);
});
