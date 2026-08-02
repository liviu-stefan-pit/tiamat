import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { GraphProjection } from "../../domain/plan";
import { GraphPanel } from "./GraphPanel";

const graph: GraphProjection = {
  runId: "r1",
  title: "Fixture plan",
  nodes: [
    {
      phaseId: "P01",
      title: "Bootstrap",
      status: "running",
      modelTier: "composer",
      objective: "boot",
    },
    {
      phaseId: "P02",
      title: "Follow-up",
      status: "ready",
      modelTier: "grok-low",
      objective: "next",
    },
  ],
  edges: [{ from: "P01", to: "P02" }],
};

describe("GraphPanel", () => {
  it("renders read-only graph projection with edges and empty state", () => {
    const { rerender } = render(
      <GraphPanel
        graph={graph}
        selectedPhaseId={null}
        onSelectPhase={() => undefined}
      />,
    );
    expect(screen.getByLabelText("Phase graph")).toBeInTheDocument();
    expect(screen.getByTestId("graph-plan-title")).toHaveTextContent(
      "Fixture plan",
    );
    expect(screen.getByTestId("graph-edges").textContent).toContain("P01 → P02");

    rerender(
      <GraphPanel
        graph={null}
        selectedPhaseId={null}
        onSelectPhase={() => undefined}
      />,
    );
    expect(screen.getByTestId("graph-empty")).toBeInTheDocument();
  });
});
