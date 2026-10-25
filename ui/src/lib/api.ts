import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AccountStatus } from "../bindings/AccountStatus";
import type { AppInfo } from "../bindings/AppInfo";
import type { Category } from "../bindings/Category";
import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import type { GameUpdate } from "../bindings/GameUpdate";
import type { NewManualGame } from "../bindings/NewManualGame";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import type { ScanSummary } from "../bindings/ScanSummary";
import type { LibraryView } from "../bindings/LibraryView";
import type { NewView } from "../bindings/NewView";
import type { SectionOrder } from "../bindings/SectionOrder";
import type { Settings } from "../bindings/Settings";
import type { SyncTimes } from "../bindings/SyncTimes";
import type { ViewSection } from "../bindings/ViewSection";
import type { ViewUpdate } from "../bindings/ViewUpdate";

/** The only module that talks to the Rust core. Every command may reject with a string. */
export const listGames = (query: GameQuery) => invoke<Game[]>("list_games", { query });
export const getGame = (id: string) => invoke<Game>("get_game", { id });
/** How many games each query matches, in order. */
export const countGames = (queries: GameQuery[]) => invoke<number[]>("count_games", { queries });
export const listViews = () => invoke<ViewSection[]>("list_views");
export const createView = (view: NewView) => invoke<LibraryView>("create_view", { view });
export const updateView = (id: string, update: ViewUpdate) =>
  invoke<LibraryView>("update_view", { id, update });
export const duplicateView = (id: string) => invoke<LibraryView>("duplicate_view", { id });
export const deleteView = (id: string) => invoke<void>("delete_view", { id });
export const createViewSection = (name: string) =>
  invoke<ViewSection>("create_view_section", { name });
export const renameViewSection = (id: string, name: string) =>
  invoke<void>("rename_view_section", { id, name });
export const deleteViewSection = (id: string) => invoke<void>("delete_view_section", { id });
export const arrangeViews = (order: SectionOrder[]) => invoke<void>("arrange_views", { order });
export const syncTimes = () => invoke<SyncTimes>("sync_times");
export const listCategories = () => invoke<Category[]>("list_categories");
/** Creates a category, or returns the existing one with that name. */
export const createCategory = (name: string) => invoke<Category>("create_category", { name });
export const renameCategory = (id: string, name: string) =>
  invoke<Category>("rename_category", { id, name });
export const deleteCategory = (id: string) => invoke<void>("delete_category", { id });
/** Puts the categories in this order. */
export const reorderCategories = (ids: string[]) => invoke<void>("reorder_categories", { ids });
/** Merges `from` into `into` (games and store categories); returns `into`. */
export const mergeCategories = (from: string, into: string) =>
  invoke<Category>("merge_categories", { from, into });
/** Gives a store category merged into `id` its own category again; returns that category. */
export const separateCategorySource = (id: string, source: string, name: string) =>
  invoke<Category>("separate_category_source", { id, source, name });
/** Gives a game exactly these categories; returns the updated game. */
export const setGameCategories = (id: string, categoryIds: string[]) =>
  invoke<Game>("set_game_categories", { id, categoryIds });
/** Ids of the providers whose store launcher is installed. */
export const listLaunchers = () => invoke<string[]>("list_launchers");
export const openLauncher = (providerId: string) => invoke<void>("open_launcher", { providerId });
/** Quits Questkeep, even when it would otherwise keep running in the tray. */
export const exitApp = () => invoke<void>("exit_app");
export const addManualGame = (input: NewManualGame) => invoke<Game>("add_manual_game", { input });
export const updateGame = (id: string, update: GameUpdate) =>
  invoke<Game>("update_game", { id, update });
export const setCover = (id: string, imagePath: string | null) =>
  invoke<Game>("set_cover", { id, imagePath });
