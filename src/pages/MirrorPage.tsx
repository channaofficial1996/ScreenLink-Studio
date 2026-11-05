import { Camera, Circle, Cable, LoaderCircle, Maximize, Play, RotateCcw, Smartphone, Square, Usb } from "lucide-react";
import { useEffect, useState } from "react";
import { Stage } from "../components/Stage";
import { StatusBadge } from "../components/StatusBadge";
import { compatText } from "../lib/compat";
import { formatDuration, formatResolution } from "../lib/format";
import type { TKey } from "../lib/i18n";
import type { Side } from "../lib/viewMode";
import { useStore } from "../store";

interface Props {
  fullscreen: boolean;
  phoneOnly: boolean;
  panelOpen: boolean;
  side: Side;
  onFullscreen(on: boolean): void;
  onPhoneOnly(): void;
  onExitPhoneOnly(): void;
  onDock(side: Side): void;
}

export function MirrorPage({ fullscreen, phoneOnly, panelOpen, side, onFullscreen, onPhoneOnly, onExitPhoneOnly, onDock }: Props) {
  const { snapshot, selected, select, mirror, stats, recording, firstFrame, settings, t, startMirror, stopMirror, restartMirror, takeScreenshot, toggleRecording } = useStore();
  const devices = snapshot.devices;
  const running = mirror.phase === "running";
  const busy = mirror.phase === "starting" || mirror.phase === "stopping";
  const isSelectedLive = running && mirror.serial === selected?.id;
  const language = settings?.language ?? "en";

  const immersive = fullscreen || phoneOnly;

  return (
    <div className={`mirror${immersive ? " mirror--fullscreen" : ""}${panelOpen ? "" : " mirror--no-panel"}`}>
      <Stage fullscreen={fullscreen} phoneOnly={phoneOnly} side={side} onFullscreen={onFullscreen} onExitPhoneOnly={onExitPhoneOnly} onDock={onDock} />

      <aside className="panel" hidden={immersive || !panelOpen}>
        <section className="card">
          <div className="card__head">
            <h3>{t("mirror.device")}</h3>
            {selected && <StatusBadge state={selected.state} />}
          </div>
          {devices.length > 1 && (
            <select
              className="field"
              value={selected?.id ?? ""}
              aria-label={t("mirror.choose")}
              disabled={running || busy}
              onChange={(e) => select(e.target.value)}
            >
              {devices.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name} · {d.id}
                </option>
              ))}
            </select>
          )}
          {selected ? (
            <>
              <p className="device-name">{selected.name}</p>
              <dl className="facts">
                <dt>{t("mirror.type")}</dt>
                <dd>{t(`platform.${selected.platform}`)}</dd>
                {selected.osVersion && (
                  <>
                    <dt>{t("mirror.system")}</dt>
                    <dd>{selected.osVersion}</dd>
                  </>
                )}
                <dt>{t("mirror.serial")}</dt>
                <dd className="mono">{selected.id}</dd>
                <dt>{t("mirror.usb")}</dt>
                <dd>
                  <span className={`usb-link${selected.state === "disconnected" ? "" : " usb-link--on"}`}>
                    <Usb size={14} />
                    {selected.state === "disconnected" ? t("mirror.usbInactive") : t("mirror.usbActive")}
                  </span>
                </dd>
              </dl>
              {selected.compatibility && (
                <p className={`notice notice--${selected.compatibility.level}`}>{compatText(selected.compatibility, language)}</p>
              )}
            </>
          ) : (
            <p className="muted">
              <Cable size={15} /> {t("top.noDevice")}
            </p>
          )}
        </section>

        <section className="card">
          <div className="card__head">
            <h3>{t("mirror.stream")}</h3>
            {firstFrame && (
              <span className="live-tag">
                <i /> LIVE
              </span>
            )}
          </div>
          <div className="stats">
            <Stat label={t("mirror.resolution")} value={formatResolution(mirror.width, mirror.height)} />
            <Stat label={t("mirror.fps")} value={firstFrame ? String(stats.fps) : "—"} />
            <Stat label={t("mirror.bitrate")} value={firstFrame ? `${(stats.kbps / 1000).toFixed(1)} Mbps` : "—"} />
            <Stat label={t("mirror.audio")} value={t(`mirror.audio.${mirror.audio}` as TKey)} />
          </div>
        </section>

        <section className="card card--actions">
          {isSelectedLive || busy ? (
            <div className="actions">
              <button className="button button--danger" onClick={stopMirror} disabled={busy}>
                {busy ? <LoaderCircle size={16} className="spin" /> : <Square size={15} />}
                {t("mirror.stop")}
              </button>
              <button className="button" onClick={restartMirror} disabled={busy}>
                <RotateCcw size={15} />
                {t("mirror.restart")}
              </button>
            </div>
          ) : (
            <button className="button button--primary" disabled={!selected?.capabilities.mirror} onClick={() => selected && startMirror(selected)}>
              <Play size={15} />
              {t("mirror.start")}
            </button>
          )}
          <div className="actions">
            <button
              className="button"
              disabled={!selected?.capabilities.screenshot || (selected.platform === "ios" && !isSelectedLive)}
              onClick={takeScreenshot}
            >
              <Camera size={15} />
              {t("mirror.screenshot")}
            </button>
            <button className={`button${recording.active ? " button--recording" : ""}`} disabled={!isSelectedLive || !selected?.capabilities.record || !firstFrame} onClick={toggleRecording}>
              <Circle size={13} fill="currentColor" />
              {recording.active ? <RecordTimer startedAt={recording.startedAtMs} /> : t("mirror.record")}
            </button>
          </div>
          <div className="actions">
            <button className="button" disabled={!firstFrame} onClick={onPhoneOnly} title={`${t("view.phoneOnly")} (F10)`}>
              <Smartphone size={15} />
              {t("view.phoneOnly")}
            </button>
            <button className="button" disabled={!firstFrame} onClick={() => onFullscreen(true)}>
              <Maximize size={15} />
              {t("mirror.fullscreen")}
            </button>
          </div>
        </section>

        {firstFrame && selected?.platform === "android" && <p className="hint">{t("mirror.hint")}</p>}
      </aside>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

export function RecordTimer({ startedAt }: { startedAt: number | null }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(timer);
  }, []);
  return <span className="mono">{formatDuration(startedAt ? (now - startedAt) / 1000 : 0)}</span>;
}
