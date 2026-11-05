import { FolderOpen } from "lucide-react";
import { ReactNode } from "react";
import { languages } from "../lib/i18n";
import type { Settings } from "../lib/types";
import { useStore } from "../store";
import { chooseFolder } from "./RecordingPage";

const RESOLUTIONS = [0, 1920, 1600, 1280, 1080, 720];
const FPS = [15, 24, 30, 60, 90];

export function SettingsPage() {
  const { settings, saveSettings, t } = useStore();
  if (!settings) return null;

  const toggle = (key: keyof Settings, label: string, hint?: string) => (
    <Row label={label} hint={hint}>
      <label className="switch">
        <input type="checkbox" aria-label={label} checked={Boolean(settings[key])} onChange={(e) => void saveSettings({ [key]: e.target.checked })} />
        <span />
      </label>
    </Row>
  );

  const browse = async () => {
    const dir = await chooseFolder(settings.saveDirectory);
    if (dir) await saveSettings({ saveDirectory: dir });
  };

  return (
    <div className="page page--narrow">
      <header className="page__head">
        <div>
          <h1>{t("settings.title")}</h1>
          <p>{t("settings.subtitle")}</p>
        </div>
      </header>

      <section className="card settings">
        <h3>{t("settings.video")}</h3>
        <Row label={t("settings.resolution")} hint={t("settings.resolutionHint")}>
          <select className="field" aria-label={t("settings.resolution")} value={settings.resolution} onChange={(e) => void saveSettings({ resolution: Number(e.target.value) })}>
            {RESOLUTIONS.map((r) => (
              <option key={r} value={r}>
                {r === 0 ? t("settings.resolution.native") : `${r} px`}
              </option>
            ))}
          </select>
        </Row>
        <Row label={t("settings.fps")} hint={t("settings.fpsHint")}>
          <select className="field" aria-label={t("settings.fps")} value={settings.fps} onChange={(e) => void saveSettings({ fps: Number(e.target.value) })}>
            {FPS.map((f) => (
              <option key={f} value={f}>
                {f} FPS
              </option>
            ))}
          </select>
        </Row>
        <Row label={t("settings.bitrate")} hint={t("settings.bitrateHint")}>
          <div className="range">
            <input
              type="range"
              min={1}
              max={40}
              value={settings.bitrateMbps}
              aria-label={t("settings.bitrate")}
              onChange={(e) => void saveSettings({ bitrateMbps: Number(e.target.value) })}
            />
            <output className="mono">{settings.bitrateMbps} Mbps</output>
          </div>
        </Row>
        <p className="footnote">{t("settings.applyNext")}</p>
      </section>

      <section className="card settings">
        <h3>{t("settings.audio")}</h3>
        {toggle("audioForwarding", t("settings.audioForwarding"), t("settings.audioHint"))}
      </section>

      <section className="card settings">
        <h3>{t("settings.device")}</h3>
        {toggle("stayAwake", t("settings.stayAwake"))}
        {toggle("showTouches", t("settings.showTouches"))}
        {toggle("autoConnect", t("settings.autoConnect"))}
      </section>

      <section className="card settings">
        <h3>{t("settings.window")}</h3>
        {toggle("alwaysOnTop", t("settings.alwaysOnTop"))}
        {toggle("launchAtLogin", t("settings.launchAtLogin"))}
        {toggle("startMinimized", t("settings.startMinimized"))}
        {toggle("autoCheckUpdates", t("settings.autoCheckUpdates"))}
        <Row label={t("settings.language")}>
          <select className="field" aria-label={t("settings.language")} value={settings.language} onChange={(e) => void saveSettings({ language: e.target.value })}>
            {languages.map((l) => (
              <option key={l.code} value={l.code}>
                {l.label}
              </option>
            ))}
          </select>
        </Row>
        <Row label={t("settings.theme")}>
          <select className="field" aria-label={t("settings.theme")} value={settings.theme} onChange={(e) => void saveSettings({ theme: e.target.value })}>
            <option value="dark">{t("settings.theme.dark")}</option>
            <option value="light">{t("settings.theme.light")}</option>
            <option value="system">{t("settings.theme.system")}</option>
          </select>
        </Row>
      </section>

      <section className="card settings">
        <h3>{t("settings.storage")}</h3>
        <Row label={t("settings.saveDirectory")} hint={settings.saveDirectory}>
          <button className="button" onClick={browse}>
            <FolderOpen size={15} />
            {t("settings.browse")}
          </button>
        </Row>
      </section>
    </div>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="setting">
      <div>
        <span className="setting__label">{label}</span>
        {hint && <span className="setting__hint">{hint}</span>}
      </div>
      {children}
    </div>
  );
}
