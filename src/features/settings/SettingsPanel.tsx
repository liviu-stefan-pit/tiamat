import { useEffect, useMemo, useState } from "react";
import type {
  CursorCapabilityReport,
  CursorCommandPreview,
} from "../../domain/cursor";
import { hasNoninteractiveApproval } from "../../domain/cursor";
import type { AbortSettings } from "../../domain/process";
import { formatAbortStatus } from "../../domain/process";
import {
  getAppSettings,
  listCursorModels,
  previewCursorCommand,
  probeCursorCapability,
  rebindAbortShortcut,
  setCursorCliPath,
  type AppSettings,
} from "../../lib/tauri/commands";
import "./SettingsPanel.css";

interface SettingsPanelProps {
  capability: CursorCapabilityReport | null;
  abortSettings: AbortSettings | null;
  onCapabilityChange: (report: CursorCapabilityReport) => void;
  onAbortSettingsChange: (settings: AbortSettings) => void;
}

export function SettingsPanel({
  capability,
  abortSettings,
  onCapabilityChange,
  onAbortSettingsChange,
}: SettingsPanelProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<CursorCommandPreview | null>(null);
  const [apiKey, setApiKey] = useState("demo-api-key-should-redact");
  const [prompt, setPrompt] = useState(
    "Dry-run preview only. Never spawn. Secret: fixture-secret-value",
  );
  const [shortcut, setShortcut] = useState(
    abortSettings?.shortcut ?? "Ctrl+Shift+F12",
  );
  const [cliPath, setCliPath] = useState("");
  const [appSettings, setAppSettings] = useState<AppSettings | null>(null);

  useEffect(() => {
    void getAppSettings()
      .then((settings) => {
        setAppSettings(settings);
        setCliPath(settings.cursorCliPath ?? "");
      })
      .catch(() => {
        /* optional in some hosts */
      });
  }, []);

  const featureEntries = useMemo(() => {
    if (!capability) return [];
    return Object.entries(capability.features).map(([key, value]) => ({
      key,
      value: Boolean(value),
    }));
  }, [capability]);

  async function onProbe() {
    setBusy(true);
    setError(null);
    try {
      const report = await probeCursorCapability();
      onCapabilityChange(report);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onSaveCliPath() {
    setBusy(true);
    setError(null);
    try {
      const settings = await setCursorCliPath(cliPath.trim() || null);
      setAppSettings(settings);
      const report = await probeCursorCapability();
      onCapabilityChange(report);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onPreview() {
    setBusy(true);
    setError(null);
    try {
      const result = await previewCursorCommand({
        workspace: "C:\\managed\\workspace",
        prompt,
        model: capability?.models[0]?.id ?? "composer-2.5",
        apiKey,
        force: true,
        trust: true,
      });
      setPreview(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onRefreshModels() {
    setBusy(true);
    setError(null);
    try {
      const models = await listCursorModels();
      if (capability) {
        onCapabilityChange({
          ...capability,
          models: models.models,
        });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onRebind() {
    setBusy(true);
    setError(null);
    try {
      const settings = await rebindAbortShortcut(shortcut);
      onAbortSettingsChange(settings);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section
      className="tiamat-panel tiamat-settings"
      aria-label="Cursor settings"
      data-testid="settings-panel"
    >
      <div className="tiamat-settings-header">
        <h2>Cursor status</h2>
        <button
          type="button"
          data-testid="cursor-reprobe"
          onClick={() => void onProbe()}
          disabled={busy}
        >
          Re-probe
        </button>
      </div>

      {error ? (
        <p className="tiamat-error" role="alert">
          {error}
        </p>
      ) : null}

      <div data-testid="configured-cli">
        <h3>Configured CLI</h3>
        <label className="tiamat-field">
          Executable path
          <input
            data-testid="cursor-cli-path-input"
            value={cliPath}
            onChange={(event) => setCliPath(event.target.value)}
            placeholder="C:\path\to\agent.cmd"
          />
        </label>
        <button
          type="button"
          data-testid="cursor-cli-path-save"
          disabled={busy}
          onClick={() => void onSaveCliPath()}
        >
          Save and probe
        </button>
        {appSettings?.cursorCliPath ? (
          <p className="tiamat-muted" data-testid="cursor-cli-path-saved">
            Saved: {appSettings.cursorCliPath}
          </p>
        ) : (
          <p className="tiamat-muted">Using PATH / known installs when empty.</p>
        )}
      </div>

      {abortSettings ? (
        <div data-testid="abort-settings">
          <h3>Global abort</h3>
          <p data-testid="abort-status-text">{formatAbortStatus(abortSettings)}</p>
          <label className="tiamat-field">
            Shortcut
            <input
              data-testid="abort-shortcut-input"
              value={shortcut}
              onChange={(event) => setShortcut(event.target.value)}
            />
          </label>
          <button
            type="button"
            data-testid="abort-rebind"
            disabled={busy}
            onClick={() => void onRebind()}
          >
            Rebind shortcut
          </button>
          <p className="tiamat-muted" data-testid="tray-fallback-flag">
            Tray fallback: {abortSettings.trayFallbackEnabled ? "enabled" : "off"}
          </p>
        </div>
      ) : null}

      {capability ? (
        <div className="tiamat-settings-body" data-testid="cursor-capability">
          <p data-testid="cursor-capability-status">
            Status: <strong>{capability.status}</strong>
          </p>
          <p data-testid="cursor-capability-message">{capability.message}</p>
          <dl className="tiamat-settings-meta">
            <div>
              <dt>Executable</dt>
              <dd data-testid="cursor-executable">
                {capability.executable ?? "—"}
              </dd>
            </div>
            <div>
              <dt>Version</dt>
              <dd data-testid="cursor-version">{capability.version ?? "—"}</dd>
            </div>
            <div>
              <dt>Auth</dt>
              <dd data-testid="cursor-auth">{capability.auth}</dd>
            </div>
            <div>
              <dt>Approval mode</dt>
              <dd data-testid="cursor-approval">
                {hasNoninteractiveApproval(capability.features)
                  ? "force/auto-review available"
                  : "missing noninteractive approval"}
              </dd>
            </div>
          </dl>

          <h3>Discovered features</h3>
          <ul className="tiamat-feature-list" data-testid="cursor-features">
            {featureEntries.map((entry) => (
              <li
                key={entry.key}
                data-testid={`cursor-feature-${entry.key}`}
                data-enabled={entry.value ? "true" : "false"}
              >
                {entry.key}: {entry.value ? "yes" : "no"}
              </li>
            ))}
          </ul>

          <div className="tiamat-settings-models">
            <div className="tiamat-settings-header">
              <h3>Models</h3>
              <button
                type="button"
                data-testid="cursor-refresh-models"
                onClick={() => void onRefreshModels()}
                disabled={busy}
              >
                Refresh models
              </button>
            </div>
            <ul data-testid="cursor-models">
              {capability.models.map((model) => (
                <li key={model.id} data-testid="cursor-model">
                  {model.label}
                </li>
              ))}
            </ul>
          </div>

          <h3>Dry-run preview</h3>
          <p className="tiamat-muted">
            Builds an argument array only. Spawned is always no.
          </p>
          <label className="tiamat-field">
            API key (redacted in preview)
            <input
              data-testid="cursor-api-key"
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
            />
          </label>
          <label className="tiamat-field">
            Prompt
            <textarea
              data-testid="cursor-preview-prompt"
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              rows={3}
            />
          </label>
          <button
            type="button"
            data-testid="cursor-dry-run"
            onClick={() => void onPreview()}
            disabled={busy}
          >
            Preview command
          </button>
          {preview ? (
            <div data-testid="cursor-command-preview">
              <p data-testid="cursor-preview-spawned">
                Spawned: {preview.spawned ? "yes" : "no"}
              </p>
              <pre data-testid="cursor-preview-argv">
                {preview.argv.join("\n")}
              </pre>
              <pre data-testid="cursor-preview-stdin">{preview.stdinPreview}</pre>
            </div>
          ) : null}
        </div>
      ) : (
        <p className="tiamat-muted">Probe Cursor CLI to populate status.</p>
      )}
    </section>
  );
}
