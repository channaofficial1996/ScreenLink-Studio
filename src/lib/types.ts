export type Platform = "android" | "ios";
export type DeviceState = "connected" | "unauthorized" | "offline" | "disconnected" | "unsupported";

export interface Capabilities {
  mirror: boolean;
  control: boolean;
  audio: boolean;
  record: boolean;
  screenshot: boolean;
}

export interface Compatibility {
  level: "info" | "warning";
  code: string;
  message: string;
}

export interface Device {
  id: string;
  platform: Platform;
  name: string;
  model: string | null;
  manufacturer: string | null;
  osVersion: string | null;
  state: DeviceState;
  capabilities: Capabilities;
  compatibility: Compatibility | null;
}

export interface DeviceSnapshot {
  devices: Device[];
  adbError: string | null;
  appleService: boolean;
}

export type MirrorPhase = "idle" | "starting" | "running" | "stopping";
export type AudioState = "off" | "pending" | "active" | "unavailable";

export interface MirrorStatus {
  phase: MirrorPhase;
  step: "pushing" | "launching" | "connecting" | null;
  serial: string | null;
  deviceName: string | null;
  width: number;
  height: number;
  audio: AudioState;
  port: number;
  token: string;
  endedReason: string | null;
}

export interface RecordingStatus {
  active: boolean;
  startedAtMs: number | null;
  files: string[];
  error: string | null;
}

export interface Settings {
  resolution: number;
  fps: number;
  bitrateMbps: number;
  audioForwarding: boolean;
  alwaysOnTop: boolean;
  launchAtLogin: boolean;
  startMinimized: boolean;
  autoConnect: boolean;
  saveDirectory: string;
  language: string;
  theme: string;
  stayAwake: boolean;
  showTouches: boolean;
  autoCheckUpdates: boolean;
}

export interface AppInfo {
  version: string;
  platform: string;
  arch: string;
  scrcpyVersion: string;
  adbVersion: string | null;
  adbPath: string | null;
  iosCanMirror: boolean;
  logDir: string | null;
}

export interface MediaItem {
  name: string;
  path: string;
  kind: "screenshot" | "recording";
  bytes: number;
  modifiedMs: number;
}

export interface StreamStats {
  fps: number;
  kbps: number;
  decoder: "hardware" | "software" | "unknown";
}

export type InputEvent =
  | { t: "touch"; action: number; x: number; y: number; w: number; h: number; pressure: number; button: number; buttons: number }
  | { t: "scroll"; x: number; y: number; w: number; h: number; hscroll: number; vscroll: number; buttons: number }
  | { t: "key"; action: number; code: number; repeat: number; meta: number }
  | { t: "text"; text: string }
  | { t: "paste"; text: string; sequence: number }
  | { t: "backOrScreenOn"; action: number }
  | { t: "expandNotifications" }
  | { t: "expandSettings" }
  | { t: "collapsePanels" }
  | { t: "rotateDevice" }
  | { t: "displayPower"; on: boolean }
  | { t: "resetVideo" };
