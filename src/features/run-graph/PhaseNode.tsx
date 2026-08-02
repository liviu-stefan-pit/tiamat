import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import type { PhaseNodeData } from "../../domain/graph-layout";

type PhaseFlowNode = Node<PhaseNodeData, "phase">;

export function PhaseNode({ data, selected }: NodeProps<PhaseFlowNode>) {
  const statusClass = `is-${data.status.replace(/_/g, "-")}`;
  return (
    <div
      className={`tiamat-phase-node ${statusClass}${selected || data.selected ? " is-selected" : ""}`}
      data-testid="graph-node"
      data-phase-id={data.phaseId}
      data-status={data.status}
      data-model={data.modelTier}
      role="button"
      tabIndex={0}
      aria-label={`${data.phaseId}: ${data.title}, status ${data.status}`}
      title={data.objective}
    >
      <Handle type="target" position={Position.Left} aria-hidden="true" />
      <span className="tiamat-phase-id">{data.phaseId}</span>
      <strong className="tiamat-phase-title">{data.title}</strong>
      <span className="tiamat-phase-meta">
        {data.status} · {data.modelTier}
      </span>
      <Handle type="source" position={Position.Right} aria-hidden="true" />
    </div>
  );
}
