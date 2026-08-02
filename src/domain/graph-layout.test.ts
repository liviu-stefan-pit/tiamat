import { describe, expect, it } from "vitest";
import { layoutGraphProjection } from "./graph-layout";
import type { GraphProjection } from "./plan";

const graph: GraphProjection = {
  runId: "r1",
  title: "Demo",
  nodes: [
    {
      phaseId: "P01",
      title: "One",
      status: "running",
      modelTier: "composer",
      objective: "first",
    },
    {
      phaseId: "P02",
      title: "Two",
      status: "ready",
      modelTier: "grok-low",
      objective: "second",
    },
  ],
  edges: [{ from: "P01", to: "P02" }],
};

describe("layoutGraphProjection", () => {
  it("layouts nodes and marks active edges for running phases", () => {
    const { nodes, edges } = layoutGraphProjection(graph, "P01");
    expect(nodes).toHaveLength(2);
    expect(nodes[0]?.data.selected).toBe(true);
    expect(nodes.every((node) => node.draggable === false)).toBe(true);
    expect(edges[0]?.animated).toBe(true);
    expect(edges[0]?.className).toBe("tiamat-edge-active");
  });
});
