import { Download, LoaderCircle, RefreshCw } from "lucide-react";
import { useState } from "react";
import { Logo } from "../components/Logo";
import { errorMessage } from "../lib/api";
import { checkForUpdate, installUpdate, Update } from "../lib/updates";
import { useStore } from "../store";

export function AboutPage() {
  const { t, info, toast, recording } = useStore();
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState<number | null>(null);

  const check = async () => {
    setChecking(true);
    try {
      const found = await checkForUpdate();
      setUpdate(found);
      if (!found) toast(t("update.latest"));
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setChecking(false);
    }
  };

  const install = async () => {
    if (!update) return;
    setProgress(0);
    try {
      await installUpdate(update, setProgress);
    } catch (e) {
      setProgress(null);
      toast(errorMessage(e), "error");
    }
  };

  return (
    <div className="page page--narrow">
      <section className="about">
        <Logo size={96} />
        <h1>ScreenLink Studio</h1>
        <p>{t("about.tagline")}</p>
      </section>
      <section className="card">
        <dl className="facts">
          <dt>{t("about.version")}</dt>
          <dd>{info?.version}</dd>
          <dt>{t("about.platform")}</dt>
          <dd>
            {info?.platform} · {info?.arch}
          </dd>
          <dt>{t("about.scrcpy")}</dt>
          <dd>{info?.scrcpyVersion}</dd>
          <dt>{t("about.adb")}</dt>
          <dd>{info?.adbVersion ?? "—"}</dd>
        </dl>
      </section>
      <section className="card">
        {update ? (
          <button className="button button--primary" disabled={progress !== null || recording.active} onClick={install}>
            {progress === null ? <Download size={15} /> : <LoaderCircle size={15} className="spin" />}
            {progress === null ? `${t("update.install")} (v${update.version})` : `${Math.round(progress * 100)}%`}
          </button>
        ) : (
          <button className="button" disabled={checking} onClick={check}>
            <RefreshCw size={15} className={checking ? "spin" : ""} />
            {checking ? t("update.checking") : t("update.check")}
          </button>
        )}
      </section>
      <section className="card">
        <h3>{t("about.notices")}</h3>
        <p className="muted">{t("about.noticesBody")}</p>
      </section>
    </div>
  );
}
