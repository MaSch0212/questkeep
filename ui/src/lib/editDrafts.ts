// Drafts of the Edit game dialog: what it shows for a game that doesn't exist yet (Add game) and
// the launch profile list it saves at once.
import type { Game } from "../bindings/Game";
import type { GameEditInfo } from "../bindings/GameEditInfo";
import type { GameMetadata } from "../bindings/GameMetadata";
import type { ImageSlot } from "../bindings/ImageSlot";
import type { LaunchProfiles } from "../bindings/LaunchProfiles";
import type { PlayAction } from "../bindings/PlayAction";
import type { ProfilesDraft } from "../bindings/ProfilesDraft";

/** The id of the game Add game edits before it exists (also its one copy's id). */
export const NEW_GAME_ID = "new";

/** An image change waiting for Save: a file path or URL to use, or `null` to remove it. */
export type ImageDrafts = ReadonlyMap<ImageSlot, string | null>;

export function emptyMetadata(): GameMetadata {
  return {
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
  };
}

const optional = (text: string) => (text.trim() === "" ? null : text.trim());

/** A file launch with optional arguments and working directory. */
export function fileAction(target: string, args: string, workingDir: string): PlayAction {
  return {
    kind: "file",
    target: target.trim(),
    args: optional(args),
    workingDir: optional(workingDir),
    tracking: "default",
    trackingPath: null,
  };
}

/** The game Add game edits: a manual game with nothing set yet. */
export function newGame(): Game {
  const now = new Date().toISOString();
  return {
    id: NEW_GAME_ID,
    name: "",
    sortingName: "",
    source: "manual",
    sourceGameId: null,
    installed: true,
    installDir: null,
    playAction: fileAction("", "", ""),
    playtimeSeconds: 0,
    playCount: 0,
    lastPlayed: null,
    dateAdded: now,
    favorite: false,
    hidden: false,
    kind: "game",
    detectLaunches: true,
    ignoredProcesses: [],
    coverPath: null,
    iconPath: null,
    backgroundPath: null,
    ...emptyMetadata(),
    fieldSources: {},
    completionStatus: null,
    userScore: null,
    notes: null,
    categories: [],
    copies: [
      {
        id: NEW_GAME_ID,
        source: "manual",
        sourceGameId: null,
        installed: true,
        playtimeSeconds: 0,
        lastPlayed: null,
      },
    ],
    metadataUpdatedAt: null,
    extra: {},
  };
}

/** What the Edit dialog would load for a game that doesn't exist yet: nothing. */
export function newGameInfo(): GameEditInfo {
  return {
    edits: { meta: emptyMetadata(), edited: [], hiddenLinks: [] },
    automatic: emptyMetadata(),
    automaticSources: {},
    links: [],
    automaticSortingName: "",
    sortingName: null,
    userImages: { cover: false, background: false, icon: false },
    metadataLink: null,
  };
}

/** The profiles of a game that doesn't exist yet: its executable. */
export function newGameProfiles(executableName: string): LaunchProfiles {
  return {
    profiles: [
      {
        id: NEW_GAME_ID,
        name: executableName,
        customName: null,
        copyId: NEW_GAME_ID,
        source: "manual",
        own: false,
        args: null,
        action: null,
        argsSupported: true,
        installed: true,
      },
    ],
    defaultId: null,
    playId: NEW_GAME_ID,
  };
}

/**
 * The profile list as the core saves it. `id` maps ids of the edited game (the Add game draft's
 * id becomes the real game's).
 */
export function profilesDraft(
  profiles: LaunchProfiles,
  id: (draftId: string) => string = (x) => x,
): ProfilesDraft {
  return {
    profiles: profiles.profiles.map((profile) => ({
      id: id(profile.id),
      own: profile.own,
      name: profile.customName,
      copyId: profile.own && profile.copyId !== null ? id(profile.copyId) : null,
      args: profile.args,
      action: profile.own ? profile.action : null,
    })),
    defaultId: profiles.defaultId === null ? null : id(profiles.defaultId),
  };
}

/** Whether two launches are the same (for "did the user change it"). */
export function sameAction(a: PlayAction | null, b: PlayAction | null): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
