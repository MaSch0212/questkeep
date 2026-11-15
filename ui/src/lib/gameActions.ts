import { createResource } from "solid-js";
import type { Script } from "../bindings/Script";
import * as api from "./api";
import { errorMessage } from "./errors";
import { actionScripts } from "./scripts";
import { store } from "./store";

/**
 * The enabled action scripts of a game, in menu order; empty while loading, without a game, and
 * when the game has none. Loaded again after any change of scripts.
 */
export function createGameActions(gameId: () => string | null): () => Script[] {
  const [loaded] = createResource(
    () => {
      const id = gameId();
      return id === null ? false : ([id, store.scriptsRevision()] as const);
    },
    ([id]) =>
      Promise.resolve()
        .then(() => api.gameScripts(id))
        .then((scripts) => ({ id, applied: scripts?.applied ?? [] }))
        .catch(() => ({ id, applied: [] as Script[] })),
  );
  return () => {
    const data = loaded();
    // A game switched to a moment ago still shows the previous one's data until its own arrives.
    return data && data.id === gameId() ? actionScripts(data.applied) : [];
  };
}

/** Runs an action script for a game; a failure to start shows as a toast. */
export async function runAction(script: Script, gameId: string): Promise<void> {
  try {
    await api.runScript(script.id, gameId);
  } catch (failure) {
    store.pushToast("error", errorMessage(failure));
  }
}
