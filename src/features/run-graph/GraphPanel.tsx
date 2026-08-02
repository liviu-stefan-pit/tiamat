import { useCallback, useEffect, useMemo } from "react";
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Node,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  layoutGraphProjection,
  type PhaseNodeData,
} from "../../domain/graph-layout";
import type { GraphProjection } from "../../domain/plan";
import { PhaseNode } from "./PhaseNode";
import "./GraphPanel.css";

const nodeTypes = { phase: PhaseNode };

interface GraphPanelProps {
  graph: GraphProjection | null;
  selectedPhaseId: string | null;
  onSelectPhase: (phaseId: string | null) => void;
}

function GraphCanvas({
  graph,
  selectedPhaseId,
  onSelectPhase,
}: GraphPanelProps & { graph: GraphProjection }) {
  const { fitView } = useReactFlow();
  const { nodes, edges } = useMemo(
    () => layoutGraphProjection(graph, selectedPhaseId),
    [graph, selectedPhaseId],
  );

  useEffect(() => {
    const handle = requestAnimationFrame(() => {
      void fitView({ padding: 0.2, duration: 200 });
    });
    return () => cancelAnimationFrame(handle);
  }, [graph.runId, graph.nodes.length, fitView]);

  const onNodeClick = useCallback(
    (_: unknown, node: Node) => {
      onSelectPhase(node.id);
    },
    [onSelectPhase],
  );

  const onPaneClick = useCallback(() => {
    onSelectPhase(null);
  }, [onSelectPhase]);

  return (
    <div className="tiamat-graph-flow" data-testid="graph-canvas">
      <div className="tiamat-graph-title" data-testid="graph-plan-title">
        {graph.title}
      </div>
      <ReactFlow
        nodes={nodes as Node<PhaseNodeData>[]}
        edges={edges}
        nodeTypes={nodeTypes}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable
        nodesFocusable
        edgesFocusable
        panOnScroll
        zoomOnScroll
        fitView
        onNodeClick={onNodeClick}
        onPaneClick={onPaneClick}
        proOptions={{ hideAttribution: true }}
        aria-label="Read-only phase dependency graph"
      >
        <Background gap={16} color="#2a3648" />
        <MiniMap
          pannable
          zoomable
          ariaLabel="Phase graph minimap"
          maskColor="rgba(8, 12, 18, 0.7)"
          nodeColor={(node) => {
            const status = (node.data as PhaseNodeData | undefined)?.status;
            switch (status) {
              case "running":
              case "verifying":
                return "#5b7cff";
              case "passed":
                return "#2f7d4a";
              case "failed":
              case "blocked":
                return "#a63d3d";
              default:
                return "#4a5568";
            }
          }}
        />
        <Controls showInteractive={false} aria-label="Graph zoom controls" />
      </ReactFlow>
      <ul className="tiamat-graph-edge-sr" data-testid="graph-edges">
        {graph.edges.map((edge) => (
          <li key={`${edge.from}->${edge.to}`}>
            {edge.from} → {edge.to}
          </li>
        ))}
      </ul>
    </div>
  );
}

export function GraphPanel({
  graph,
  selectedPhaseId,
  onSelectPhase,
}: GraphPanelProps) {
  return (
    <section className="tiamat-panel tiamat-graph" aria-label="Phase graph">
      <h2>Phase graph</h2>
      <p className="tiamat-muted">
        Read-only dependency graph projection (source of truth is the plan store).
      </p>
      {graph && graph.nodes.length > 0 ? (
        <ReactFlowProvider>
          <GraphCanvas
            graph={graph}
            selectedPhaseId={selectedPhaseId}
            onSelectPhase={onSelectPhase}
          />
        </ReactFlowProvider>
      ) : (
        <div className="tiamat-graph-canvas" data-testid="graph-canvas">
          <p className="tiamat-muted" data-testid="graph-empty">
            No compiled plan yet. Start implementation to run the architect.
          </p>
        </div>
      )}
    </section>
  );
}
