import type { Game } from "./bindings/Game";

export function makeGame(overrides: Partial<Game> = {}): Game {
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
    coverPath: null,
    iconPath: null,
    backgroundPath: null,
    description: null,
    releaseDate: null,
    developers: [],
    publishers: [],
    genres: [],
    metadataUpdatedAt: null,
    extra: {},
    ...overrides,
  };
}
