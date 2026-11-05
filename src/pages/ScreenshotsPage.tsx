import { Camera } from "lucide-react";
import { ScreenshotGrid, useMedia } from "../components/MediaList";
import { useStore } from "../store";

export function ScreenshotsPage() {
  const { t, selected, mirror, takeScreenshot } = useStore();
  const items = useMedia("screenshot");
  const iosNeedsLive = selected?.platform === "ios" && mirror.phase !== "running";
  const can = !!selected?.capabilities.screenshot && !iosNeedsLive;

  return (
    <div className="page">
      <header className="page__head">
        <div>
          <h1>{t("shots.title")}</h1>
          <p>{can ? t("shots.subtitle") : t("shots.needDevice")}</p>
        </div>
        <button className="button button--primary" disabled={!can} onClick={takeScreenshot}>
          <Camera size={15} />
          {t("shots.take")}
        </button>
      </header>
      {items.length === 0 ? <div className="empty">{t("shots.empty")}</div> : <ScreenshotGrid items={items} />}
    </div>
  );
}
