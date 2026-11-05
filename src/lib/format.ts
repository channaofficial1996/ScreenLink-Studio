export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = -1;
  do {
    value /= 1024;
    unit++;
  } while (value >= 1024 && unit < units.length - 1);
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

export function formatDuration(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const pad = (n: number) => String(n).padStart(2, "0");
  const hours = Math.floor(s / 3600);
  const mm = pad(Math.floor((s % 3600) / 60));
  return `${hours > 0 ? `${pad(hours)}:` : ""}${mm}:${pad(s % 60)}`;
}

export function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Compare numbers to the longest edge so a phone reads as "1080 × 2400" not an abstract label. */
export function formatResolution(width: number, height: number): string {
  return width && height ? `${width} × ${height}` : "—";
}
