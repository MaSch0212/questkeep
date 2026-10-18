export type NavKey =
  | "ArrowLeft"
  | "ArrowRight"
  | "ArrowUp"
  | "ArrowDown"
  | "Home"
  | "End"
  | "PageUp"
  | "PageDown";

const NAV_KEYS: ReadonlySet<string> = new Set<NavKey>([
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Home",
  "End",
  "PageUp",
  "PageDown",
]);

export function isNavKey(key: string): key is NavKey {
  return NAV_KEYS.has(key);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * Index of the tile that should be selected after `key`. Left/right step through the flat list
 * (so they wrap across rows); up/down move by a row; Home/End jump to the first/last game.
 * Returns -1 only when there are no tiles. A `current` of -1 (nothing selected) starts at the
 * first tile.
 */
export function nextIndex(
  current: number,
  key: NavKey,
  columns: number,
  total: number,
  pageRows: number,
): number {
  if (total <= 0) return -1;
  const last = total - 1;
  if (current < 0 || current > last) return key === "End" ? last : 0;
  const cols = Math.max(1, Math.floor(columns));
  const page = cols * Math.max(1, Math.floor(pageRows));
  switch (key) {
    case "ArrowLeft":
      return clamp(current - 1, 0, last);
    case "ArrowRight":
      return clamp(current + 1, 0, last);
    case "ArrowUp":
      return current - cols >= 0 ? current - cols : current;
    case "ArrowDown": {
      if (current + cols <= last) return current + cols;
      // Short last row: step to the final tile unless we are already on the last row.
      const onLastRow = Math.floor(current / cols) === Math.floor(last / cols);
      return onLastRow ? current : last;
    }
    case "Home":
      return 0;
    case "End":
      return last;
    case "PageUp":
      return clamp(current - page, 0, last);
    case "PageDown":
      return clamp(current + page, 0, last);
  }
}
