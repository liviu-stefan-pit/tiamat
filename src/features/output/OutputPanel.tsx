import { useState } from "react";
import { pickOutputDir } from "../../lib/tauri/commands";

interface OutputPanelProps {
  outputDir: string;
  onOutputDirChange: (dir: string) => void;
}

export function OutputPanel({ outputDir, onOutputDirChange }: OutputPanelProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState(outputDir);

  async function onPick() {
    setBusy(true);
    setError(null);
    try {
      const picked = await pickOutputDir();
      if (picked) {
        onOutputDirChange(picked);
        setDraft(picked);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="pane" data-testid="output-panel">
      <header className="pane-header">
        <h2>Output</h2>
        <p>
          Folder where Tiamat creates the run workspace. Same folder as your
          notes is fine — work lands in <code>run-&lt;id&gt;/projects/</code>{" "}
          beside your inputs, not by overwriting them.
        </p>
      </header>

      <div className="button-row">
        <button
          type="button"
          data-testid="pick-output"
          disabled={busy}
          onClick={() => void onPick()}
        >
          Choose folder
        </button>
      </div>

      <form
        className="path-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (draft.trim()) onOutputDirChange(draft.trim());
        }}
      >
        <input
          data-testid="output-path-input"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Output folder path"
          disabled={busy}
        />
        <button type="submit" disabled={busy || !draft.trim()}>
          Set
        </button>
      </form>

      {outputDir && (
        <p className="output-current" data-testid="output-current">
          {outputDir}
        </p>
      )}

      {error && (
        <p className="error" data-testid="output-error">
          {error}
        </p>
      )}
    </section>
  );
}
