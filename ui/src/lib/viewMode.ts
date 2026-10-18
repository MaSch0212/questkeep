export type ViewMode = "grid" | "list";

export const VIEW_MODE_KEY = "questkeep.viewMode";

/** The remembered library view; storage can be missing or throw (private mode), so default to grid. */
export function loadViewMode(): ViewMode {
  try {
    return localStorage.getItem(VIEW_MODE_KEY) === "list" ? "list" : "grid";
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
