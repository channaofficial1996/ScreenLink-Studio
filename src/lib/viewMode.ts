import { invoke } from "@tauri-apps/api/core";

export type Side = "left" | "right";

export const phoneMode = {
  enter: (width: number, height: number, side: Side) => invoke<void>("phone_mode_enter", { width, height, side }),
  fit: (width: number, height: number, side?: Side) => invoke<void>("phone_mode_fit", { width, height, side }),
  pin: (pinned: boolean) => invoke<void>("phone_mode_pin", { pinned }),
  exit: () => invoke<void>("phone_mode_exit"),
};

/** A per-viewer layout preference. Storage can be unavailable, and the app works without it. */
export function loadFlag(key: string, fallback: boolean): boolean {
  try {
    const value = localStorage.getItem(key);
    return value === null ? fallback : value === "1";
  } catch {
    return fallback;
  }
}

export function saveFlag(key: string, value: boolean) {
  try {
    localStorage.setItem(key, value ? "1" : "0");
  } catch {
    // Not saved; the layout simply resets next launch.
  }
}
