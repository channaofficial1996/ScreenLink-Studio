import { listen } from "@tauri-apps/api/event";
import { createContext, ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "./lib/api";
import { TKey, translate } from "./lib/i18n";
import { MirrorClient } from "./lib/mirrorClient";
import type { AppInfo, Device, DeviceSnapshot, MirrorStatus, RecordingStatus, Settings, StreamStats } from "./lib/types";

export type Page = "mirror" | "devices" | "recording" | "screenshots" | "settings" | "help" | "about";

export interface Toast {
  id: number;
  kind: "info" | "error";
  text: string;
}

const idleMirror: MirrorStatus = {
  phase: "idle",
  step: null,
  serial: null,
  deviceName: null,
  width: 0,
  height: 0,
  audio: "off",
  port: 0,
  token: "",
  endedReason: null,
};

const idleRecording: RecordingStatus = { active: false, startedAtMs: null, files: [], error: null };
const idleStats: StreamStats = { fps: 0, kbps: 0, decoder: "unknown" };

interface Store {
  page: Page;
  setPage(page: Page): void;
  info: AppInfo | null;
  snapshot: DeviceSnapshot;
  settings: Settings | null;
  saveSettings(patch: Partial<Settings>): Promise<void>;
  mirror: MirrorStatus;
  recording: RecordingStatus;
  stats: StreamStats;
  firstFrame: boolean;
  client: MirrorClient;
  selectedId: string | null;
  select(id: string): void;
  selected: Device | null;
  mirroredDevice: Device | null;
  startMirror(device: Device): Promise<void>;
  stopMirror(): Promise<void>;
  restartMirror(): Promise<void>;
  takeScreenshot(): Promise<void>;
  toggleRecording(): Promise<void>;
  mediaVersion: number;
  bumpMedia(): void;
  toasts: Toast[];
  toast(text: string, kind?: Toast["kind"]): void;
  t(key: TKey): string;
}

const StoreContext = createContext<Store | null>(null);

export function useStore(): Store {
  const store = useContext(StoreContext);
  if (!store) throw new Error("useStore outside provider");
  return store;
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [page, setPage] = useState<Page>("mirror");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [snapshot, setSnapshot] = useState<DeviceSnapshot>({ devices: [], adbError: null, appleService: false });
  const [settings, setSettings] = useState<Settings | null>(null);
  const [mirror, setMirror] = useState<MirrorStatus>(idleMirror);
  const [recording, setRecording] = useState<RecordingStatus>(idleRecording);
  const [stats, setStats] = useState<StreamStats>(idleStats);
  const [firstFrame, setFirstFrame] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [mediaVersion, setMediaVersion] = useState(0);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const toastId = useRef(0);

  const toast = useCallback((text: string, kind: Toast["kind"] = "info") => {
    const id = ++toastId.current;
    setToasts((list) => [...list.slice(-2), { id, kind, text }]);
    window.setTimeout(() => setToasts((list) => list.filter((x) => x.id !== id)), 5000);
  }, []);

  const clientRef = useRef<MirrorClient | null>(null);
  if (!clientRef.current) {
    clientRef.current = new MirrorClient({
      onStats: setStats,
      onFirstFrame: () => setFirstFrame(true),
      onVideoSize: () => {},
      onError: (m) => toast(m, "error"),
    });
  }
  const client = clientRef.current;

  const bumpMedia = useCallback(() => setMediaVersion((v) => v + 1), []);

  useEffect(() => {
    void api.appInfo().then(setInfo);
    void api.settings().then(setSettings);
    void api.devices().then(setSnapshot);
    void api.mirrorStatus().then(setMirror).catch(() => {});
    void api.recordingStatus().then(setRecording).catch(() => {});

    const subscriptions = [
      listen<DeviceSnapshot>("devices-changed", (e) => setSnapshot(e.payload)),
      listen<MirrorStatus>("mirror-status", (e) => setMirror(e.payload)),
      listen<Settings>("settings-changed", (e) => setSettings(e.payload)),
      listen<RecordingStatus>("recording-status", (e) => {
        setRecording(e.payload);
        if (e.payload.error) toast(e.payload.error, "error");
        if (!e.payload.active) bumpMedia();
      }),
    ];
    return () => subscriptions.forEach((s) => void s.then((unlisten) => unlisten()));
  }, [bumpMedia, toast]);

  // The stream socket follows the backend session, however that session was started.
  useEffect(() => {
    if (mirror.phase === "running" && mirror.port) {
      client.connect(mirror.port, mirror.token);
      return () => {
        client.disconnect();
        setFirstFrame(false);
        setStats(idleStats);
      };
    }
  }, [client, mirror.phase, mirror.port, mirror.token]);

  useEffect(() => {
    if (mirror.endedReason) toast(mirror.endedReason, "error");
  }, [mirror.endedReason, toast]);

  // Keep a sensible selection: the mirrored device, then whatever is usable.
  useEffect(() => {
    if (mirror.serial) {
      setSelectedId(mirror.serial);
      return;
    }
    setSelectedId((current) => {
      const stillThere = snapshot.devices.find((d) => d.id === current);
      if (stillThere && stillThere.state !== "disconnected") return current;
      const usable = snapshot.devices.find((d) => d.capabilities.mirror) ?? snapshot.devices.find((d) => d.state !== "disconnected");
      return usable?.id ?? current ?? null;
    });
  }, [snapshot, mirror.serial]);

  const selected = useMemo(() => snapshot.devices.find((d) => d.id === selectedId) ?? null, [snapshot, selectedId]);
  const mirroredDevice = useMemo(() => snapshot.devices.find((d) => d.id === mirror.serial) ?? null, [snapshot, mirror.serial]);

  const saveSettings = useCallback(
    async (patch: Partial<Settings>) => {
      if (!settings) return;
      try {
        setSettings(await api.updateSettings({ ...settings, ...patch }));
      } catch (e) {
        toast(errorMessage(e), "error");
      }
    },
    [settings, toast],
  );

  const startMirror = useCallback(
    async (device: Device) => {
      try {
        setSelectedId(device.id);
        client.resumeAudio();
        if (device.platform === "ios") await api.iosMirrorStart(device.id);
        else await api.mirrorStart(device.id);
      } catch {
        // The backend also reports this through mirror-status, which raises the toast.
      }
    },
    [client],
  );

  const stopMirror = useCallback(async () => {
    try {
      if (mirroredDevice?.platform === "ios") await api.iosMirrorStop();
      else await api.mirrorStop();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }, [mirroredDevice, toast]);

  const restartMirror = useCallback(async () => {
    const device = mirroredDevice;
    if (!device) return;
    await stopMirror();
    await startMirror(device);
  }, [mirroredDevice, startMirror, stopMirror]);

  const t = useCallback((key: TKey) => translate(settings?.language ?? "en", key), [settings?.language]);

  const takeScreenshot = useCallback(async () => {
    try {
      const device = selected;
      if (device?.platform === "ios") {
        const png = await client.capturePng();
        if (png) await api.savePreviewPng(Array.from(png));
      } else if (device) {
        await api.screenshot(device.id);
      }
      toast(t("shots.saved"));
      bumpMedia();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }, [selected, client, toast, t, bumpMedia]);

  const toggleRecording = useCallback(async () => {
    try {
      if (recording.active) await api.recordingStop();
      else await api.recordingStart();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }, [recording.active, toast]);

  const value: Store = {
    page,
    setPage,
    info,
    snapshot,
    settings,
    saveSettings,
    mirror,
    recording,
    stats,
    firstFrame,
    client,
    selectedId,
    select: setSelectedId,
    selected,
    mirroredDevice,
    startMirror,
    stopMirror,
    restartMirror,
    takeScreenshot,
    toggleRecording,
    mediaVersion,
    bumpMedia,
    toasts,
    toast,
    t,
  };
  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}
