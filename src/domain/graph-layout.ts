import type { Edge, Node } from "@xyflow/react";
import type { GraphProjection } from "./plan";

export type PhaseNodeData = {
  phaseId: string;
  title: string;
  status: string;
  modelTier: string;
  objective: string;
  selected: boolean;
};

const COLUMN_GAP = 220;
const ROW_GAP = 110;

export function layoutGraphProjection(
  graph: GraphProjection,
  selectedPhaseId: string | null,
): { nodes: Node<PhaseNodeData>[]; edges: Edge[] } {
  const dependents = new Map<string, string[]>();
  const indegree = new Map<string, number>();
  for (const node of graph.nodes) {
    dependents.set(node.phaseId, []);
    indegree.set(node.phaseId, 0);
  }
  for (const edge of graph.edges) {
    if (!indegree.has(edge.to) || !dependents.has(edge.from)) continue;
    dependents.get(edge.from)!.push(edge.to);
    indegree.set(edge.to, (indegree.get(edge.to) ?? 0) + 1);
  }

  const level = new Map<string, number>();
  const queue = graph.nodes
    .filter((node) => (indegree.get(node.phaseId) ?? 0) === 0)
    .map((node) => node.phaseId);
  for (const id of queue) level.set(id, 0);
  while (queue.length > 0) {
    const id = queue.shift()!;
    const nextLevel = (level.get(id) ?? 0) + 1;
    for (const child of dependents.get(id) ?? []) {
      const current = level.get(child) ?? 0;
      if (nextLevel > current) level.set(child, nextLevel);
      const remaining = (indegree.get(child) ?? 1) - 1;
      indegree.set(child, remaining);
      if (remaining === 0) queue.push(child);
    }
  }

  const byLevel = new Map<number, string[]>();
  for (const node of graph.nodes) {
    const lvl = level.get(node.phaseId) ?? 0;
    const list = byLevel.get(lvl) ?? [];
    list.push(node.phaseId);
    byLevel.set(lvl, list);
  }

  const indexInLevel = new Map<string, number>();
  for (const [, ids] of byLevel) {
    ids.forEach((id, index) => indexInLevel.set(id, index));
  }

  const statusById = new Map(graph.nodes.map((node) => [node.phaseId, node.status]));

  const nodes: Node<PhaseNodeData>[] = graph.nodes.map((node) => {
    const lvl = level.get(node.phaseId) ?? 0;
    const row = indexInLevel.get(node.phaseId) ?? 0;
    return {
      id: node.phaseId,
      type: "phase",
      position: { x: lvl * COLUMN_GAP, y: row * ROW_GAP },
      data: {
        phaseId: node.phaseId,
        title: node.title,
        status: node.status,
        modelTier: node.modelTier,
        objective: node.objective,
        selected: selectedPhaseId === node.phaseId,
      },
      draggable: false,
      connectable: false,
      selectable: true,
      focusable: true,
      ariaLabel: `${node.phaseId} ${node.title} ${node.status}`,
    };
  });

  const edges: Edge[] = graph.edges.map((edge) => {
    const fromStatus = statusById.get(edge.from) ?? "";
    const toStatus = statusById.get(edge.to) ?? "";
    const active =
      fromStatus === "running" ||
      fromStatus === "verifying" ||
      toStatus === "running" ||
      toStatus === "verifying";
    return {
      id: `${edge.from}->${edge.to}`,
      source: edge.from,
      target: edge.to,
      type: "smoothstep",
      animated: active,
      className: active ? "tiamat-edge-active" : "tiamat-edge",
      focusable: true,
      ariaLabel: `Dependency ${edge.from} to ${edge.to}`,
    };
  });

  return { nodes, edges };
}
