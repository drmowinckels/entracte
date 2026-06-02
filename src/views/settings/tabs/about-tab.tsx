import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getVersion } from "@tauri-apps/api/app";
import { useUpdateCheck } from "../hooks/use-update-check";
import type { UseSupporter } from "../hooks/use-supporter";
import { usePlatformCapabilities } from "../../../lib/platform";
import { writeToClipboard } from "../utils";

const TOAST_MS = 3000;
const SUPPORTER_CHECKOUT_URL =
  "https://shop.drmowinckels.io/checkout/buy/40af6bbf-154c-4321-948e-3329b1176319";

export function AboutTab({ supporter }: { supporter: UseSupporter }) {
  const [version, setVersion] = useState("");
  const [diagnosticsStatus, setDiagnosticsStatus] = useState("");
  const [licenseInput, setLicenseInput] = useState("");
  const update = useUpdateCheck();
  const caps = usePlatformCapabilities();

  const onVerify = async () => {
    const trimmed = licenseInput.trim();
    if (!trimmed) return;
    const ok = await supporter.verify(trimmed);
    if (ok) setLicenseInput("");
  };

  useEffect(() => {
    let cancelled = false;
    getVersion()
      .then((v) => {
        if (!cancelled) setVersion(v);
      })
      .catch((e) => console.error("getVersion failed", e));
    return () => {
      cancelled = true;
    };
  }, []);

  const flashDiagnostics = (msg: string) => {
    setDiagnosticsStatus(msg);
    window.setTimeout(() => setDiagnosticsStatus(""), TOAST_MS);
  };

  const onCopyDiagnosticsReport = async () => {
    try {
      const report = await invoke<string>("build_diagnostics_report");
      const ok = await writeToClipboard(report);
      flashDiagnostics(
        ok ? "Report copied to clipboard" : "Clipboard copy failed",
      );
    } catch (e) {
      console.error("copy diagnostics report failed", e);
      flashDiagnostics("Could not build report");
    }
  };

  return (
    <>
      <h2>About</h2>
      <section>
        <div className="about-title-row">
          <p className="about-title">Entracte</p>
          <button onClick={update.check} disabled={update.checking}>
            {update.checking ? "Checking…" : "Check for updates"}
          </button>
        </div>
        <p className="about-meta">Version {version || "—"}</p>
        <p className="about-meta">Cross-platform break reminder.</p>
        <p className="about-meta">Apache 2.0 licensed.</p>
        {update.info && update.info.has_update && update.info.release_url && (
          <>
            <p className="about-meta">
              Update available: <strong>{update.info.latest}</strong> (you have{" "}
              {update.info.current}).{" "}
              <button
                className="link"
                onClick={() => openUrl(update.info!.release_url!)}
              >
                Open release page
              </button>
            </p>
            {caps.installerUnsignedWarning && (
              <p className="about-meta">
                The Windows installer isn't Authenticode-signed yet, so
                SmartScreen will warn — click <em>More info → Run anyway</em> to
                proceed.
              </p>
            )}
          </>
        )}
        {update.info && !update.info.has_update && (
          <p className="about-meta">
            You're on the latest version ({update.info.current}).
          </p>
        )}
        {update.error && (
          <p className="about-meta">Check failed: {update.error}</p>
        )}
      </section>

      <h2>Supporter{supporter.status.is_supporter ? " ✓" : ""}</h2>
      <section>
        {supporter.status.is_supporter ? (
          <>
            <p className="about-meta">
              Thank you. The customisation pack is unlocked.
            </p>
            <p className="about-meta">
              License: <code>{supporter.status.masked_key}</code>
            </p>
            <div className="actions inline">
              <button
                className="secondary"
                onClick={() => supporter.remove()}
                disabled={supporter.pending}
              >
                Remove license
              </button>
            </div>
          </>
        ) : (
          <>
            <p className="about-meta">
              Entracte is free to use. The customisation pack — custom overlay
              colours, rotating themes, custom sounds, custom CSS, and editable
              break hints — is unlocked by becoming a supporter once, forever.
            </p>
            <div className="actions inline">
              <button onClick={() => openUrl(SUPPORTER_CHECKOUT_URL)}>
                Become a supporter →
              </button>
            </div>
            <p className="about-meta">
              Already have a license? Paste it below and click Verify.
            </p>
            <div className="supporter-entry">
              <input
                type="text"
                value={licenseInput}
                onChange={(e) => setLicenseInput(e.target.value)}
                placeholder="License key"
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
                disabled={supporter.pending}
                onKeyDown={(e) => {
                  if (e.key === "Enter") onVerify();
                }}
              />
              <button
                onClick={onVerify}
                disabled={supporter.pending || licenseInput.trim() === ""}
              >
                {supporter.pending ? "Verifying…" : "Verify"}
              </button>
            </div>
          </>
        )}
        {supporter.message && (
          <p className="diagnostics-status">{supporter.message}</p>
        )}
      </section>

      <div className="section-heading">
        <h2>Author</h2>
        <button
          onClick={() => openUrl("https://buymeacoffee.com/drmowinckels")}
        >
          ☕ Buy me a coffee
        </button>
      </div>
      <section>
        <p className="about-meta">
          Built by <strong>Dr. Athanasia M. Mowinckel</strong>
        </p>
        <p className="about-meta">
          Senior staff engineer & researcher, working on tools for reproducible
          science and developer wellbeing.
        </p>
      </section>

      <div className="section-heading">
        <h2>Diagnostics</h2>
        <button onClick={onCopyDiagnosticsReport}>
          Copy diagnostics report
        </button>
      </div>
      <section>
        {diagnosticsStatus && (
          <p className="diagnostics-status">{diagnosticsStatus}</p>
        )}
        <p className="diagnostics-hint">
          Click <strong>Copy diagnostics report</strong> when filing an issue at{" "}
          <button
            className="link"
            onClick={() =>
              openUrl("https://github.com/drmowinckels/entracte/issues")
            }
          >
            github.com/drmowinckels/entracte/issues
          </button>{" "}
          — it includes app version, settings, and the last 50 KB of logs.
        </p>
      </section>
    </>
  );
}
