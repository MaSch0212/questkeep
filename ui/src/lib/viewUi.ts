import type { GroupBy } from "../bindings/GroupBy";
import type { ViewUiState } from "../bindings/ViewUiState";

/** A view's UI state before anything was remembered. */
export const DEFAULT_VIEW_UI: ViewUiState = {
  panelFloating: false,
  panelWidth: null,
  listWidth: null,
  selected: null,
  collapsed: {},
};

/**
 * A width between `min` and half of `available` (but never below `min`), rounded to whole
 * pixels. Used for the details panel and the details layout's list.
 */
export function clampWidth(width: number, min: number, available: number): number {
  const max = Math.max(min, Math.floor(available / 2));
  return Math.round(Math.min(max, Math.max(min, width)));
}

/** The collapsed group keys of a view for one grouping field. */
export function collapsedKeys(state: ViewUiState, group: GroupBy): readonly string[] {
  return state.collapsed[group] ?? [];
}

/** Collapses or expands one group for one grouping field; other fields keep their state. */
export function toggleCollapsed(state: ViewUiState, group: GroupBy, key: string): ViewUiState {
  const keys = collapsedKeys(state, group);
  const next = keys.includes(key) ? keys.filter((k) => k !== key) : [...keys, key];
  return { ...state, collapsed: { ...state.collapsed, [group]: next } };
}

/**
 * Calls `save` for each changed key once no further change came for `delay` ms. `flush` saves
 * everything pending right away (e.g. when the window closes).
 */
export function createDebouncedSaver<T>(save: (key: string, value: T) => void, delay: number) {
  const pending = new Map<string, { value: T; timer: ReturnType<typeof setTimeout> }>();
  function write(key: string) {
    const entry = pending.get(key);
    if (!entry) return;
    clearTimeout(entry.timer);
    pending.delete(key);
    save(key, entry.value);
  }
  return {
    schedule(key: string, value: T) {
      clearTimeout(pending.get(key)?.timer);
      pending.set(key, { value, timer: setTimeout(() => write(key), delay) });
    },
    flush() {
      // Deleting the current entry while iterating a Map is safe.
      for (const key of pending.keys()) write(key);
    },
    /** Drops everything pending without saving (tests, reset). */
    cancel() {
      for (const entry of pending.values()) clearTimeout(entry.timer);
      pending.clear();
    },
  };
}
