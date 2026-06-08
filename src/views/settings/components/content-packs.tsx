import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  open as openDialog,
  save as saveDialog,
} from "@tauri-apps/plugin-dialog";
import { InfoTip } from "./info-tip";
import { localDateString } from "../../../lib/time";
import type { ContentPackSummary } from "../types";

type Status = { kind: "ok" | "err"; message: string } | null;

const FILTER = [{ name: "Entracte content pack", extensions: ["json"] }];

// Import/export of local content packs (#155): a plain JSON bundle of break
// ideas + guided routines the user picks from disk. Import is additive
// (duplicates skipped); export captures the current pools + custom routines.
export function ContentPacks({
  // Reload settings after an import so the new ideas/routines show immediately.
  reload,
}: {
  reload: () => Promise<unknown>;
}) {
  const [status, setStatus] = useState<Status>(null);

  const onExport = async () => {
    setStatus(null);
    try {
      const today = localDateString();
      const path = await saveDialog({
        defaultPath: `entracte-content-pack-${today}.json`,
        filters: FILTER,
      });
      if (typeof path !== "string" || !path) return;
      await invoke("export_content_pack", {
        path,
        name: `Entracte content pack (${today})`,
      });
      setStatus({ kind: "ok", message: `Exported to ${path}` });
    } catch (e) {
      setStatus({ kind: "err", message: `Export failed: ${e}` });
    }
  };

  const onImport = async () => {
    setStatus(null);
    try {
      const path = await openDialog({
        multiple: false,
        directory: false,
        filters: FILTER,
      });
      if (typeof path !== "string" || !path) return;
      const summary = await invoke<ContentPackSummary>("import_content_pack", {
        path,
      });
      await reload();
      const ideas = `${summary.hints_added} idea${summary.hints_added === 1 ? "" : "s"}`;
      const routines = `${summary.routines_added} routine${summary.routines_added === 1 ? "" : "s"}`;
      setStatus({ kind: "ok", message: `Imported ${ideas} and ${routines}.` });
    } catch (e) {
      setStatus({ kind: "err", message: `Import failed: ${e}` });
    }
  };

  return (
    <>
      <p className="placeholder">
        Share or back up your break ideas and guided routines as a local file.
        <InfoTip text="A content pack is a plain JSON file. Importing adds its ideas and routines to your pools without removing anything you already have; exact duplicates are skipped." />
      </p>
      <div className="actions inline">
        <button type="button" className="secondary" onClick={onImport}>
          Import content pack…
        </button>
        <button type="button" className="secondary" onClick={onExport}>
          Export content pack…
        </button>
      </div>
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
