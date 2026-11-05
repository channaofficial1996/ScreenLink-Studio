import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { Toasts } from "./components/Toasts";
import { TopBar } from "./components/TopBar";
import { UpdateBanner } from "./components/UpdateBanner";
import { loadFlag, phoneMode, saveFlag, Side } from "./lib/viewMode";
import { AboutPage } from "./pages/AboutPage";
import { DevicesPage } from "./pages/DevicesPage";
import { HelpPage } from "./pages/HelpPage";
import { MirrorPage } from "./pages/MirrorPage";
import { RecordingPage } from "./pages/RecordingPage";
import { ScreenshotsPage } from "./pages/ScreenshotsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { useStore } from "./store";

export default function App() {
  const { page, setPage, settings, mirror, firstFrame, client, toast } = useStore();
  const [fullscreen, setFullscreen] = useState(false);
  const [phoneOnly, setPhoneOnly] = useState(false);
  const [side, setSide] = useState<Side>("right");
  const [sidebarOpen, setSidebarOpen] = useState(() => loadFlag("layout.sidebar", true));
  const [panelOpen, setPanelOpen] = useState(() => loadFlag("layout.panel", true));
  const immersive = fullscreen || phoneOnly;

  const setWindowFullscreen = useCallback((on: boolean) => {
    setFullscreen(on);
    void getCurrentWindow().setFullscreen(on);
  }, []);

  const toggleSidebar = useCallback(() => {
    const next = !sidebarOpen;
    setSidebarOpen(next);
    saveFlag("layout.sidebar", next);
  }, [sidebarOpen]);

  const togglePanel = useCallback(() => {
    const next = !panelOpen;
    setPanelOpen(next);
    saveFlag("layout.panel", next);
  }, [panelOpen]);

  const enterPhoneOnly = useCallback(() => {
    if (!firstFrame) return;
    setPage("mirror");
    setPhoneOnly(true);
    phoneMode.enter(client.videoWidth, client.videoHeight, side).catch((e) => {
      setPhoneOnly(false);
      toast(String(e?.message ?? e), "error");
    });
  }, [firstFrame, client, side, setPage, toast]);

  const exitPhoneOnly = useCallback(() => {
    setPhoneOnly(false);
    void phoneMode.exit();
  }, []);

  const dock = useCallback(
    (next: Side) => {
      setSide(next);
      void phoneMode.fit(client.videoWidth, client.videoHeight, next);
    },
    [client],
  );

  // F10 works from anywhere in the app, including while the phone screen has keyboard focus.
  const phoneOnlyRef = useRef({ phoneOnly, enterPhoneOnly, exitPhoneOnly });
  phoneOnlyRef.current = { phoneOnly, enterPhoneOnly, exitPhoneOnly };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.code !== "F10") return;
      e.preventDefault();
      const current = phoneOnlyRef.current;
      if (current.phoneOnly) current.exitPhoneOnly();
      else current.enterPhoneOnly();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // These views have no way back to the controls, so they must not outlive the session.
  useEffect(() => {
    if (mirror.phase === "running") return;
    if (phoneOnly) exitPhoneOnly();
    if (fullscreen) setWindowFullscreen(false);
  }, [mirror.phase, phoneOnly, fullscreen, exitPhoneOnly, setWindowFullscreen]);

  // Follow rotation so the panel keeps the phone's shape.
  useEffect(() => {
    if (phoneOnly && mirror.width && mirror.height) void phoneMode.fit(mirror.width, mirror.height);
  }, [phoneOnly, mirror.width, mirror.height]);

  useEffect(() => {
    const theme = settings?.theme ?? "dark";
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      const resolved = theme === "system" ? (media.matches ? "light" : "dark") : theme;
      document.documentElement.dataset.theme = resolved;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [settings?.theme]);

  useEffect(() => {
    if (settings) document.documentElement.lang = settings.language;
  }, [settings?.language]);

  return (
    <div className={`shell${immersive ? " shell--fullscreen" : ""}`}>
      {!immersive && (
        <TopBar
          sidebarOpen={sidebarOpen}
          panelOpen={panelOpen}
          onToggleSidebar={toggleSidebar}
          onTogglePanel={togglePanel}
          onPhoneOnly={enterPhoneOnly}
        />
      )}
      {!immersive && <UpdateBanner />}
      <div className="shell__body">
        {!immersive && sidebarOpen && <Sidebar />}
        <main className="shell__main">
          {/* Stays mounted so navigating away never interrupts the live video. */}
          <div className="page-slot" hidden={page !== "mirror"}>
            <MirrorPage
              fullscreen={fullscreen}
              phoneOnly={phoneOnly}
              panelOpen={panelOpen}
              side={side}
              onFullscreen={setWindowFullscreen}
              onPhoneOnly={enterPhoneOnly}
              onExitPhoneOnly={exitPhoneOnly}
              onDock={dock}
            />
          </div>
          {page === "devices" && <DevicesPage />}
          {page === "recording" && <RecordingPage />}
          {page === "screenshots" && <ScreenshotsPage />}
          {page === "settings" && <SettingsPage />}
          {page === "help" && <HelpPage />}
          {page === "about" && <AboutPage />}
        </main>
      </div>
      <Toasts />
    </div>
  );
}
