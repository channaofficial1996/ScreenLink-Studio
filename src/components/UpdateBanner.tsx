import { Download, LoaderCircle, X } from "lucide-react";
import { useEffect, useState } from "react";
import { errorMessage } from "../lib/api";
import { checkForUpdate, installUpdate, Update } from "../lib/updates";
import { useStore } from "../store";

const STARTUP_DELAY_MS = 4000;

/** Offers updates; never installs on its own, since that would end a live session or recording. */
export function UpdateBanner() {
  const { settings, recording, toast, t } = useStore();
  const [update, setUpdate] = useState<Update | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const enabled = settings?.autoCheckUpdates ?? false;

  useEffect(() => {
    if (!enabled) return;
    const timer = window.setTimeout(() => {
      // Being offline at startup is normal, so a failed check stays silent here.
      checkForUpdate().then(setUpdate).catch(() => {});
    }, STARTUP_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [enabled]);

  if (!update || dismissed) return null;

  const install = async () => {
    setProgress(0);
    try {
      await installUpdate(update, setProgress);
    } catch (e) {
      setProgress(null);
      toast(errorMessage(e), "error");
    }
  };

  return (
    <div className="update-banner" role="status">
      <span>
        {t("update.available")} <strong>v{update.version}</strong>
      </span>
      {progress === null ? (
        <>
          <button className="button button--primary" disabled={recording.active} onClick={install}>
            <Download size={15} />
            {t("update.install")}
          </button>
          <button className="icon-button" aria-label={t("update.later")} title={t("update.later")} onClick={() => setDismissed(true)}>
            <X size={16} />
          </button>
        </>
      ) : (
        <span className="update-banner__progress">
          <LoaderCircle size={15} className="spin" />
          {Math.round(progress * 100)}%
        </span>
      )}
    </div>
  );
}
