import { invoke } from "@tauri-apps/api/core";
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
export const runningGames = () => invoke<string[]>("running_games");
export const appInfo = () => invoke<AppInfo>("app_info");
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const seedFakeLibrary = (count: number) => invoke<number>("seed_fake_library", { count });
