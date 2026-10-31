import type { BulkChange } from "../bindings/BulkChange";
import type { Game } from "../bindings/Game";
import * as api from "./api";
import { errorMessage } from "./errors";
import { store } from "./store";

const plural = (n: number) => `${n} ${n === 1 ? "game" : "games"}`;

/** Changes all `games` at once (one transaction), then shows the result. */
export async function changeAll(games: readonly Game[], change: BulkChange): Promise<void> {
  try {
    await api.bulkChange(
      games.map((g) => g.id),
      change,
    );
    if (change.addCategories.length > 0 || change.removeCategories.length > 0) {
      await store.loadCategories();
    }
    store.scheduleRefresh(0);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

/** Hands every game that is not installed to its store client, one after another. */
export async function installAll(games: readonly Game[]): Promise<void> {
  const missing = games.filter((g) => !g.installed);
  if (missing.length === 0) return;
  const failures: string[] = [];
  await missing.reduce(
    (previous, game) =>
      previous.then(() =>
        api.installGame(game.id).catch((error: unknown) => {
          failures.push(`${game.name}: ${errorMessage(error)}`);
        }),
      ),
    Promise.resolve(),
  );
  if (failures.length > 0) store.pushToast("error", failures.join("\n"));
  else store.pushToast("info", `Install of ${plural(missing.length)} handed to the store clients`);
}

export async function refreshAll(games: readonly Game[]): Promise<void> {
  const results = await Promise.allSettled(games.map((g) => api.refreshMetadata(g.id)));
  const failed = results.filter((r) => r.status === "rejected").length;
  if (failed > 0) store.pushToast("error", `Could not refresh ${plural(failed)}`);
  else store.pushToast("info", `Refreshing metadata for ${plural(games.length)}…`);
}

export async function removeAll(games: readonly Game[]): Promise<void> {
  try {
    await api.removeGames(games.map((g) => g.id));
    store.clearSelection();
    store.pushToast("info", `Removed ${plural(games.length)}`);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}
