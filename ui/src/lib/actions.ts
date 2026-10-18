import type { Game } from "../bindings/Game";
import type { GameUpdate } from "../bindings/GameUpdate";
import type { NewManualGame } from "../bindings/NewManualGame";
import type { PlayAction } from "../bindings/PlayAction";
import * as api from "./api";
import { errorMessage } from "./errors";
import { pickImage } from "./pickers";
import { store } from "./store";

/** Runs `work` and resolves to an error message, or null on success. Never rejects. */
async function attempt(work: () => Promise<void>): Promise<string | null> {
  try {
    await work();
    return null;
  } catch (error) {
    return errorMessage(error);
  }
}

export function launchGame(id: string): Promise<string | null> {
  return attempt(() => api.launchGame(id));
}

/** Keyboard/double-click launch: failures surface as a toast. */
export async function launchWithToast(id: string): Promise<void> {
  if (store.isRunning(id)) return;
  const error = await launchGame(id);
  if (error) store.pushToast("error", error);
}

export function updateGame(id: string, patch: Partial<GameUpdate>): Promise<string | null> {
  return attempt(async () => {
    const update: GameUpdate = {
      name: null,
      favorite: null,
      hidden: null,
      playAction: null,
      ...patch,
    };
    store.upsertGame(await api.updateGame(id, update));
    store.scheduleRefresh();
  });
}

export function savePlayAction(id: string, playAction: PlayAction): Promise<string | null> {
  return updateGame(id, { playAction });
}

export function changeCover(id: string): Promise<string | null> {
  return attempt(async () => {
    const path = await pickImage();
    if (path === null) return;
    store.upsertGame(await api.setCover(id, path));
    store.bumpCover([id]);
  });
}

export function removeCover(id: string): Promise<string | null> {
  return attempt(async () => {
    store.upsertGame(await api.setCover(id, null));
    store.bumpCover([id]);
  });
}

export function removeGame(id: string): Promise<string | null> {
  return attempt(async () => {
    await api.removeGame(id);
    store.handleCoreEvent({ type: "gameRemoved", id });
  });
}

export async function addGame(input: NewManualGame): Promise<{ game: Game } | { error: string }> {
  try {
    const game = await api.addManualGame(input);
    await store.refresh();
    store.selectAndOpen(game.id);
    return { game };
  } catch (error) {
    return { error: errorMessage(error) };
  }
}
