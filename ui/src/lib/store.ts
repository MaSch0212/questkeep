import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { batch, createMemo, createRoot, createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import type { AppInfo } from "../bindings/AppInfo";
import type { Category } from "../bindings/Category";
import type { CoreEvent } from "../bindings/CoreEvent";
import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import type { GroupBy } from "../bindings/GroupBy";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import type { SortField } from "../bindings/SortField";
import type { SyncTimes } from "../bindings/SyncTimes";
import type { ViewLayout } from "../bindings/ViewLayout";
import type { ViewSection } from "../bindings/ViewSection";
import * as api from "./api";
import { errorMessage } from "./errors";
import { groupGames } from "./grouping";
import { defaultDescending, defaultQuery, mergeSources } from "./query";
import {
  allViews,
  effectiveState,
  findView,
  sectionOrder,
  viewForGame,
  withChange,
  withoutDraft,
  type Drafts,
  type ViewState,
} from "./views";
import {
  createSyncTracker,
  IDLE_SYNC,
  isSyncBusy,
  syncResultText,
  type SyncState,
} from "./syncStatus";
import { takeLegacyViewMode, type ViewMode } from "./viewMode";

/** What "Save as new view" asks for. */
export interface NewViewInput {
  name: string;
  icon: string;
  sectionId: string;
}

export type Page = "library" | "settings";

export interface Toast {
  id: number;
  kind: "info" | "error";
  message: string;
  /** A button on the toast (e.g. "Review"). */
  action?: { label: string; run: () => void };
}

const REFRESH_DEBOUNCE_MS = 60;

function createAppStore() {
  const [sections, setSections] = createSignal<ViewSection[]>([]);
  const [activeViewId, setActiveViewId] = createSignal<string | null>(null);
  const [drafts, setDrafts] = createSignal<Drafts>({});
  // The keyword filter: narrows the open view by name, never saved with it.
  const [keyword, setKeyword] = createSignal<string | null>(null);
  const [viewCounts, setViewCounts] = createSignal<Readonly<Record<string, number>>>({});
  const [editingViews, setEditingViews] = createSignal(false);
  const [syncTimes, setSyncTimes] = createSignal<SyncTimes>({ all: null, providers: {} });
  const [launchers, setLaunchers] = createSignal<string[]>([]);
  const [categories, setCategories] = createSignal<Category[]>([]);
  // Collapsed groups per view id, until the app closes.
  const [collapsed, setCollapsed] = createSignal<Readonly<Record<string, readonly string[]>>>({});
  // A game opened from the command bar that no view contains; its details show anyway.
  const [pinnedGame, setPinnedGame] = createSignal<Game | null>(null);
  const [games, setGames] = createSignal<Game[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [detailsOpen, setDetailsOpen] = createSignal(false);
  const [running, setRunning] = createSignal<ReadonlySet<string>>(new Set());
  const [providers, setProviders] = createSignal<ProviderInfo[]>([]);
  const [appInfo, setAppInfo] = createSignal<AppInfo | null>(null);
  const [seenSources, setSeenSources] = createSignal<ReadonlySet<string>>(new Set());
  const [toasts, setToasts] = createSignal<Toast[]>([]);
  const [duplicatesOpen, setDuplicatesOpen] = createSignal(false);
  /** A newer app version (checked after start); the title bar offers it. */
  const [update, setUpdate] = createSignal<api.UpdateInfo | null>(null);
  const [updateOpen, setUpdateOpen] = createSignal(false);
  const [coverVersions, setCoverVersions] = createStore<Record<string, number>>({});
  const [page, setPage] = createSignal<Page>("library");
  const [syncState, setSyncState] = createSignal<SyncState>(IDLE_SYNC);

  const activeView = createMemo(() => findView(sections(), activeViewId()));
  const viewState = createMemo<ViewState>(() => {
    const view = activeView();
    return view
      ? effectiveState(view, drafts())
      : { query: defaultQuery(), layout: "grid", group: "none" };
  });
  const query = createMemo<GameQuery>(() => ({ ...viewState().query, search: keyword() }));
  const viewMode = createMemo<ViewMode>(() => viewState().layout);
  const groupBy = createMemo<GroupBy>(() => viewState().group);

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
    const pinned = pinnedGame();
    if (pinned?.id === id) return pinned;
    return previous?.id === id ? previous : undefined;
  });
  const sources = createMemo(() => mergeSources(providers(), seenSources()));
  const groups = createMemo(() => {
    const names = new Map(sources().map((s) => [s.id, s.name]));
    return groupGames(games(), groupBy(), categories(), (id) => names.get(id) ?? id);
  });
  const collapsedGroups = createMemo<ReadonlySet<string>>(
    () => new Set(collapsed()[activeViewId() ?? ""] ?? []),
  );

  // Scroll offset of the library, kept while another page (settings) replaces it.
  const libraryScroll: Record<ViewMode, number> = { grid: 0, list: 0, details: 0 };
  let requestSeq = 0;
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  let toastSeq = 0;

  function dismissToast(id: number) {
    setToasts((list) => list.filter((t) => t.id !== id));
  }

  function pushToast(kind: Toast["kind"], message: string, action?: Toast["action"]) {
    const id = ++toastSeq;
    setToasts((list) => [...list.slice(-4), { id, kind, message, action }]);
    setTimeout(() => dismissToast(id), kind === "error" || action ? 8000 : 4500);
  }

  // Groups of games in several stores that a toast already pointed out this session.
  const notifiedDuplicates = new Set<string>();

  /** After a sync: points out games that newly look like the same game in several stores. */
  async function checkDuplicates() {
    let found: Game[][];
    try {
      found = (await api.findDuplicates()) ?? [];
    } catch {
      return; // only a hint; the dialog shows errors
    }
    const keys = found.map((group) =>
      group
        .map((g) => g.id)
        .toSorted()
        .join(),
    );
    const fresh = keys.filter((key) => !notifiedDuplicates.has(key)).length;
    for (const key of keys) notifiedDuplicates.add(key);
    if (fresh === 0) return;
    pushToast(
      "info",
      found.length === 1
        ? "1 game looks like it's in more than one store"
        : `${found.length} games look like they're in more than one store`,
      { label: "Review", run: () => setDuplicatesOpen(true) },
    );
  }

  // One status for every scan, however it started; one toast when the run ends.
  const syncTracker = createSyncTracker({
    onChange: setSyncState,
    onFinished: (state) => {
      pushToast("info", syncResultText(state));
      void checkDuplicates();
      // The core records the end of a "sync all" right after its last store.
      setTimeout(() => void loadSyncTimes(), 500);
    },
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
      void refreshCounts();
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

  let countSeq = 0;
  /** The sidebar's game count of every view, following unsaved changes. */
  async function refreshCounts(): Promise<void> {
    const seq = ++countSeq;
    const views = allViews(sections());
    if (views.length === 0) return;
    try {
      const counts = await api.countGames(
        views.map((view) => effectiveState(view, drafts()).query),
      );
      if (seq !== countSeq) return;
      setViewCounts(Object.fromEntries(views.map((view, i) => [view.id, counts[i] ?? 0])));
    } catch {
      // Counts are a convenience; the list itself reports errors.
    }
  }

  /** Changes the open view's filters, sort or layout; kept as a draft until saved. */
  function changeView(change: (state: ViewState) => ViewState) {
    const view = activeView();
    if (!view) return;
    setDrafts((current) => withChange(current, view, change));
    scheduleRefresh(0);
  }

  function changeQuery(patch: Partial<GameQuery>) {
    changeView((state) => ({ ...state, query: { ...state.query, ...patch } }));
  }

  function selectView(id: string) {
    batch(() => {
      setPage("library");
      // Coming back from another page to the open view keeps its keyword filter.
      if (id !== activeViewId()) setKeyword(null);
      setActiveViewId(id);
      setPinnedGame(null);
    });
    scheduleRefresh(0);
  }

  /** Loads the views; keeps the open one if it still exists, else opens the first. */
  async function loadViews(): Promise<void> {
    let list = (await api.listViews()) ?? [];
    // Before views existed the layout was remembered locally; it becomes "All games"'s layout.
    const legacy = takeLegacyViewMode();
    const all = findView(list, "all");
    if (legacy !== null && all && all.layout !== legacy) {
      await api.updateView(all.id, { layout: legacy }).catch(() => undefined);
      list = await api.listViews();
    }
    batch(() => {
      setSections(list);
      const ids = new Set(allViews(list).map((v) => v.id));
      setDrafts((current) =>
        Object.fromEntries(Object.entries(current).filter(([id]) => ids.has(id))),
      );
      if (!ids.has(activeViewId() ?? "")) setActiveViewId(allViews(list)[0]?.id ?? null);
    });
  }

  /** Runs a change to the views in the core, then reloads them (also after a failure). */
  async function editViews(action: () => Promise<unknown>): Promise<boolean> {
    let ok = true;
    try {
      await action();
    } catch (error) {
      ok = false;
      pushToast("error", errorMessage(error));
    }
    try {
      await loadViews();
    } catch (error) {
      pushToast("error", errorMessage(error));
    }
    scheduleRefresh(0);
    return ok;
  }

  /** Writes the open view's unsaved changes into it. */
  async function updateActiveView() {
    const view = activeView();
    if (!view) return;
    const state = viewState();
    const ok = await editViews(() =>
      api.updateView(view.id, { query: state.query, layout: state.layout, group: state.group }),
    );
    if (ok) setDrafts((current) => withoutDraft(current, view.id));
  }

  /** Saves the open view's current state as a new view and opens it; the old view goes back
   *  to what it had saved. */
  async function saveAsNewView(input: NewViewInput): Promise<boolean> {
    const from = activeView();
    const state = viewState();
    let created: string | null = null;
    const ok = await editViews(async () => {
      const view = await api.createView({
        sectionId: input.sectionId,
        name: input.name,
        icon: input.icon,
        query: state.query,
        layout: state.layout,
        group: state.group,
      });
      created = view.id;
    });
    if (ok && created !== null) {
      const id: string = created;
      if (from) setDrafts((current) => withoutDraft(current, from.id));
      selectView(id);
    }
    return ok;
  }

  /** Moves views and sections as shown (optimistically), then stores the order. */
  async function arrangeViews(next: ViewSection[]) {
    setSections(next);
    await editViews(() => api.arrangeViews(sectionOrder(next)));
  }

  /**
   * Shows a game's details: in the open view when it contains the game, else in the first view
   * that does. A game no view contains is shown over the first view.
   */
  function openGamePage(game: Game) {
    const target = viewForGame(sections(), drafts(), activeViewId(), game);
    batch(() => {
      setPage("library");
      setKeyword(null);
      setActiveViewId(target?.id ?? allViews(sections())[0]?.id ?? null);
      setPinnedGame(target ? null : game);
      setSelectedId(game.id);
      setDetailsOpen(true);
    });
    scheduleRefresh(0);
  }

  async function loadSyncTimes() {
    try {
      const times = await api.syncTimes();
      if (times) setSyncTimes(times);
    } catch {
      // Only used for "last synced" labels.
    }
  }

  /** Stores and what they can do; signing in or out changes which ones can sync. */
  async function loadProviders() {
    setProviders(await api.listProviders());
  }

  async function loadCategories() {
    try {
      setCategories((await api.listCategories()) ?? []);
    } catch (error) {
      pushToast("error", errorMessage(error));
    }
  }

  /** Runs a category change in the core; the list follows through `categoriesChanged`. */
  async function editCategories<T>(action: () => Promise<T>): Promise<T | null> {
    try {
      return await action();
    } catch (error) {
      pushToast("error", errorMessage(error));
      return null;
    }
  }

  /** Shows the new order at once, then saves it (a failure reloads the saved one). */
  async function reorderCategories(ids: string[]) {
    const byId = new Map(categories().map((c) => [c.id, c]));
    setCategories(ids.flatMap((id) => byId.get(id) ?? []));
    if ((await editCategories(() => api.reorderCategories(ids))) === null) {
      await loadCategories();
    }
  }

  /** Gives a game exactly these categories and shows the result right away. */
  async function setGameCategories(id: string, categoryIds: string[]) {
    const game = await editCategories(() => api.setGameCategories(id, categoryIds));
    if (game) {
      upsertGame(game);
      if (pinnedGame()?.id === id) setPinnedGame(game);
    }
  }

  function toggleGroup(key: string) {
    const view = activeViewId();
    if (view === null) return;
    setCollapsed((current) => {
      const keys = current[view] ?? [];
      const next = keys.includes(key) ? keys.filter((k) => k !== key) : [...keys, key];
      return { ...current, [view]: next };
    });
  }

  async function loadLaunchers() {
    try {
      setLaunchers((await api.listLaunchers()) ?? []);
    } catch (error) {
      pushToast("error", errorMessage(error));
    }
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

  function iconUrl(game: Game): string | null {
    return imageUrl(game, game.iconPath);
  }

  function backgroundUrl(game: Game): string | null {
    return imageUrl(game, game.backgroundPath);
  }

  function setViewMode(mode: ViewLayout) {
    changeView((state) => ({ ...state, layout: mode }));
  }

  function setGroupBy(group: GroupBy) {
    changeView((state) => ({ ...state, group }));
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

  // Copy id -> library entry id, for games combined from several stores.
  const entryOfCopy = createMemo(() => {
    const map = new Map<string, string>();
    for (const game of games()) for (const copy of game.copies) map.set(copy.id, game.id);
    return map;
  });

  /** Whether a game (any of its store copies) is running. */
  function isRunning(id: string): boolean {
    const ids = running();
    if (ids.has(id)) return true;
    for (const runningId of ids) if (entryOfCopy().get(runningId) === id) return true;
    return false;
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
        void loadSyncTimes();
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
      case "categoriesChanged":
        void loadCategories();
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

  async function loadUpdate() {
    setUpdate((await api.availableUpdate()) ?? null);
  }

  /** Opens the update dialog when the tray asked for it. */
  async function showUpdateRequest() {
    try {
      if (!(await api.takeUpdateRequest())) return;
      await loadUpdate();
      if (update()) setUpdateOpen(true);
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
      listen<api.UpdateInfo>("update-available", (e) => setUpdate(e.payload)).then(keep),
      listen("show-update", () => void showUpdateRequest()).then(keep),
    ]);
    const loads = Promise.allSettled([
      loadProviders(),
      api.appInfo().then(setAppInfo),
      api.runningGames().then((ids) => setRunning(new Set(ids))),
      // The list needs the open view first.
      loadViews().then(refresh),
      loadSyncTimes(),
      loadLaunchers(),
      loadCategories(),
      loadUpdate(),
    ]).then((results) => {
      for (const r of results)
        if (r.status === "rejected") pushToast("error", errorMessage(r.reason));
    });
    await Promise.all([subscribe.catch(() => undefined), loads]);
    void showNotices();
    void showUpdateRequest();
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
      setSections([]);
      setActiveViewId(null);
      setDrafts({});
      setKeyword(null);
      setViewCounts({});
      setEditingViews(false);
      setSyncTimes({ all: null, providers: {} });
      setLaunchers([]);
      setCategories([]);
      setCollapsed({});
      setPinnedGame(null);
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
      setUpdate(null);
      setUpdateOpen(false);
      setSeenSources(new Set<string>());
      setToasts([]);
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
    duplicatesOpen,
    setDuplicatesOpen,
    loadProviders,
    update,
    setUpdate,
    updateOpen,
    setUpdateOpen,
    checkDuplicates,
    viewMode,
    setViewMode,
    groupBy,
    setGroupBy,
    groups,
    collapsedGroups,
    toggleGroup,
    // categories
    categories,
    loadCategories,
    setGameCategories,
    createCategory: (name: string) => editCategories(() => api.createCategory(name)),
    renameCategory: (id: string, name: string) =>
      editCategories(() => api.renameCategory(id, name)),
    deleteCategory: (id: string) => editCategories(() => api.deleteCategory(id)),
    reorderCategories,
    mergeCategories: (from: string, into: string) =>
      editCategories(() => api.mergeCategories(from, into)),
    separateCategorySource: (id: string, source: string, name: string) =>
      editCategories(() => api.separateCategorySource(id, source, name)),
    syncState,
    syncBusy: () => isSyncBusy(syncState()),
    page,
    showSettings: () => setPage("settings"),
    showLibrary: () => setPage("library"),
    rememberLibraryScroll: (mode: ViewMode, top: number) => {
      libraryScroll[mode] = top;
    },
    libraryScroll: (mode: ViewMode) => libraryScroll[mode],
    // views
    sections,
    activeView,
    activeViewId,
    drafts,
    hasDraft: (id: string) => id in drafts(),
    viewCounts,
    editingViews,
    setEditingViews,
    selectView,
    loadViews,
    resetView: () => {
      const view = activeView();
      if (view) setDrafts((current) => withoutDraft(current, view.id));
      scheduleRefresh(0);
    },
    updateActiveView,
    saveAsNewView,
    arrangeViews,
    renameView: (id: string, name: string) => editViews(() => api.updateView(id, { name })),
    setViewIcon: (id: string, icon: string) => editViews(() => api.updateView(id, { icon })),
    duplicateView: (id: string) => editViews(() => api.duplicateView(id)),
    deleteView: (id: string) => editViews(() => api.deleteView(id)),
    createSection: async (name: string) => {
      let id: string | null = null;
      await editViews(async () => {
        id = (await api.createViewSection(name)).id;
      });
      return id as string | null;
    },
    renameSection: (id: string, name: string) => editViews(() => api.renameViewSection(id, name)),
    deleteSection: (id: string) => editViews(() => api.deleteViewSection(id)),
    // the open view's filters, sort and layout (kept as a draft until saved)
    keyword,
    setKeyword: (value: string | null) => {
      setKeyword(value);
      scheduleRefresh(0);
    },
    setSort: (sort: SortField) => changeQuery({ sort, descending: defaultDescending(sort) }),
    setSortField: (sort: SortField) => changeQuery({ sort }),
    setDescending: (descending: boolean) => changeQuery({ descending }),
    toggleDirection: () => changeQuery({ descending: !query().descending }),
    changeQuery,
    // sync and launchers
    syncTimes,
    loadSyncTimes,
    launchers,
    loadLaunchers,
    openGamePage,
    refresh,
    scheduleRefresh,
    upsertGame,
    bumpCover,
    coverUrl,
    iconUrl,
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
