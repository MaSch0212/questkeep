import { createEffect, Show } from "solid-js";
import { store } from "../lib/store";
import { DetailsPanel } from "./DetailsPanel";
import { GameList } from "./GameList";

/**
 * The "details" library view: a narrow, virtualized list on the left and the selected game's
 * full details page on the right. Something is always selected while the library has games.
 */
export function DetailsView() {
  // Nothing selected (first visit, or the selection was filtered out): show the first game.
  createEffect(() => {
    if (store.selectedIndex() >= 0) return;
    const first = store.games()[0];
    if (first) store.selectGame(first.id);
  });

  return (
    <div class="dview">
      <div class="dview__list">
        <GameList compact />
      </div>
      <div class="dview__page">
        <Show
          when={store.selectedGame()}
          fallback={<p class="dview__empty muted">Select a game to see its details.</p>}
        >
          {(game) => (
            <DetailsPanel game={game()} variant="page" onClose={() => store.closeDetails()} />
          )}
        </Show>
      </div>
    </div>
  );
}