export const removeGame = (id: string) => invoke<void>("remove_game", { id });
export const listProviders = () => invoke<ProviderInfo[]>("list_providers");
export const scanLibrary = (providerId: string) =>
  invoke<ScanSummary>("scan_library", { providerId });
/** Scans every store; progress arrives as core events. */
export const scanAllLibraries = () => invoke<void>("scan_all_libraries");
/** `copyId` picks one store's copy of a game owned in several; by default the core picks. */
export const launchGame = (id: string, copyId: string | null = null) =>
  invoke<void>("launch_game", { id, copyId });
export const installGame = (id: string, copyId: string | null = null) =>
  invoke<void>("install_game", { id, copyId });
export const uninstallGame = (id: string, copyId: string | null = null) =>
  invoke<void>("uninstall_game", { id, copyId });
/** Combines other games into `into` (the same game in several stores); returns the entry. */
export const combineGames = (into: string, others: string[]) =>
  invoke<Game>("combine_games", { into, others });
/** Takes a store's copy out of its combined game; returns the remaining game. */
export const separateCopy = (id: string) => invoke<Game>("separate_copy", { id });
/** Makes a store's copy the one Play starts; returns its game. */
export const preferCopy = (id: string) => invoke<Game>("prefer_copy", { id });
/** Groups of games that look like the same game in different stores, best entry first. */
export const findDuplicates = () => invoke<Game[][]>("find_duplicates");
/** Remembers that these games are different. */
export const dismissDuplicates = (ids: string[]) => invoke<void>("dismiss_duplicates", { ids });
/** Opens a provider's own way to add games; the new game is imported once the provider sees it. */
export const addGameWithOption = (providerId: string, optionId: string) =>
  invoke<void>("add_game_with_option", { providerId, optionId });
export const refreshMetadata = (id: string) => invoke<void>("refresh_metadata", { id });
export const runningGames = () => invoke<string[]>("running_games");
export const appInfo = () => invoke<AppInfo>("app_info");
/** Messages the app queued for the UI (each returned once), e.g. after a second start. */
export const takeNotices = () => invoke<string[]>("take_notices");
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
/** Opens the app's data or logs folder in the file manager (done by the Rust side). */
export const openAppFolder = (kind: "data" | "logs") => invoke<void>("open_app_folder", { kind });
export const seedFakeLibrary = (count: number) => invoke<number>("seed_fake_library", { count });

export const refetchArtwork = (id: string) => invoke<void>("refetch_artwork", { id });

export const accountStatus = () => invoke<AccountStatus[]>("account_status");
/** Resolves `true` once signed in, `false` if the user closed the login window. Can take minutes. */
export const signIn = (provider: string) => invoke<boolean>("sign_in", { provider });
export const signInWithCode = (provider: string, code: string) =>
  invoke<AccountStatus>("sign_in_with_code", { provider, code });
export const signOut = (provider: string) => invoke<AccountStatus>("sign_out", { provider });

export interface UpdateInfo {
  version: string;
  notes: string | null;
  date: string | null;
}
/** Resolves `null` when up to date (always in dev builds). */
export const checkUpdate = () => invoke<UpdateInfo | null>("check_update");
/** Downloads, installs and restarts the app. */
export const installUpdate = () => invoke<void>("install_update");

// Window controls for the custom title bar. Close and minimize still go through the shell's
// close/minimize-to-tray handling.
export const minimizeWindow = () => getCurrentWindow().minimize();
export const toggleMaximizeWindow = () => getCurrentWindow().toggleMaximize();
export const closeWindow = () => getCurrentWindow().close();
export const isWindowMaximized = () => getCurrentWindow().isMaximized();
/** Resolves to an unlisten function. */
export const onWindowResized = (handler: () => void) => getCurrentWindow().onResized(handler);
/** Resolves to an unlisten function. */
export const onWindowFocusChanged = (handler: (focused: boolean) => void) =>
  getCurrentWindow().onFocusChanged((event) => handler(event.payload));
