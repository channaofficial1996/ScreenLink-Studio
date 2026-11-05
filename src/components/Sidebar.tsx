import { Camera, CircleHelp, Info, MonitorSmartphone, Settings, Smartphone, Video } from "lucide-react";
import { TKey } from "../lib/i18n";
import { Page, useStore } from "../store";

const primary: { page: Page; label: TKey; icon: typeof Video }[] = [
  { page: "mirror", label: "nav.mirror", icon: MonitorSmartphone },
  { page: "devices", label: "nav.devices", icon: Smartphone },
  { page: "recording", label: "nav.recording", icon: Video },
  { page: "screenshots", label: "nav.screenshots", icon: Camera },
];

const secondary: { page: Page; label: TKey; icon: typeof Video }[] = [
  { page: "settings", label: "nav.settings", icon: Settings },
  { page: "help", label: "nav.help", icon: CircleHelp },
  { page: "about", label: "nav.about", icon: Info },
];

export function Sidebar() {
  const { page, setPage, t, snapshot, recording } = useStore();
  const deviceCount = snapshot.devices.filter((d) => d.state !== "disconnected").length;

  const item = ({ page: target, label, icon: Icon }: (typeof primary)[number]) => (
    <button
      key={target}
      className={`nav-item${page === target ? " nav-item--active" : ""}`}
      aria-current={page === target ? "page" : undefined}
      onClick={() => setPage(target)}
    >
      <Icon size={18} />
      <span>{t(label)}</span>
      {target === "devices" && deviceCount > 0 && <b className="nav-item__badge">{deviceCount}</b>}
      {target === "recording" && recording.active && <i className="nav-item__rec" />}
    </button>
  );

  return (
    <nav className="sidebar">
      <div className="sidebar__group">{primary.map(item)}</div>
      <div className="sidebar__group">{secondary.map(item)}</div>
    </nav>
  );
}
