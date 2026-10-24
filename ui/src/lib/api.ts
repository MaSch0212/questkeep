import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AccountStatus } from "../bindings/AccountStatus";
import type { AppInfo } from "../bindings/AppInfo";
import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import type { GameUpdate } from "../bindings/GameUpdate";
import type { NewManualGame } from "../bindings/NewManualGame";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import type { ScanSummary } from "../bindings/ScanSummary";
import type { Settings } from "../bindings/Settings";

/** The only module that talks to the Rust core. Every command may reject with a string. */
export const listGames = (query: GameQuery) => invoke<Game[]>("list_games", { query });
export const getGame = (id: string) => invoke<Game>("get_game", { id });
export const addManualGame = (input: NewManualGame) => invoke<Game>("add_manual_game", { input });
export const updateGame = (id: string, update: GameUpdate) =>
  invoke<Game>("update_game", { id, update });
export const setCover = (id: string, imagePath: string | null) =>
  invoke<Game>("set_cover", { id, imagePath });
export const removeGame = (id: string) => invoke<void>("remove_game", { id });
export const listProviders = () => invoke<ProviderInfo[]>("list_providers");
export const scanLibrary = (providerId: string) =>
  invoke<ScanSummary>("scan_library", { providerId });
export const launchGame = (id: string) => invoke<void>("launch_game", { id });
export const installGame = (id: string) => invoke<void>("install_game", { id });
export const uninstallGame = (id: string) => invoke<void>("uninstall_game", { id });
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
