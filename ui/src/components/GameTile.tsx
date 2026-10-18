import { Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { store } from "../lib/store";
import { Cover } from "./Cover";

interface GameTileProps {
  game: Game;
  selected: boolean;
  onSelect: (id: string) => void;
  onLaunch: (id: string) => void;
}

export function tileDomId(id: string): string {
  return `tile-${id}`;
}

export function GameTile(props: GameTileProps) {
  const running = () => store.isRunning(props.game.id);

  return (
    <div
      role="gridcell"
      id={tileDomId(props.game.id)}
      class="tile"
      classList={{ "tile--selected": props.selected, "tile--missing": !props.game.installed }}
      aria-selected={props.selected}
      aria-label={props.game.name}
      onClick={() => props.onSelect(props.game.id)}
      onDblClick={() => props.onLaunch(props.game.id)}
    >
      <div class="tile__cover">
        <Cover game={props.game} />
        <div class="tile__badges">
          <Show when={props.game.favorite}>
            <span class="badge badge--star" title="Favorite">
              ★
            </span>
          </Show>
          <Show when={running()}>
            <span class="badge badge--running">Running</span>
          </Show>
        </div>
      </div>
      <div class="tile__title" title={props.game.name}>
        {props.game.name}
      </div>
    </div>
  );
}
