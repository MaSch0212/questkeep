import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { batch, createMemo, createRoot, createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import type { AppInfo } from "../bindings/AppInfo";
import type { CoreEvent } from "../bindings/CoreEvent";
import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import type { SortField } from "../bindings/SortField";
import * as api from "./api";
import { errorMessage } from "./errors";
import {
  defaultDescending,
  defaultQuery,
  mergeSources,
  toggleSource,
  withLibraryView,
  type LibraryView,
} from "./query";
import {
  createSyncTracker,
  IDLE_SYNC,
  isSyncBusy,
  syncResultText,
  type SyncState,
} from "./syncStatus";
import { loadViewMode, saveViewMode, type ViewMode } from "./viewMode";

export type Page = "library" | "settings";

export interface Toast {
  id: number;
  kind: "info" | "error";
  message: string;
}

const REFRESH_DEBOUNCE_MS = 60;

function createAppStore() {
  const [query, setQuery] = createSignal<GameQuery>(defaultQuery());
  const [games, setGames] = createSignal<Game[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [detailsOpen, setDetailsOpen] = createSignal(false);
  const [running, setRunning] = createSignal<ReadonlySet<string>>(new Set());
  const [providers, setProviders] = createSignal<ProviderInfo[]>([]);
  const [appInfo, setAppInfo] = createSignal<AppInfo | null>(null);
  const [seenSources, setSeenSources] = createSignal<ReadonlySet<string>>(new Set());
  const [toasts, setToasts] = createSignal<Toast[]>([]);
  const [coverVersions, setCoverVersions] = createStore<Record<string, number>>({});
  const [page, setPage] = createSignal<Page>("library");
  const [viewMode, setViewModeSignal] = createSignal<ViewMode>(loadViewMode());
  const [syncState, setSyncState] = createSignal<SyncState>(IDLE_SYNC);

  const indexById = createMemo(() => {
    const map = new Map<string, number>();
    games().forEach((game, index) => map.set(game.id, index));
    return map;
  });
  const selectedIndex = createMemo(() => {
    const id = selectedId();
    return id === null ? -1 : (indexById().get(id) ?? -1);
  });
  // Keeps the last known game while it is temporarily filtered out of the list.
  const selectedGame = createMemo<Game | undefined>((previous) => {
    const id = selectedId();
    if (id === null) return undefined;
    const index = indexById().get(id);
    if (index !== undefined) return games()[index];
    return previous?.id === id ? previous : undefined;
  });
  const sources = createMemo(() => mergeSources(providers(), seenSources()));

  // Scroll offset of the library, kept while another page (settings) replaces it.
  const libraryScroll: Record<ViewMode, number> = { grid: 0, list: 0, details: 0 };
  let requestSeq = 0;
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  let toastSeq = 0;

  function dismissToast(id: number) {
    setToasts((list) => list.filter((t) => t.id !== id));
  }

  function pushToast(kind: Toast["kind"], message: string) {
    const id = ++toastSeq;
    setToasts((list) => [...list.slice(-4), { id, kind, message }]);
    setTimeout(() => dismissToast(id), kind === "error" ? 8000 : 4500);
  }

  // One status for every scan, however it started; one toast when the run ends.
  const syncTracker = createSyncTracker({
    onChange: setSyncState,
    onFinished: (state) => pushToast("info", syncResultText(state)),
  });

  async function refresh(): Promise<void> {
    const seq = ++requestSeq;
    setLoading(true);
    try {
      const list = await api.listGames(query());
      if (seq !== requestSeq) return; // a newer request superseded this one
      batch(() => {
        setGames(list);
        setSeenSources((prev) => {
          const next = new Set(prev);
          for (const game of list) next.add(game.source);
          return next.size === prev.size ? prev : next;
        });
        setLoading(false);
      });
    } catch (error) {
      if (seq !== requestSeq) return;
      setLoading(false);
      pushToast("error", errorMessage(error));
    }
  }

  function scheduleRefresh(delay = REFRESH_DEBOUNCE_MS) {
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => void refresh(), delay);
  }

  function updateQuery(patch: Partial<GameQuery> | ((q: GameQuery) => GameQuery)) {
    setQuery((q) => (typeof patch === "function" ? patch(q) : { ...q, ...patch }));
    scheduleRefresh(0);
  }

  /** Replaces one game in the list with a fresher copy (e.g. a command result). */
  function upsertGame(game: Game) {
    setGames((list) => list.map((g) => (g.id === game.id ? game : g)));
  }

  function bumpCover(ids: readonly string[]) {
    batch(() => {
      for (const id of ids) setCoverVersions(id, (v) => (v ?? 0) + 1);
    });
  }

  function imageUrl(game: Game, path: string | null): string | null {
    if (path === null) return null;
    return `${convertFileSrc(path)}?v=${coverVersions[game.id] ?? 0}`;
  }

  function coverUrl(game: Game): string | null {
    return imageUrl(game, game.coverPath);
  }

  function backgroundUrl(game: Game): string | null {
    return imageUrl(game, game.backgroundPath);
  }

  function setViewMode(mode: ViewMode) {
    setViewModeSignal(mode);
    saveViewMode(mode);
  }

  function selectGame(id: string | null) {
    setSelectedId(id);
    if (id === null) setDetailsOpen(false);
  }

  function selectAndOpen(id: string) {
    batch(() => {
      setPage("library");
      setSelectedId(id);
      setDetailsOpen(true);
    });
  }

  function isRunning(id: string): boolean {
    return running().has(id);
  }

  function setRunningState(id: string, value: boolean) {
    setRunning((prev) => {
      if (prev.has(id) === value) return prev;
      const next = new Set(prev);
      if (value) next.add(id);
      else next.delete(id);
      return next;
    });
  }

  function handleCoreEvent(event: CoreEvent) {
    switch (event.type) {
      case "gamesChanged":
        bumpCover(event.ids);
        scheduleRefresh();
        break;
      case "gameRemoved":
        batch(() => {
          setGames((list) => list.filter((g) => g.id !== event.id));
          setRunningState(event.id, false);
          if (selectedId() === event.id) selectGame(null);
        });
        scheduleRefresh();
        break;
      case "scansQueued":
      case "scanStarted":
      case "scanFailed":
        syncTracker.handle(event);
        break;
      case "libraryScanned":
        syncTracker.handle(event);
        scheduleRefresh();
        break;
      case "gameStarted":
        setRunningState(event.id, true);
        break;
      case "gameStopped":
        setRunningState(event.id, false);
        api
          .getGame(event.id)
          .then(upsertGame)
          .catch((error: unknown) => pushToast("error", errorMessage(error)));
        break;
      case "gameError":
        pushToast("error", event.message);
        break;
    }
  }

  /** Shows the messages the app queued for the UI (e.g. "another build was started"). */
  async function showNotices() {
    try {
      for (const notice of (await api.takeNotices()) ?? []) pushToast("info", notice);
    } catch (error) {
      pushToast("error", errorMessage(error));
    }
  }

  /** Loads initial data and subscribes to core events. Returns a cleanup function. */
  async function init(): Promise<() => void> {
    const unlisteners: (() => void)[] = [];
    let disposed = false;
    const keep = (fn: () => void) => {
      if (disposed) fn();
      else unlisteners.push(fn);
    };
    const subscribe = Promise.all([
      listen<CoreEvent>("core-event", (e) => handleCoreEvent(e.payload)).then(keep),
      listen("app-notice", () => void showNotices()).then(keep),
    ]);
    const loads = Promise.allSettled([
      api.listProviders().then(setProviders),
      api.appInfo().then(setAppInfo),
      api.runningGames().then((ids) => setRunning(new Set(ids))),
      refresh(),
    ]).then((results) => {
      for (const r of results)
        if (r.status === "rejected") pushToast("error", errorMessage(r.reason));
    });
    await Promise.all([subscribe.catch(() => undefined), loads]);
    void showNotices();
    return () => {
      disposed = true;
      for (const fn of unlisteners) fn();
      clearTimeout(refreshTimer);
    };
  }

  function reset() {
    clearTimeout(refreshTimer);
    requestSeq++;
    batch(() => {
      setQuery(defaultQuery());
      setGames([]);
      setLoading(false);
      setSelectedId(null);
      setDetailsOpen(false);
      setPage("library");
      libraryScroll.grid = 0;
      libraryScroll.list = 0;
      libraryScroll.details = 0;
      syncTracker.reset();
      setRunning(new Set<string>());
      setProviders([]);
      setAppInfo(null);
      setSeenSources(new Set<string>());
      setToasts([]);
      setViewModeSignal(loadViewMode());
    });
  }

  return {
    // state
    query,
    games,
    loading,
    selectedId,
    selectedIndex,
    selectedGame,
    detailsOpen,
    running,
    providers,
    appInfo,
    sources,
    toasts,
    viewMode,
    setViewMode,
    syncState,
    syncBusy: () => isSyncBusy(syncState()),
    page,
    showSettings: () => setPage("settings"),
    showLibrary: () => setPage("library"),
    rememberLibraryScroll: (mode: ViewMode, top: number) => {
      libraryScroll[mode] = top;
    },
    libraryScroll: (mode: ViewMode) => libraryScroll[mode],
    // queries
    setSearch: (search: string | null) => updateQuery({ search }),
    setSort: (sort: SortField) => updateQuery({ sort, descending: defaultDescending(sort) }),
    toggleDirection: () => updateQuery((q) => ({ ...q, descending: !q.descending })),
    setView: (view: LibraryView) => {
      setPage("library");
      updateQuery((q) => withLibraryView(q, view));
    },
    toggleSourceFilter: (id: string) => {
      setPage("library");
      updateQuery((q) => ({ ...q, sources: toggleSource(q.sources, id) }));
    },
    setInstalledOnly: (installedOnly: boolean) => {
      setPage("library");
      updateQuery({ installedOnly });
    },
    refresh,
    scheduleRefresh,
    upsertGame,
    bumpCover,
    coverUrl,
    backgroundUrl,
    // selection
    selectGame,
    selectAndOpen,
    openDetails: () => setDetailsOpen(selectedId() !== null),
    closeDetails: () => setDetailsOpen(false),
    isRunning,
    // toasts
    pushToast,
    dismissToast,
    // lifecycle
    handleCoreEvent,
    init,
    reset,
  };
}

export const store = createRoot(createAppStore);
