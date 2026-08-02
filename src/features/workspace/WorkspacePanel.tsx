import type { RunWorkspaceManifest } from "../../domain/workspace";
import { summarizeWorkspace } from "../../domain/workspace";
import "./WorkspacePanel.css";

interface WorkspacePanelProps {
  manifest: RunWorkspaceManifest | null;
  busy?: boolean;
  onExportProject?: (projectId: string) => void;
  onPromote?: () => void;
}

export function WorkspacePanel({
  manifest,
  busy = false,
  onExportProject,
  onPromote,
}: WorkspacePanelProps) {
  if (!manifest) {
    return (
      <section className="tiamat-panel" aria-label="Workspace" data-testid="workspace-panel">
        <h2>Isolated workspace</h2>
        <p className="tiamat-muted">No managed workspace yet. Start after trust to materialize.</p>
      </section>
    );
  }

  const canAct = !busy && Boolean(onExportProject || onPromote);
  const firstProjectId = manifest.projects[0]?.projectId;

  return (
    <section className="tiamat-panel" aria-label="Workspace" data-testid="workspace-panel">
      <h2>Isolated workspace</h2>
      <p data-testid="workspace-summary">{summarizeWorkspace(manifest)}</p>
      <p data-testid="workspace-source-unchanged">
        Source fingerprints: {manifest.sourceUnchanged ? "unchanged" : "CHANGED"}
      </p>
      <p className="tiamat-muted" data-testid="workspace-managed-root">
        Managed root: {manifest.managedRunRoot}
      </p>
      <p className="tiamat-muted" data-testid="workspace-promotion">
        Promotion: {manifest.promotion.status}
      </p>
      <p className="tiamat-muted" data-testid="workspace-checkpoints">
        Checkpoints: {manifest.checkpoints.length}
        {manifest.checkpoints.length > 0
          ? ` · latest ${manifest.checkpoints[manifest.checkpoints.length - 1]?.checkpointId ?? ""}`
          : ""}
      </p>
      {canAct ? (
        <div className="tiamat-workspace-actions" data-testid="workspace-actions">
          {onExportProject && firstProjectId ? (
            <button
              type="button"
              data-testid="workspace-export"
              disabled={busy}
              onClick={() => onExportProject(firstProjectId)}
            >
              Export
            </button>
          ) : null}
          {onPromote ? (
            <button
              type="button"
              data-testid="workspace-promote"
              disabled={busy || manifest.promotion.status === "promoted"}
              onClick={() => onPromote()}
            >
              Promote
            </button>
          ) : null}
        </div>
      ) : null}
      <ul data-testid="workspace-projects">
        {manifest.projects.map((project) => (
          <li key={project.projectId} data-testid="workspace-project">
            <strong>{project.projectId}</strong> — {project.kind}
            <div className="tiamat-muted">write: {project.writeRoot}</div>
            <div className="tiamat-muted">lock: {project.lockName}</div>
            {onExportProject ? (
              <button
                type="button"
                data-testid={`workspace-export-${project.projectId}`}
                disabled={busy}
                onClick={() => onExportProject(project.projectId)}
              >
                Export {project.projectId}
              </button>
            ) : null}
          </li>
        ))}
      </ul>
    </section>
  );
}
