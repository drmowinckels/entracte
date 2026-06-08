import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { HOOK_TEMPLATES } from "../../../lib/hook-templates";
import { HOOK_EVENTS } from "../constants";
import type { HookConfig, HookEvent, HookTestOutcome } from "../types";

export type HookRowProps = {
  hook: HookConfig;
  onChange: (patch: Partial<HookConfig>) => void;
  onRemove: () => void;
  // Injectable for tests; defaults to the `test_hook` IPC command.
  testHook?: (command: string) => Promise<HookTestOutcome>;
};

function HookTestResult({ result }: { result: HookTestOutcome }) {
  return (
    <div className="hook-test-result" role="status" aria-live="polite">
      {result.error ? (
        <p className="hook-test-error">⚠ {result.error}</p>
      ) : (
        <>
          <p
            className={
              result.exit_code === 0 ? "hook-test-ok" : "hook-test-fail"
            }
          >
            {result.exit_code === 0
              ? "✓ Exited 0"
              : `Exited with code ${result.exit_code ?? "unknown"}`}
          </p>
          {result.stdout && (
            <pre className="hook-test-stream" aria-label="stdout">
              {result.stdout}
            </pre>
          )}
          {result.stderr && (
            <pre className="hook-test-stream" aria-label="stderr">
              {result.stderr}
            </pre>
          )}
          {!result.stdout && !result.stderr && (
            <p className="hook-test-empty">No output.</p>
          )}
        </>
      )}
    </div>
  );
}

// One editable hook: event picker, a command field with a template inserter,
// an on/off toggle, a Test button that runs the command once and shows its
// output, and Remove.
export function HookRow({ hook, onChange, onRemove, testHook }: HookRowProps) {
  const runTest =
    testHook ??
    ((command: string) => invoke<HookTestOutcome>("test_hook", { command }));
  const [result, setResult] = useState<HookTestOutcome | null>(null);
  const [running, setRunning] = useState(false);

  const onTest = async () => {
    if (!hook.command.trim()) return;
    setRunning(true);
    setResult(null);
    try {
      setResult(await runTest(hook.command));
    } catch (e) {
      setResult({
        ok: false,
        exit_code: null,
        stdout: "",
        stderr: "",
        error: String(e),
      });
    } finally {
      setRunning(false);
    }
  };

  const insertTemplate = (id: string) => {
    const tpl = HOOK_TEMPLATES.find((t) => t.id === id);
    if (tpl) onChange({ command: tpl.command, event: tpl.event });
  };

  return (
    <div className="hook-row-wrap">
      <div className="hook-row">
        <select
          aria-label="Hook event"
          value={hook.event}
          onChange={(e) => onChange({ event: e.target.value as HookEvent })}
        >
          {HOOK_EVENTS.map((opt) => (
            <option key={opt.id} value={opt.id}>
              {opt.label}
            </option>
          ))}
        </select>
        <input
          type="text"
          aria-label="Hook command"
          className="hook-command"
          placeholder={`e.g. sh -c "osascript -e 'tell app \\"Music\\" to pause'"`}
          value={hook.command}
          onChange={(e) => onChange({ command: e.target.value })}
        />
        <label className="hook-toggle">
          <input
            type="checkbox"
            checked={hook.enabled}
            onChange={(e) => onChange({ enabled: e.target.checked })}
          />
          <span>On</span>
        </label>
        <button
          type="button"
          className="secondary"
          disabled={running || hook.command.trim().length === 0}
          onClick={onTest}
        >
          {running ? "Testing…" : "Test"}
        </button>
        <button
          type="button"
          className="secondary hook-remove"
          onClick={onRemove}
        >
          Remove
        </button>
      </div>
      <div className="hook-templates">
        {/* A controlled "action" select: it always shows the prompt and
            inserts on pick, so it never holds a stale selection. */}
        <select
          aria-label="Insert template"
          value=""
          onChange={(e) => insertTemplate(e.target.value)}
        >
          <option value="">Insert template…</option>
          {HOOK_TEMPLATES.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
      </div>
      {result && <HookTestResult result={result} />}
    </div>
  );
}
