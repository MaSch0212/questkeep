export type ViewMode = "grid" | "list" | "details";

export const VIEW_MODE_KEY = "questkeep.viewMode";

const MODES = new Set(["grid", "list", "details"]);

/** A stored value as a view mode; anything unknown means the default, grid. */
export function parseViewMode(value: string | null): ViewMode {
  return value !== null && MODES.has(value) ? (value as ViewMode) : "grid";
}

/** The remembered library view; storage can be missing or throw (private mode), so default to grid. */
export function loadViewMode(): ViewMode {
  try {
    return parseViewMode(localStorage.getItem(VIEW_MODE_KEY));
  } catch {
    return "grid";
  }
}

export function saveViewMode(mode: ViewMode): void {
  try {
    localStorage.setItem(VIEW_MODE_KEY, mode);
  } catch {
    // Remembering the view is a convenience only.
  }
}
