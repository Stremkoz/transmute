const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];

/** "1.4 MB": decimal units, as Windows Explorer's sizes read. */
export function formatBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < BYTE_UNITS.length - 1) {
    value /= 1000;
    unit++;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${BYTE_UNITS[unit]}`;
}

/** "under 1 s", "45 s", "3 min 12 s", "1 h 05 min". */
export function formatDuration(ms: number): string {
  if (ms < 1000) return "under 1 s";
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ${String(seconds % 60).padStart(2, "0")} s`;
  return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, "0")} min`;
}

export function formatCount(count: number): string {
  return count.toLocaleString();
}

/** "1 image" / "3 images". */
export function plural(count: number, one: string, many = `${one}s`): string {
  return `${formatCount(count)} ${count === 1 ? one : many}`;
}
