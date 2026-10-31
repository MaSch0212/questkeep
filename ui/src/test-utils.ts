import type { Game } from "./bindings/Game";

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
    categories: [],
    metadataUpdatedAt: null,
    extra: {},
    copies: [],
    ...overrides,
  };
}
