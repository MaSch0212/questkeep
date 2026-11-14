import type { Game } from "./bindings/Game";
import type { ProviderInfo } from "./bindings/ProviderInfo";
import type { StoreSettings } from "./bindings/StoreSettings";

/** A game; unless `copies` is given, its only copy is itself (as the core reports it). */
export function makeGame(overrides: Partial<Game> = {}): Game {
  const game = makeRawGame(overrides);
  if (overrides.copies) return game;
  const { id, source, sourceGameId, installed, playtimeSeconds, lastPlayed } = game;
  return {
    ...game,
    copies: [{ id, source, sourceGameId, installed, playtimeSeconds, lastPlayed }],
  };
}

function makeRawGame(overrides: Partial<Game>): Game {
  return {
    id: "g1",
    name: "Hades",
    sortingName: "hades",
    source: "manual",
    sourceGameId: null,
    installed: true,
    installDir: "C:\\Games\\Hades",
    playAction: {
      kind: "file",
      target: "C:\\Games\\Hades\\Hades.exe",
      args: null,
      workingDir: null,
      tracking: "default",
      trackingPath: null,
    },
    playtimeSeconds: 0,
    playCount: 0,
    lastPlayed: null,
    dateAdded: "2026-01-02T10:00:00Z",
    favorite: false,
    hidden: false,
    kind: "game",
    detectLaunches: true,
    ignoredProcesses: [],
    coverPath: null,
    iconPath: null,
    backgroundPath: null,
    description: null,
    releaseDate: null,
    developers: [],
    publishers: [],
    genres: [],
    tags: [],
    features: [],
    series: [],
    ageRatings: [],
    platforms: [],
    gameModes: [],
    themes: [],
    perspectives: [],
    criticScore: null,
    communityScore: null,
    installSize: null,
    timeToBeat: null,
    links: [],
    fieldSources: {},
    completionStatus: null,
    userScore: null,
    notes: null,
    categories: [],
    metadataUpdatedAt: null,
    extra: {},
    copies: [],
    ...overrides,
  };
}

/** Store settings as the stores declare them (see `fn settings` in `crates/stores/*`). */
export const NO_STORE_SETTINGS: StoreSettings = { options: [], credentials: null };

export const STEAM_SETTINGS: StoreSettings = {
  options: [],
  credentials: {
    noun: "Steam key",
    offer: "Use a Steam Web API key instead",
    badge: "API key",
    detail: "Using a Steam Web API key",
    intro:
      "A key needs no sign-in, but only works when your profile's game details are public. It is stored in your system's credential store.",
    link: "https://steamcommunity.com/dev/apikey",
    fields: [
      {
        key: "apiKey",
        label: "Steam Web API key",
        hint: null,
        secret: true,
        required: true,
        numeric: false,
      },
      {
        key: "steamId",
        label: "SteamID64 (optional)",
        hint: "Leave empty to use the account last signed in to the Steam client.",
        secret: false,
        required: false,
        numeric: true,
      },
    ],
  },
};

/** A store with one switch. */
export function optionSettings(key: string, label: string): StoreSettings {
  return { options: [{ key, label, hint: "" }], credentials: null };
}

/** A registered store as `listProviders` reports it. */
export function makeProvider(overrides: Partial<ProviderInfo> & { id: string }): ProviderInfo {
  return {
    name: overrides.id,
    icon: "<svg></svg>",
    canScan: true,
    canScanOwned: false,
    canUninstall: false,
    launcherPages: false,
    shopUrl: null,
    installed: true,
    syncable: true,
    addGameOptions: [],
    settings: NO_STORE_SETTINGS,
    ...overrides,
  };
}

/** Every element whose tooltip (`data-tooltip`) is exactly `text`; throws when there is none. */
export function allByTooltip(text: string): HTMLElement[] {
  const found = [...document.querySelectorAll<HTMLElement>("[data-tooltip]")].filter(
    (element) => element.dataset.tooltip === text,
  );
  if (found.length === 0) throw new Error(`No element with the tooltip "${text}"`);
  return found;
}

/** The one element whose tooltip is exactly `text`. */
export function byTooltip(text: string): HTMLElement {
  const found = allByTooltip(text);
  if (found.length > 1) throw new Error(`${found.length} elements with the tooltip "${text}"`);
  return found[0] as HTMLElement;
}
