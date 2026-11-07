import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { batch, createEffect, createMemo, createRoot, createSignal, on, onCleanup } from "solid-js";
import { createStore } from "solid-js/store";
import type { AccountWarning } from "../bindings/AccountWarning";
import { setLanguage, translator } from "../i18n";
import { shellCommon } from "../i18n/shellCommon";
import type { AppInfo } from "../bindings/AppInfo";
import type { Category } from "../bindings/Category";
import type { CompletionSettings } from "../bindings/CompletionSettings";
import type { CoreEvent } from "../bindings/CoreEvent";
import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import type { GroupBy } from "../bindings/GroupBy";
import type { LibraryView } from "../bindings/LibraryView";
import type { ListField } from "../bindings/ListField";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import type { Settings } from "../bindings/Settings";
import type { PriceSite } from "../bindings/PriceSite";
import type { SetupState } from "../bindings/SetupState";
import type { SortField } from "../bindings/SortField";
import type { StartPage } from "../bindings/StartPage";
import type { StartupPage } from "../bindings/StartupPage";
import type { SyncTimes } from "../bindings/SyncTimes";
import type { ViewLayout } from "../bindings/ViewLayout";
import type { ViewColumn } from "../bindings/ViewColumn";
import type { ValueCount } from "../bindings/ValueCount";
import type { ViewSection } from "../bindings/ViewSection";
import type { ViewUiState } from "../bindings/ViewUiState";
import type { PopoverAnchor } from "../components/Popover";
import * as api from "./api";
import { errorMessage } from "./errors";
import { groupGames } from "./grouping";
import { defaultDescending, defaultQuery, mergeSources } from "./query";
import {
  allViews,
  effectiveState,
  EXPLORE_ID,
  exploreView,
  findView,
  sameFilters,
  sectionOrder,
  viewForGame,
  withChange,
  withoutDraft,
  type Drafts,
  type ViewState,
} from "./views";
import { backupLabel, IDLE_WORK, workLabel, type BackgroundWork } from "./backgroundWork";
import {
  createSyncTracker,
  IDLE_SYNC,
  isSyncBusy,
  syncResultText,
  type SyncState,
} from "./syncStatus";
import { takeLegacyViewMode, type ViewMode } from "./viewMode";
import { HOME_PAGE, startupTarget } from "./startPage";
import { rangeSelection, toggleSelected } from "./selection";
import { DEFAULT_VIEW_UI, collapsedKeys, createDebouncedSaver, toggleCollapsed } from "./viewUi";

const tr = translator(shellCommon);

/** What "Save as new view" asks for. */
export interface NewViewInput {
  name: string;
  icon: string;
  sectionId: string;
}

export type Page = "home" | "library" | "settings";

export interface Toast {
  id: number;
  kind: "info" | "error";
  message: string;
  /** A button on the toast (e.g. "Review"). */
  action?: { label: string; run: () => void };
}

const REFRESH_DEBOUNCE_MS = 60;
/** How long UI state (panel, selection, collapsed groups) waits before it is saved. */
const UI_SAVE_DELAY_MS = 500;

