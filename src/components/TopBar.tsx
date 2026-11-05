import { PanelLeft, PanelRight, Settings as SettingsIcon, Smartphone, Usb } from "lucide-react";
import { useStore } from "../store";
import { Logo } from "./Logo";

interface Props {
  sidebarOpen: boolean;
  panelOpen: boolean;
  onToggleSidebar(): void;
  onTogglePanel(): void;
  onPhoneOnly(): void;
}

export function TopBar({ sidebarOpen, panelOpen, onToggleSidebar, onTogglePanel, onPhoneOnly }: Props) {
  const { info, snapshot, mirror, firstFrame, page, setPage, t } = useStore();
  const live = snapshot.devices.filter((d) => d.state !== "disconnected");
  const anyConnected = live.some((d) => d.state === "connected");

  let status = { label: t("top.noDevice"), tone: "idle" };
  if (mirror.phase === "running") status = { label: t("top.mirroring"), tone: "live" };
  else if (anyConnected) status = { label: t("top.connected"), tone: "ok" };
  else if (live.length > 0) status = { label: t("top.needsAttention"), tone: "warn" };

  const usbActive = live.length > 0;

  return (
    <header className="topbar">
      <div className="topbar__brand">
        <Logo />
        <span className="topbar__name">
          ScreenLink <em>Studio</em>
        </span>
      </div>
      <div className="topbar__status">
        <span className={`pill pill--${status.tone}`}>
          <i className="pill__dot" />
          {status.label}
        </span>
        <span className={`pill pill--${usbActive ? "ok" : "idle"}`}>
          <Usb size={14} />
          {usbActive ? t("top.usbActive") : t("top.usbIdle")}
        </span>
      </div>
      <div className="topbar__end">
        {info && <span className="topbar__version">v{info.version}</span>}
        <button className="button topbar__phone" disabled={!firstFrame} onClick={onPhoneOnly} title={`${t("view.phoneOnly")} (F10)`}>
          <Smartphone size={15} />
          {t("view.phoneOnly")}
        </button>
        <button
          className={`icon-button${sidebarOpen ? "" : " icon-button--off"}`}
          onClick={onToggleSidebar}
          aria-pressed={sidebarOpen}
          title={sidebarOpen ? t("view.hideSidebar") : t("view.showSidebar")}
          aria-label={sidebarOpen ? t("view.hideSidebar") : t("view.showSidebar")}
        >
          <PanelLeft size={18} />
        </button>
        {page === "mirror" && (
          <button
            className={`icon-button${panelOpen ? "" : " icon-button--off"}`}
            onClick={onTogglePanel}
            aria-pressed={panelOpen}
            title={panelOpen ? t("view.hidePanel") : t("view.showPanel")}
            aria-label={panelOpen ? t("view.hidePanel") : t("view.showPanel")}
          >
            <PanelRight size={18} />
          </button>
        )}
        <button className="icon-button" onClick={() => setPage("settings")} title={t("top.settings")} aria-label={t("top.settings")}>
          <SettingsIcon size={18} />
        </button>
      </div>
    </header>
  );
}
