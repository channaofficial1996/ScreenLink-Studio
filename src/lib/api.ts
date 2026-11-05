import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, DeviceSnapshot, MediaItem, MirrorStatus, RecordingStatus, Settings } from "./types";

export interface ApiError {
  code: string;
  message: string;
}

export function errorMessage(e: unknown): string {
  if (typeof e === "object" && e !== null && "message" in e) return String((e as ApiError).message);
  return String(e);
}

export const api = {
  appInfo: () => invoke<AppInfo>("get_app_info"),
  devices: () => invoke<DeviceSnapshot>("get_devices"),
  refreshDevices: () => invoke<void>("refresh_devices"),
  settings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<Settings>("update_settings", { settings }),
  mirrorStart: (serial: string) => invoke<MirrorStatus>("mirror_start", { serial }),
  mirrorStop: () => invoke<void>("mirror_stop"),
  mirrorStatus: () => invoke<MirrorStatus>("mirror_status"),
  iosMirrorStart: (udid: string) => invoke<MirrorStatus>("ios_mirror_start", { udid }),
  iosMirrorStop: () => invoke<void>("ios_mirror_stop"),
  screenshot: (serial: string) => invoke<string>("take_screenshot", { serial }),
  savePreviewPng: (png: number[]) => invoke<string>("save_preview_png", { png }),
  recordingStart: () => invoke<RecordingStatus>("recording_start"),
  recordingStop: () => invoke<RecordingStatus>("recording_stop"),
  recordingStatus: () => invoke<RecordingStatus>("recording_status"),
  listMedia: () => invoke<MediaItem[]>("list_media"),
  deleteMedia: (name: string) => invoke<void>("delete_media", { name }),
  revealMedia: (name: string) => invoke<void>("reveal_media", { name }),
  openMedia: (name: string) => invoke<void>("open_media", { name }),
  openSaveDirectory: () => invoke<void>("open_save_directory"),
};
