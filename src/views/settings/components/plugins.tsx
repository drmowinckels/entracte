import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { InfoTip } from "./info-tip";
import type { InstallOutcome, PluginSummary } from "../types";

type Status = { kind: "ok" | "err"; message: string } | null;

const FILTER = [{ name: "Entracte plugin", extensions: ["json"] }];

// Local-only plugin install/uninstall (#156). This slice ships content
// providers: a signed plugin file whose ideas/routines merge into the active
// profile on install (after a native confirmation dialog) and are removed
// exactly on uninstall. Detector/export plugins need the wasm runtime and are
// rejected by the backend for now.
export function Plugins({
  // Reload settings after install/uninstall so merged ideas/routines show
  // (or disappear) immediately elsewhere in Settings.
  reload,
}: {
  reload: () => Promise<unknown>;
}) {
  const [plugins, setPlugins] = useState<PluginSummary[]>([]);
  const [status, setStatus] = useState<Status>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<PluginSummary[]>("list_plugins");
      setPlugins(Array.isArray(list) ? list : []);
    } catch (e) {
      setStatus({ kind: "err", message: `Could not list plugins: ${e}` });
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onInstall = async () => {
    setStatus(null);
    try {
      const path = await openDialog({
        multiple: false,
        directory: false,
        filters: FILTER,
      });
      if (typeof path !== "string" || !path) return;
      setBusy(true);
      const outcome = await invoke<InstallOutcome>("install_plugin", { path });
      await Promise.all([refresh(), reload()]);
      const images = outcome.images_added ?? 0;
      const message =
        outcome.kind === "content"
          ? `Installed "${outcome.name}" — added ${outcome.hints_added} idea${
              outcome.hints_added === 1 ? "" : "s"
            } and ${outcome.routines_added} routine${
              outcome.routines_added === 1 ? "" : "s"
            }${
              images > 0
                ? ` with ${images} image${images === 1 ? "" : "s"}`
                : ""
            }.`
          : `Installed "${outcome.name}".`;
      setStatus({ kind: "ok", message });
    } catch (e) {
      setStatus({ kind: "err", message: `Install failed: ${e}` });
    } finally {
      setBusy(false);
    }
  };

  const onUninstall = async (plugin: PluginSummary) => {
    setStatus(null);
    try {
      setBusy(true);
      await invoke("uninstall_plugin", { id: plugin.id });
      await Promise.all([refresh(), reload()]);
      setStatus({ kind: "ok", message: `Removed "${plugin.name}".` });
    } catch (e) {
      setStatus({ kind: "err", message: `Uninstall failed: ${e}` });
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <p className="placeholder">
        Install a local plugin file to add break ideas and routines from the
        community.
        <InfoTip text="Plugins are local files you choose yourself — no store, no account, no network. Installing shows a confirmation dialog with the plugin's name, author, and signing key. Uninstalling removes exactly what it added." />
      </p>
      <p className="placeholder plugin-warning">
        ⚠ Only install plugin files from sources you trust.
      </p>
      <div className="actions inline">
        <button
          type="button"
          className="secondary"
          onClick={onInstall}
          disabled={busy}
        >
          {busy ? "Working…" : "Install plugin…"}
        </button>
      </div>
      {plugins.length > 0 ? (
        <ul className="plugin-list">
          {plugins.map((p) => (
            <li key={p.id} className="plugin-row">
              <div className="plugin-meta">
                <span className="plugin-name">{p.name}</span>
                <span className="plugin-sub">
                  {p.author ? `${p.author} · ` : ""}v{p.version} ·{" "}
                  {p.hints_added} idea{p.hints_added === 1 ? "" : "s"},{" "}
                  {p.routines_added} routine
                  {p.routines_added === 1 ? "" : "s"}
                </span>
              </div>
              <button
                type="button"
                className="secondary"
                onClick={() => onUninstall(p)}
                disabled={busy}
                aria-label={`Uninstall ${p.name}`}
              >
                Uninstall
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="placeholder">No plugins installed.</p>
      )}
      {status && (
        <p
          className={status.kind === "err" ? "content-pack-err" : "placeholder"}
          role="status"
          aria-live="polite"
        >
          {status.message}
        </p>
      )}
    </>
  );
}