/** The result of a call, or null when it fails. */
function optional<T>(call: () => Promise<T>): Promise<T | null> {
  return Promise.resolve()
    .then(call)
    .catch(() => null);
}

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
  const [completion, setCompletion] = createSignal<CompletionSettings>({
    statuses: [],
    newGame: null,
    firstStart: null,
  });
  // Values of list fields with game counts, loaded when a filter needs them.
  const [valueLists, setValueLists] = createSignal<Partial<Record<ListField, ValueCount[]>>>({});
  // A game opened from the command bar that no view contains; its details show anyway.
  const [pinnedGame, setPinnedGame] = createSignal<Game | null>(null);
  /** A request to scroll the open grid or list to a group (`seq` makes repeats distinct). */
  const [groupJump, setGroupJump] = createSignal<{ key: string; seq: number } | null>(null);
  let jumpSeq = 0;
  /** The open game context menu, if any. */
  const [contextMenu, setContextMenu] = createSignal<{
    id: string;
    anchor: PopoverAnchor;
    returnTo: Element | null;
    /** The game itself, for one that isn't in the open view (the apps launcher). */
    game?: Game;
  } | null>(null);
  /** Games on the combine screen, if it is open. */
  const [combining, setCombining] = createSignal<readonly string[] | null>(null);
  const [games, setGames] = createSignal<Game[]>([]);
  const [loading, setLoading] = createSignal(false);
  /** What each view remembers (panel, selection, collapsed groups); Explore's is never saved. */
  const [viewUi, setViewUi] = createSignal<Readonly<Record<string, ViewUiState>>>({});
  // Before any view is open (tests, startup) the selection lives under this key, never saved.
  const NO_VIEW = "";
  const uiOf = (id: string | null) => viewUi()[id ?? NO_VIEW] ?? DEFAULT_VIEW_UI;
  const activeUi = createMemo(() => uiOf(activeViewId()));
  /** The open view's selected game. */
  const selectedId = () => activeUi().selected;
  /** Games selected together (multi-select), in selection order; empty for one game. */
  const [multi, setMulti] = createSignal<ReadonlySet<string>>(new Set());
  /** Where a Shift range starts: the game clicked last without Shift. */
  let rangeAnchor: string | null = null;
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
  /** Whether the first-run setup or the "no IGDB key" hint shows; null until loaded. */
  const [setup, setSetup] = createSignal<SetupState | null>(null);
  /** The settings section to scroll to when the settings page opens next. */
  const [settingsSection, setSettingsSection] = createSignal<string | null>(null);
  const [coverVersions, setCoverVersions] = createStore<Record<string, number>>({});
  const [page, setPage] = createSignal<Page>("library");
  const [showStartPage, setShowStartPage] = createSignal(true);
  /** Price comparison sites offered in the + menu. */
  const [priceSites, setPriceSites] = createSignal<PriceSite[]>([]);
  const [startData, setStartData] = createSignal<StartPage | null>(null);
  // Set once the startup page was chosen; from then on the open page is remembered.
  const [started, setStarted] = createSignal(false);
  const [syncState, setSyncState] = createSignal<SyncState>(IDLE_SYNC);
  const [backgroundWork, setBackgroundWork] = createSignal<BackgroundWork>(IDLE_WORK);
  /** The running backup: files written of all (total 0 while it starts); null when none runs. */
  const [backupProgress, setBackupProgress] = createSignal<{ done: number; total: number } | null>(
    null,
  );
  /** Counts finished backups, so the backup list reloads. */
  const [backupsChanged, setBackupsChanged] = createSignal(0);
  /** Store accounts whose sign-in ended without the user signing out (sync panel). */
  const [accountWarnings, setAccountWarnings] = createSignal<AccountWarning[]>([]);
  /** A quit waits for the running backup: the quit dialog shows. */
  const [quitDuringBackup, setQuitDuringBackup] = createSignal(false);

  // Explore's starting point; its changes are an ordinary draft on top.
  const [explore, setExplore] = createSignal<LibraryView>(exploreView());
  const activeView = createMemo(() =>
    activeViewId() === EXPLORE_ID ? explore() : findView(sections(), activeViewId()),
  );
  const viewState = createMemo<ViewState>(() => {
    const view = activeView();
    return view
      ? effectiveState(view, drafts())
      : { query: defaultQuery(), layout: "grid", group: "none", columns: [] };
  });
  const query = createMemo<GameQuery>(() => ({ ...viewState().query, search: keyword() }));
  const viewMode = createMemo<ViewMode>(() => viewState().layout);
  const groupBy = createMemo<GroupBy>(() => viewState().group);

  const indexById = createMemo(() => {
    const map = new Map<string, number>();
    games().forEach((game, index) => map.set(game.id, index));
    return map;
  });
  /** Every selected game id: the multi-selection, or the one selected game. */
  const selectedIds = createMemo<ReadonlySet<string>>(() => {
    const ids = multi();
    if (ids.size > 0) return ids;
    const id = selectedId();
    return new Set(id === null ? [] : [id]);
  });
  /** The selected games that are in the list, in selection order. */
  const selectedGames = createMemo(() =>
    [...selectedIds()].flatMap((id) => {
      const index = indexById().get(id);
      return index === undefined ? [] : [games()[index] as Game];
    }),
  );
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
    return groupGames(games(), groupBy(), {
      categories: categories(),
      sourceName: (id) => names.get(id) ?? id,
      storeOrder: providers().map((p) => p.id),
      statuses: completion().statuses,
    });
  });
  const collapsedGroups = createMemo<ReadonlySet<string>>(
    () => new Set(collapsedKeys(activeUi(), groupBy())),
  );

  // Scroll offset of the library, kept while another page (settings) replaces it.
  const libraryScroll: Record<ViewMode, number> = { grid: 0, list: 0, details: 0 };
  const uiSaver = createDebouncedSaver<ViewUiState>((id, state) => {
    void Promise.resolve()
      .then(() => api.setViewUiState(id, state))
      .catch(() => undefined);
  }, UI_SAVE_DELAY_MS);
  /** Writes pending UI state before the page goes away (reload, update install). */
  const flushUi = () => uiSaver.flush();
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
    pushToast("info", tr("duplicatesFound", { count: found.length }), {
      label: tr("review"),
      run: () => setDuplicatesOpen(true),
    });
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
      if (id !== activeViewId()) {
        setKeyword(null);
        setDetailsOpen(false);
        setMulti(new Set<string>());
      }
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
      const ids = new Set([...allViews(list).map((v) => v.id), EXPLORE_ID]);
      setDrafts((current) =>
        Object.fromEntries(Object.entries(current).filter(([id]) => ids.has(id))),
      );
      if (!ids.has(activeViewId() ?? "")) setActiveViewId(allViews(list)[0]?.id ?? null);
    });
  }

  /** Opens Explore fresh (nothing kept from before), optionally preset with filters. */
  function openExplore(preset: Partial<GameQuery> = {}) {
    batch(() => {
      setExplore(exploreView(preset));
      setDrafts((current) => withoutDraft(current, EXPLORE_ID));
      setViewUi((current) => ({ ...current, [EXPLORE_ID]: DEFAULT_VIEW_UI }));
      setActiveViewId(null);
      selectView(EXPLORE_ID);
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
      api.updateView(view.id, {
        query: state.query,
        layout: state.layout,
        group: state.group,
        columns: state.columns,
      }),
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
        columns: state.columns,
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

  let startSeq = 0;
  let startTimer: ReturnType<typeof setTimeout> | undefined;
  /** Loads the start page's shelves and stats. */
  async function loadStart(): Promise<void> {
    const seq = ++startSeq;
    try {
      const data = await api.startPage();
      if (seq === startSeq && data) setStartData(data);
    } catch (error) {
      if (seq === startSeq) pushToast("error", errorMessage(error));
    }
  }

  /** Reloads the start page soon, but only while it is open. */
  function scheduleStartRefresh(delay = REFRESH_DEBOUNCE_MS) {
    clearTimeout(startTimer);
    if (page() !== "home") return;
    startTimer = setTimeout(() => void loadStart(), delay);
  }

  function showHome() {
    if (!showStartPage()) return;
    batch(() => {
      setPage("home");
      setDetailsOpen(false);
    });
    void loadStart();
  }

  /** Applies saved settings the app shell depends on (the start page switch). */
  function applySettings(settings: Settings) {
    setLanguage(settings.language);
    setShowStartPage(settings.showStartPage);
    setPriceSites(settings.priceSites);
    if (!settings.showStartPage && page() === "home") {
      const first = allViews(sections())[0];
      if (first) selectView(first.id);
      else setPage("library");
    }
  }

  /** Opens the page chosen in the settings, once the views are loaded. */
  function openStartupPage(startup: StartupPage, lastPage: string | null) {
    const ids = allViews(sections()).map((v) => v.id);
    const target = startupTarget(startup, showStartPage(), lastPage, ids);
    if (target.kind === "home") showHome();
    else if (target.id !== null && target.id !== activeViewId()) selectView(target.id);
    setStarted(true);
  }

  // Remember the open start page or view for "The page I had open last" (never settings).
  createEffect(
    on([started, page, activeViewId], ([ready, current, viewId]) => {
      if (!ready || current === "settings") return;
      const value = current === "home" ? HOME_PAGE : viewId;
      // Explore always starts fresh, so it is never reopened at startup.
      if (value === null || value === EXPLORE_ID) return;
      void Promise.resolve()
        .then(() => api.setLastPage(value))
        .catch(() => undefined);
    }),
  );

  // Today's playtime grows while a game runs.
  createEffect(
    on([page, running], ([current, ids]) => {
      if (current !== "home" || ids.size === 0) return;
      const timer = setInterval(() => void loadStart(), 60_000);
      onCleanup(() => clearInterval(timer));
    }),
  );

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

  async function loadCompletion() {
    const settings = await optional(api.completionSettings);
    if (settings) setCompletion(settings);
  }

  /** Saves the completion statuses; games of removed ones get `replacements[id]`. */
  async function saveCompletion(
    settings: CompletionSettings,
    replacements: Record<string, string | null>,
  ): Promise<boolean> {
    try {
      setCompletion(await api.setCompletionSettings(settings, replacements));
      return true;
    } catch (error) {
      pushToast("error", errorMessage(error));
      return false;
    }
  }

  /** Sets the completion status of games and shows the result right away. */
  async function setCompletionStatus(ids: string[], status: string | null) {
    try {
      await api.setCompletionStatus(ids, status);
      for (const game of await Promise.all(ids.map((id) => api.getGame(id)))) {
        upsertGame(game);
        if (pinnedGame()?.id === game.id) setPinnedGame(game);
      }
    } catch (error) {
      pushToast("error", errorMessage(error));
    }
  }

  /** Loads (again) the values of a list field for the filters. */
  async function loadValues(field: ListField) {
    const list = await optional(() => api.listValues(field));
    if (list) setValueLists((current) => ({ ...current, [field]: list }));
  }

  /** The name of a list filter value, from what is loaded (else the key itself). */
  function valueName(field: ListField, key: string): string {
    switch (field) {
      case "source":
        return sources().find((s) => s.id === key)?.name ?? key;
      case "category":
        return categories().find((c) => c.id === key)?.name ?? key;
      case "completionStatus":
        return completion().statuses.find((s) => s.id === key)?.name ?? key;
      default:
        return valueLists()[field]?.find((v) => v.key === key)?.name ?? key;
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

  /** Changes what a view remembers; saved shortly after (never for Explore). */
  function updateUi(id: string | null, change: (state: ViewUiState) => ViewUiState) {
    const key = id ?? NO_VIEW;
    const next = change(uiOf(key));
    setViewUi((current) => ({ ...current, [key]: next }));
    if (key !== EXPLORE_ID && key !== NO_VIEW) uiSaver.schedule(key, next);
  }

  /** Selects a game in the open view (remembered per view). */
  function setSelectedId(id: string | null) {
    if (selectedId() === id) return;
    updateUi(activeViewId(), (state) => ({ ...state, selected: id }));
  }

  function toggleGroup(key: string) {
    updateUi(activeViewId(), (state) => toggleCollapsed(state, groupBy(), key));
  }

  /** Expands a group if needed and asks the open grid or list to scroll to it. */
  function jumpToGroup(key: string) {
    if (collapsedGroups().has(key)) toggleGroup(key);
    setGroupJump({ key, seq: ++jumpSeq });
  }

  /** Collapses (or expands) every group of the open view's grouping. */
  function setAllGroupsCollapsed(collapse: boolean) {
    const keys = collapse ? groups().map((g) => g.key) : [];
    updateUi(activeViewId(), (state) => ({
      ...state,
      collapsed: { ...state.collapsed, [groupBy()]: keys },
    }));
  }

  /** Opens a game's context menu; focus goes back to where it was when it closes. Pass the
   *  game when the open view may not contain it. */
  function openContextMenu(id: string, anchor: PopoverAnchor, game?: Game) {
    setContextMenu({ id, anchor, returnTo: document.activeElement, game });
  }

  function closeContextMenu(returnFocus: boolean) {
    const open = contextMenu();
    setContextMenu(null);
    if (returnFocus && open?.returnTo instanceof HTMLElement) {
      open.returnTo.focus({ preventScroll: true });
    }
  }
  /** Loads what each view remembers. Changes made before it arrived win. */
  async function loadViewUi() {
    const loaded = (await api.viewUiStates()) ?? {};
    setViewUi((current) => ({ ...loaded, ...current }));
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

  /** Changes the open view's table columns (a draft until saved, like its sort). */
  function changeColumns(change: (columns: readonly ViewColumn[]) => ViewColumn[]) {
    const view = activeView();
    if (!view) return;
    // Columns don't change which games show: no reload.
    setDrafts((current) =>
      withChange(current, view, (state) => ({ ...state, columns: change(state.columns) })),
    );
  }

  function selectGame(id: string | null) {
    setMulti(new Set<string>());
    rangeAnchor = id;
    setSelectedId(id);
    if (id === null) setDetailsOpen(false);
  }

  function selectAndOpen(id: string) {
    batch(() => {
      setPage("library");
      setMulti(new Set<string>());
      rangeAnchor = id;
      setSelectedId(id);
      setDetailsOpen(true);
    });
  }

  /** Ctrl+click: adds the game to the selection or takes it out. */
  function toggleSelect(id: string) {
    const next = toggleSelected(multi(), selectedId(), id);
    batch(() => {
      setMulti(next);
      rangeAnchor = id;
      setSelectedId(next.has(id) ? id : ([...next].at(-1) ?? null));
    });
  }

  /** Shift+click and Shift+arrows: selects from the anchor to `id` in the view's order. */
  function selectRange(order: readonly string[], id: string) {
    const ids = rangeSelection(order, rangeAnchor ?? selectedId(), id);
    batch(() => {
      setMulti(new Set(ids));
      setSelectedId(id);
    });
  }

  /** Ctrl+A: selects every game in the view. */
  function selectAll(order: readonly string[]) {
    if (order.length === 0) return;
    setMulti(new Set(order));
  }

  function clearSelection() {
    setMulti(new Set<string>());
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
      case "gameRemoved":
      case "libraryScanned":
      case "gameStarted":
      case "gameStopped":
        scheduleStartRefresh();
        break;
      default:
        break;
    }
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
        void loadAccountWarnings();
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
      case "installFinished":
        api
          .getGame(event.id)
          .then((game) => {
            upsertGame(game);
            pushToast("info", tr("installed", { name: game.name }), {
              label: tr("play"),
              run: () =>
                void api.launchGame(game.id).catch((error: unknown) => {
                  pushToast("error", errorMessage(error));
                }),
            });
          })
          .catch((error: unknown) => pushToast("error", errorMessage(error)));
        break;
      case "categoriesChanged":
        void loadCategories();
        break;
      case "backgroundWork":
        setBackgroundWork((work) => ({ ...work, [event.queue]: event.pending }));
        break;
      case "backupProgress":
        setBackupProgress({ done: event.done, total: event.total });
        break;
      case "backupFinished":
        batch(() => {
          setBackupProgress(null);
          setBackupsChanged((n) => n + 1);
        });
        if (event.error !== null) pushToast("error", tr("backupFailed", { error: event.error }));
        break;
      case "quitDuringBackup":
        setQuitDuringBackup(true);
        break;
    }
  }

  /** A sign-in can end during a scan (an expired session) or be renewed in settings. */
  async function loadAccountWarnings() {
    const warnings = await optional(api.accountWarnings);
    if (warnings) setAccountWarnings(warnings);
  }

  async function dismissAccountWarning(provider: string) {
    try {
      await api.dismissAccountWarning(provider);
      setAccountWarnings((list) => list.filter((w) => w.provider !== provider));
    } catch (error) {
      pushToast("error", errorMessage(error));
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

  /** Loads whether the first-run setup or the IGDB hint shows. */
  async function loadSetup() {
    setSetup((await api.setupState()) ?? null);
  }

  /** Ends the first-run setup (finished or skipped) and shows the IGDB hint if it now applies. */
  async function endSetup() {
    try {
      await api.finishSetup();
      await loadSetup();
    } catch (error) {
      pushToast("error", errorMessage(error));
      setSetup(null);
      setContextMenu(null);
      setGroupJump(null);
    }
  }

  async function dismissIgdbHint() {
    setSetup((current) => current && { ...current, showIgdbHint: false });
    try {
      await api.dismissIgdbHint();
    } catch (error) {
      pushToast("error", errorMessage(error));
    }
  }

  /** Opens the settings scrolled to one section. */
  function showSettingsSection(section: string) {
    setSettingsSection(section);
    setPage("settings");
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
      loadCompletion(),
      api.appInfo().then(setAppInfo),
      api.runningGames().then((ids) => setRunning(new Set(ids))),
      // The list needs the open view first; the startup page needs the views and settings.
      // Neither settings nor the last page may keep the list from loading.
      Promise.all([
        loadViews(),
        optional(api.getSettings),
        optional(api.lastPage),
        loadViewUi().catch(() => undefined),
      ]).then(([, settings, lastPage]) => {
        if (settings) {
          setLanguage(settings.language);
          setShowStartPage(settings.showStartPage);
          setPriceSites(settings.priceSites);
        }
        openStartupPage(settings?.startupPage ?? { kind: "start" }, lastPage ?? null);
        return refresh();
      }),
      loadSyncTimes(),
      loadLaunchers(),
      loadCategories(),
      loadUpdate(),
      loadSetup(),
      loadAccountWarnings(),
    ]).then((results) => {
      for (const r of results)
        if (r.status === "rejected") pushToast("error", errorMessage(r.reason));
    });
    await Promise.all([subscribe.catch(() => undefined), loads]);
    window.addEventListener("beforeunload", flushUi);
    void showNotices();
    void showUpdateRequest();
    // A quit held back while the window did not exist yet (the app waited in the tray).
    void Promise.resolve(api.quitPending())
      .then((pending) => {
        if (pending === true) setQuitDuringBackup(true);
      })
      .catch(() => {});
    return () => {
      disposed = true;
      for (const fn of unlisteners) fn();
      window.removeEventListener("beforeunload", flushUi);
      uiSaver.flush();
      clearTimeout(refreshTimer);
      clearTimeout(startTimer);
    };
  }

  function reset() {
    clearTimeout(refreshTimer);
    requestSeq++;
    batch(() => {
      setSections([]);
      setAccountWarnings([]);
      setBackupProgress(null);
      setQuitDuringBackup(false);
      setActiveViewId(null);
      setDrafts({});
      setKeyword(null);
      setViewCounts({});
      setEditingViews(false);
      setSyncTimes({ all: null, providers: {} });
      setLaunchers([]);
      setCategories([]);
      uiSaver.cancel();
      setViewUi({});
      setPinnedGame(null);
      setGames([]);
      setLoading(false);
      setDetailsOpen(false);
      setPage("library");
      setShowStartPage(true);
      setStartData(null);
      setStarted(false);
      libraryScroll.grid = 0;
      libraryScroll.list = 0;
      libraryScroll.details = 0;
      syncTracker.reset();
      setBackgroundWork(IDLE_WORK);
      setRunning(new Set<string>());
      setProviders([]);
      setAppInfo(null);
      setUpdate(null);
      setUpdateOpen(false);
      setSetup(null);
      setContextMenu(null);
      setCombining(null);
      setMulti(new Set<string>());
      setGroupJump(null);
      setSettingsSection(null);
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
    /** The open view's table columns as stored (empty: the defaults). */
    columns: () => viewState().columns,
    changeColumns,
    groups,
    collapsedGroups,
    groupJump,
    jumpToGroup,
    setAllGroupsCollapsed,
    /** The game whose context menu is open (fresh from the list), if any. */
    contextMenu: () => {
      const open = contextMenu();
      const game = open && (games().find((g) => g.id === open.id) ?? open.game ?? selectedGame());
      if (!open || game?.id !== open.id) return null;
      // On a game of a multi-selection the menu acts on all selected games.
      const all = selectedGames();
      const several = all.length > 1 && selectedIds().has(game.id);
      return { game, games: several ? all : [game], anchor: open.anchor };
    },
    openContextMenu,
    closeContextMenu,
    combining,
    /** Opens the combine screen for these games (two or more). */
    openCombine: (ids: readonly string[]) => setCombining(ids.length > 1 ? [...ids] : null),
    closeCombine: () => setCombining(null),
    /** The details panel floats over the list in the open view (grid and table). */
    panelFloating: () => activeUi().panelFloating,
    setPanelFloating: (floating: boolean) =>
      updateUi(activeViewId(), (state) => ({ ...state, panelFloating: floating })),
    /** The open view's details panel width in px; null is the theme default. */
    panelWidth: () => activeUi().panelWidth,
    setPanelWidth: (width: number) =>
      updateUi(activeViewId(), (state) => ({ ...state, panelWidth: width })),
    /** The open view's list width in the details layout in px; null is the theme default. */
    listWidth: () => activeUi().listWidth,
    setListWidth: (width: number) =>
      updateUi(activeViewId(), (state) => ({ ...state, listWidth: width })),
    toggleGroup,
    // completion statuses and filter values
    completion,
    loadCompletion,
    saveCompletion,
    setCompletionStatus,
    valueLists,
    loadValues,
    valueName,
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
    /** "Downloading …" while covers or details are queued, else null. */
    backgroundWorkLabel: () => backupLabel(backupProgress()) ?? workLabel(backgroundWork()),
    backupRunning: () => backupProgress() !== null,
    accountWarnings,
    loadAccountWarnings,
    dismissAccountWarning,
    backupsChanged,
    quitDuringBackup,
    setQuitDuringBackup,
    page,
    showSettings: () => setPage("settings"),
    priceSites,
    showSettingsSection,
    takeSettingsSection: () => {
      const section = settingsSection();
      setSettingsSection(null);
      return section;
    },
    setup,
    loadSetup,
    endSetup,
    dismissIgdbHint,
    showHome,
    showStartPage,
    startData,
    applySettings,
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
    openExplore,
    isExplore: () => activeViewId() === EXPLORE_ID,
    /** Whether the open view filters differently from how it was saved (Explore always does). */
    filtersChanged: () => {
      const view = activeView();
      return (
        activeViewId() === EXPLORE_ID || (!!view && !sameFilters(viewState().query, view.query))
      );
    },
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
    toggleSelect,
    selectRange,
    selectAll,
    clearSelection,
    selectedIds,
    selectedGames,
    isSelected: (id: string) => selectedIds().has(id),
    /** Several games are selected (the multi-select panel shows). */
    isMulti: () => selectedGames().length > 1,
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
