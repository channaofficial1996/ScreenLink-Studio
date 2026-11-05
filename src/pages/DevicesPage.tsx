import { Apple, Check, Minus, Play, RefreshCw, Smartphone } from "lucide-react";
import { StatusBadge } from "../components/StatusBadge";
import { api } from "../lib/api";
import { compatText } from "../lib/compat";
import type { Device } from "../lib/types";
import { useStore } from "../store";

export function DevicesPage() {
  const { snapshot, info, t } = useStore();
  return (
    <div className="page">
      <header className="page__head">
        <div>
          <h1>{t("devices.title")}</h1>
          <p>{t("devices.subtitle")}</p>
        </div>
        <button className="button" onClick={() => void api.refreshDevices()}>
          <RefreshCw size={15} />
          {t("devices.refresh")}
        </button>
      </header>

      {snapshot.adbError && (
        <p className="notice notice--warning">
          <strong>{t("devices.adbError")}.</strong> {snapshot.adbError}
        </p>
      )}

      {snapshot.devices.length === 0 ? (
        <div className="empty">{t("devices.empty")}</div>
      ) : (
        <div className="device-grid">
          {snapshot.devices.map((d) => (
            <DeviceCard key={d.id} device={d} />
          ))}
        </div>
      )}

      {info && (
        <p className="footnote">
          {t("devices.appleService")}: {snapshot.appleService ? t("devices.appleService.ok") : t("devices.appleService.missing")}
        </p>
      )}
    </div>
  );
}

function DeviceCard({ device }: { device: Device }) {
  const { t, mirror, startMirror, setPage, settings } = useStore();
  const live = mirror.phase === "running" && mirror.serial === device.id;
  const Icon = device.platform === "ios" ? Apple : Smartphone;
  const caps: [string, boolean][] = [
    [t("devices.capMirror"), device.capabilities.mirror],
    [t("devices.capControl"), device.capabilities.control],
    [t("devices.capAudio"), device.capabilities.audio],
    [t("devices.capRecord"), device.capabilities.record],
  ];

  return (
    <article className={`card device-card${device.state === "disconnected" ? " device-card--gone" : ""}`}>
      <div className="card__head">
        <span className="device-card__icon">
          <Icon size={20} />
        </span>
        <StatusBadge state={device.state} />
      </div>
      <p className="device-name">{device.name}</p>
      <p className="muted mono">{device.id}</p>
      <p className="muted">{[device.manufacturer, device.model, device.osVersion].filter(Boolean).join(" · ")}</p>
      {device.compatibility && (
        <p className={`notice notice--${device.compatibility.level}`}>{compatText(device.compatibility, settings?.language ?? "en")}</p>
      )}
      {device.state !== "disconnected" && (
        <ul className="caps">
          {caps.map(([label, on]) => (
            <li key={label} className={on ? "caps__on" : ""}>
              {on ? <Check size={13} /> : <Minus size={13} />}
              {label}
            </li>
          ))}
        </ul>
      )}
      {device.capabilities.mirror && (
        <button
          className={`button${live ? "" : " button--primary"}`}
          onClick={() => {
            if (!live) void startMirror(device);
            setPage("mirror");
          }}
        >
          <Play size={15} />
          {live ? t("devices.current") : t("devices.mirror")}
        </button>
      )}
    </article>
  );
}
