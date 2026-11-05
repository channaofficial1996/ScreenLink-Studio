import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, Update } from "@tauri-apps/plugin-updater";

export type { Update };

export function checkForUpdate(): Promise<Update | null> {
  return check();
}

/**
 * The backend must release adb first: its daemon runs from the install folder, and the installer
 * cannot replace a file that is in use.
 */
export async function installUpdate(update: Update, onProgress: (fraction: number) => void) {
  let total = 0;
  let received = 0;
  await update.download((event) => {
    if (event.event === "Started") total = event.data.contentLength ?? 0;
    if (event.event === "Progress") {
      received += event.data.chunkLength;
      if (total > 0) onProgress(received / total);
    }
  });
  await invoke("prepare_update");
  await update.install();
  await relaunch();
}
