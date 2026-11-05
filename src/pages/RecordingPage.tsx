import { open } from "@tauri-apps/plugin-dialog";
import { Circle, FolderCog, FolderOpen, Square } from "lucide-react";
import { RecordingList, useMedia } from "../components/MediaList";
import { api } from "../lib/api";
import { useStore } from "../store";
import { RecordTimer } from "./MirrorPage";

export function chooseFolder(current: string | undefined): Promise<string | null> {
  return open({ directory: true, defaultPath: current });
}

export function RecordingPage() {
  const { t, recording, mirror, mirroredDevice, toggleRecording, settings, saveSettings } = useStore();
  const items = useMedia("recording");
  const canRecord = mirror.phase === "running" && !!mirroredDevice?.capabilities.record;

  const changeFolder = async () => {
    const dir = await chooseFolder(settings?.saveDirectory);
    if (dir) await saveSettings({ saveDirectory: dir });
  };

  return (
    <div className="page">
      <header className="page__head">
        <div>
          <h1>{t("recording.title")}</h1>
          <p>{t("recording.subtitle")}</p>
        </div>
      </header>

      <section className={`card record-hero${recording.active ? " record-hero--on" : ""}`}>
        <div className="record-hero__status">
          <i className="record-hero__light" />
          <div>
            <strong>{recording.active ? t("recording.active") : t("recording.idle")}</strong>
            {recording.active ? (
              <span className="record-hero__timer">
                <RecordTimer startedAt={recording.startedAtMs} />
              </span>
            ) : (
              !canRecord && <span className="muted">{t("recording.needMirror")}</span>
            )}
          </div>
        </div>
        <button className={`button ${recording.active ? "button--danger" : "button--primary"}`} disabled={!canRecord} onClick={toggleRecording}>
          {recording.active ? <Square size={15} /> : <Circle size={13} fill="currentColor" />}
          {recording.active ? t("mirror.stopRecording") : t("recording.start")}
        </button>
      </section>

      <section className="card folder-row">
        <div>
          <span className="muted">{t("recording.saveTo")}</span>
          <p className="mono folder-row__path">{settings?.saveDirectory}</p>
        </div>
        <div className="actions">
          <button className="button" onClick={changeFolder}>
            <FolderCog size={15} />
            {t("recording.change")}
          </button>
          <button className="button" onClick={() => void api.openSaveDirectory()}>
            <FolderOpen size={15} />
            {t("recording.open")}
          </button>
        </div>
      </section>

      <h2 className="section-title">{t("recording.library")}</h2>
      {items.length === 0 ? <div className="empty">{t("recording.empty")}</div> : <RecordingList items={items} />}
      <p className="footnote">{t("recording.rotateNote")}</p>
    </div>
  );
}
