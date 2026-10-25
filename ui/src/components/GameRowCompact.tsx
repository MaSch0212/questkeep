import { Show } from "solid-js";
import type { Game } from "../bindings/Game";
import Star from "lucide-solid/icons/star";
import { store } from "../lib/store";
import { Cover } from "./Cover";
import { SourceIcon, sourceName } from "./SourceIcon";
import { rowDomId } from "./GameRow";

interface GameRowCompactProps {
  game: Game;
  /** This game is the one the page on the right shows. */
  selected: boolean;
  onSelect: (id: string) => void;
}

/** One row of the narrow list in the details view: store icon, small cover, name. */
export function GameRowCompact(props: GameRowCompactProps) {
  const running = () => store.isRunning(props.game.id);
  return (
    <div
      role="option"
      id={rowDomId(props.game.id)}
      class="compact-row"
      classList={{
        "compact-row--selected": props.selected,
        "compact-row--missing": !props.game.installed,
      }}
      aria-selected={props.selected}
      aria-label={props.game.installed ? props.game.name : `${props.game.name} (not installed)`}
      onClick={() => props.onSelect(props.game.id)}
    >
      <span class="compact-row__source" title={sourceName(props.game.source)}>
        <SourceIcon source={props.game.source} />
      </span>
      <div class="list-row__thumb">
        <Cover game={props.game} />
      </div>
      <span class="compact-row__name" title={props.game.name}>
        {props.game.name}
      </span>
      <Show when={running()}>
        <span class="compact-row__running" title="Running" aria-hidden="true" />
      </Show>
      <Show when={props.game.favorite}>
        <span class="list-row__star" title="Favorite">
          <Star class="icon icon--filled" aria-hidden="true" />
        </span>
      </Show>
    </div>
  );
}
