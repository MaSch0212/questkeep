import { translator } from "../i18n";
import { gameChrome } from "../i18n/gameChrome";
import { createEffect, Show } from "solid-js";
import { store } from "../lib/store";
import { DetailsPanel } from "./DetailsPanel";
import { GameList } from "./GameList";
import { MultiSelectPanel } from "./MultiSelectPanel";
import { Splitter } from "./Splitter";

const tr = translator(gameChrome);

/**
 * The "details" library view: a narrow, virtualized list on the left and the selected game's
 * full details page on the right, with a splitter between them (width remembered per view).
 * Something is always selected while the library has games.
 */
export function DetailsView(props: {
  listWidth: number;
  minListWidth: number;
  maxListWidth: number;
}) {
  // Nothing selected (first visit, or the selection was filtered out): show the first game.
  createEffect(() => {
    if (store.selectedIndex() >= 0) return;
    const first = store.games()[0];
    if (first) store.selectGame(first.id);
  });

  return (
    <div class="dview">
      <div class="dview__list" style={{ width: `${props.listWidth}px` }}>
        <GameList compact />
      </div>
      <Splitter
        label={tr("listWidth")}
        value={props.listWidth}
        min={props.minListWidth}
        max={props.maxListWidth}
        grows="right"
        onChange={(width) => store.setListWidth(width)}
      />
      <div class="dview__page">
        <Show
          when={store.isMulti()}
          fallback={
            <Show
              when={store.selectedGame()}
              fallback={<p class="dview__empty muted">{tr("selectToSee")}</p>}
            >
              {(game) => (
                <DetailsPanel game={game()} variant="page" onClose={() => store.closeDetails()} />
              )}
            </Show>
          }
        >
          <MultiSelectPanel
            games={store.selectedGames()}
            variant="page"
            onClose={() => store.clearSelection()}
          />
        </Show>
      </div>
    </div>
  );
}
