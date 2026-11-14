import type { GameQuery } from "../bindings/GameQuery";

/** Where the window is: a page, the open view and the game whose page shows. */
export interface NavLocation {
  page: "home" | "statistics" | "library" | "settings";
  /** The open view (only on the library page). */
  viewId: string | null;
  /** The game whose details show (only on the library page). */
  gameId: string | null;
  /** Explore's filters, as Explore is rebuilt each time it opens. */
  explore?: GameQuery;
}

/** Back/forward steps; `index` points at the current location. */
export interface NavHistory {
  entries: readonly NavLocation[];
  index: number;
}

/** How many steps back are kept. */
export const MAX_HISTORY = 50;

export const EMPTY_HISTORY: NavHistory = { entries: [], index: -1 };

/** Two locations that are the same step: the same page, and in the library the same view. */
export function sameStep(a: NavLocation, b: NavLocation): boolean {
  if (a.page !== b.page) return false;
  return a.page !== "library" || a.viewId === b.viewId;
}

/** Updates the current location in place (e.g. another game selected, filters changed). */
export function replaceCurrent(history: NavHistory, location: NavLocation): NavHistory {
  if (history.index < 0) return { entries: [location], index: 0 };
  const entries = history.entries.slice();
  entries[history.index] = location;
  return { entries, index: history.index };
}

/** A new step: the steps ahead of the current one are dropped; the oldest go beyond the limit. */
export function visit(history: NavHistory, location: NavLocation, max = MAX_HISTORY): NavHistory {
  const entries = [...history.entries.slice(0, history.index + 1), location];
  const start = Math.max(0, entries.length - max);
  return { entries: entries.slice(start), index: entries.length - 1 - start };
}

export const canGoBack = (history: NavHistory) => history.index > 0;
export const canGoForward = (history: NavHistory) => history.index < history.entries.length - 1;

/** The history moved one step back or forward, with the location to show; null at either end. */
export function step(
  history: NavHistory,
  delta: -1 | 1,
): { history: NavHistory; location: NavLocation } | null {
  const index = history.index + delta;
  const location = history.entries[index];
  if (!location) return null;
  return { history: { entries: history.entries, index }, location };
}

/** Reads a location saved as JSON (the state kept across an update), or null if it isn't one. */
export function parseLocation(text: string | null | undefined): NavLocation | null {
  if (!text) return null;
  try {
    const value = JSON.parse(text) as Partial<NavLocation> | null;
    const pages = ["home", "statistics", "library", "settings"];
    if (!value || typeof value.page !== "string" || !pages.includes(value.page)) return null;
    return {
      page: value.page,
      viewId: typeof value.viewId === "string" ? value.viewId : null,
      gameId: typeof value.gameId === "string" ? value.gameId : null,
      ...(value.explore && typeof value.explore === "object" ? { explore: value.explore } : {}),
    };
  } catch {
    return null;
  }
}
