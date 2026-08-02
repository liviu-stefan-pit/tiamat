import "./Shell.css";

export function Shell() {
  return (
    <div className="tiamat-shell" data-testid="tiamat-shell">
      <header className="tiamat-header">
        <h1>Tiamat</h1>
        <p className="tiamat-subtitle">Desktop implementation orchestrator</p>
      </header>
      <main className="tiamat-main">
        <section className="tiamat-panel" aria-label="Intake placeholder">
          <h2>Intake</h2>
          <p>Drop folders or files to begin preflight.</p>
        </section>
        <section className="tiamat-panel" aria-label="Graph placeholder">
          <h2>Phase graph</h2>
          <p>Read-only dependency graph will appear here.</p>
        </section>
        <section className="tiamat-panel" aria-label="Activity log placeholder">
          <h2>Activity log</h2>
          <p>Structured live events will stream here.</p>
        </section>
      </main>
    </div>
  );
}
