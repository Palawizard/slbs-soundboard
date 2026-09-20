import { useCallback, useEffect, useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

type Phase = "idle" | "available" | "downloading" | "ready" | "failed";

/// Checks GitHub releases on start-up and lets the user install the new version
/// without downloading anything by hand. Failures stay silent: an unreachable
/// update endpoint must never block a local soundboard.
export function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void check()
      .then((result) => {
        if (!cancelled && result) {
          setUpdate(result);
          setPhase("available");
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  const install = useCallback(async () => {
    if (!update) return;
    setPhase("downloading");
    setError(null);
    let downloaded = 0;
    let total = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (total > 0) setProgress(Math.min(100, Math.round((downloaded / total) * 100)));
        }
        if (event.event === "Finished") setProgress(100);
      });
      setPhase("ready");
      await relaunch();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setPhase("failed");
    }
  }, [update]);

  if (!update || dismissed) return null;

  return (
    <div className="update-banner" role="status">
      <div>
        <strong>Version {update.version} disponible</strong>
        <span>
          {phase === "downloading"
            ? `Téléchargement… ${progress} %`
            : phase === "ready"
              ? "Installation en cours, l’application va redémarrer."
              : phase === "failed"
                ? (error ?? "La mise à jour a échoué.")
                : "Installez-la pour rester à jour avec vos amis."}
        </span>
      </div>
      <div className="update-actions">
        <button
          type="button"
          className="primary-button"
          onClick={() => void install()}
          disabled={phase === "downloading" || phase === "ready"}
        >
          {phase === "failed" ? "Réessayer" : "Installer"}
        </button>
        <button
          type="button"
          className="secondary-button"
          onClick={() => setDismissed(true)}
          disabled={phase === "downloading" || phase === "ready"}
        >
          Plus tard
        </button>
      </div>
    </div>
  );
}
