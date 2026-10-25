import type { CoreEvent } from "../bindings/CoreEvent";
import type { ScanSummary } from "../bindings/ScanSummary";

/**
 * Library sync progress, built from core scan events. Several stores are scanned one after
 * another (a full sync) or one at a time (Settings, sign-in, app start); all of that is one "run"
 * that ends shortly after the last store finished, so back-to-back scans join the same run and
 * produce a single result message.
 */

export type SyncEvent = Extract<
  CoreEvent,
  { type: "scansQueued" | "scanStarted" | "libraryScanned" | "scanFailed" }
>;

export type SyncEntryStatus = "waiting" | "running" | "done" | "failed";

export interface SyncEntry {
  provider: string;
  status: SyncEntryStatus;
  /** Set when `status` is `done`. */
  summary?: ScanSummary;
  /** Set when `status` is `failed`. */
  message?: string;
}

export interface SyncState {
  /** Stores of the current run (while `active`) or of the last one, in scan order. */
  entries: SyncEntry[];
  /** A run is in progress, including the short grace period after the last store finished. */
  active: boolean;
  /** When the last run ended (ms since epoch); null before the first run ends. */
  finishedAt: number | null;
}

export const IDLE_SYNC: SyncState = { entries: [], active: false, finishedAt: null };

/** How long after the last store finished a run stays open for a follow-up scan to join. */
export const SYNC_END_DEBOUNCE_MS = 1500;

export function isSyncEvent(event: CoreEvent): event is SyncEvent {
  return (
    event.type === "scansQueued" ||
    event.type === "scanStarted" ||
    event.type === "libraryScanned" ||
    event.type === "scanFailed"
  );
}

function setEntry(entries: SyncEntry[], next: SyncEntry): SyncEntry[] {
  const index = entries.findIndex((e) => e.provider === next.provider);
  if (index < 0) return [...entries, next];
  return entries.map((e, i) => (i === index ? next : e));
}

/** The state after one scan event. Pure; a run starts with the first event while not `active`. */
export function applySyncEvent(state: SyncState, event: SyncEvent): SyncState {
  let entries = state.active ? state.entries : [];
  switch (event.type) {
    case "scansQueued":
      for (const provider of event.providers) {
        entries = setEntry(entries, { provider, status: "waiting" });
      }
      break;
    case "scanStarted":
      entries = setEntry(entries, { provider: event.provider, status: "running" });
      break;
    case "libraryScanned":
      entries = setEntry(entries, {
        provider: event.provider,
        status: "done",
        summary: event.summary,
      });
      break;
    case "scanFailed":
      entries = setEntry(entries, {
        provider: event.provider,
        status: "failed",
        message: event.message,
      });
      break;
  }
  return { entries, active: true, finishedAt: state.active ? state.finishedAt : null };
}

/** Some store is waiting for its turn or being scanned. */
export function isSyncBusy(state: SyncState): boolean {
  return state.entries.some((e) => e.status === "waiting" || e.status === "running");
}

/** This store is waiting for its turn or being scanned. */
export function isProviderSyncing(state: SyncState, provider: string): boolean {
  return state.entries.some(
    (e) => e.provider === provider && (e.status === "waiting" || e.status === "running"),
  );
}

/** "Syncing 2 of 7…" while busy, otherwise a neutral name for the indicator. */
export function syncLabel(state: SyncState): string {
  if (!isSyncBusy(state)) return "Sync status";
  const total = state.entries.length;
  const finished = state.entries.filter((e) => e.status === "done" || e.status === "failed").length;
  return `Syncing ${Math.min(total, finished + 1)} of ${total}…`;
}

/** The one message shown when a run ends. */
export function syncResultText(state: SyncState): string {
  let added = 0;
  let updated = 0;
  let failed = 0;
  for (const entry of state.entries) {
    if (entry.status === "failed") failed++;
    added += entry.summary?.added ?? 0;
    updated += entry.summary?.updated ?? 0;
  }
  const changes = added + updated === 0 ? "no changes" : `${added} added, ${updated} updated`;
  return `Sync finished: ${changes}${failed > 0 ? `, ${failed} failed` : ""}`;
}

/** Per-store status text for the popover. */
export function syncEntryText(entry: SyncEntry): string {
  switch (entry.status) {
    case "waiting":
      return "Waiting";
    case "running":
      return "Scanning…";
    case "done": {
      const added = entry.summary?.added ?? 0;
      const updated = entry.summary?.updated ?? 0;
      return added + updated === 0 ? "No changes" : `${added} added, ${updated} updated`;
    }
    case "failed":
      return entry.message ?? "Failed";
  }
}

export interface SyncTrackerOptions {
  /** Called with every new state. */
  onChange: (state: SyncState) => void;
  /** Called once when a run ends, with its final state. */
  onFinished: (state: SyncState) => void;
  debounceMs?: number;
  now?: () => number;
}

export interface SyncTracker {
  state: () => SyncState;
  /** Feeds a core event; returns false (and does nothing) for events that are not scan events. */
  handle: (event: CoreEvent) => boolean;
  /** Drops the state and any pending end timer (tests, sign-out). */
  reset: () => void;
}

export function createSyncTracker(options: SyncTrackerOptions): SyncTracker {
  const debounceMs = options.debounceMs ?? SYNC_END_DEBOUNCE_MS;
  const now = options.now ?? Date.now;
  let state = IDLE_SYNC;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function set(next: SyncState) {
    state = next;
    options.onChange(state);
  }

  function finish() {
    timer = undefined;
    set({ ...state, active: false, finishedAt: now() });
    options.onFinished(state);
  }

  return {
    state: () => state,
    handle(event) {
      if (!isSyncEvent(event)) return false;
      clearTimeout(timer);
      timer = undefined;
      set(applySyncEvent(state, event));
      if (!isSyncBusy(state)) timer = setTimeout(finish, debounceMs);
      return true;
    },
    reset() {
      clearTimeout(timer);
      timer = undefined;
      set(IDLE_SYNC);
    },
  };
}
