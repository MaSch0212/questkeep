import { Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { store } from "../lib/store";
import { Cover } from "./Cover";
import { QuickAction } from "./QuickAction";
import { SourceIcon, sourceName } from "./SourceIcon";
import Download from "lucide-solid/icons/download";
import Star from "lucide-solid/icons/star";

interface GameTileProps {
  game: Game;
  /** The keyboard cursor is on this tile. */
  selected: boolean;
  /** The details panel is showing this game. */
  open?: boolean;
  onSelect: (id: string) => void;
  onLaunch: (id: string) => void;
  /** Unique within the grid when the same game shows in several groups; the game id by default. */
  domKey?: string;
}

export function tileDomId(id: string): string {
  return `tile-${id}`;
}

export function GameTile(props: GameTileProps) {
  const running = () => store.isRunning(props.game.id);

  return (
    <div
      role="gridcell"
      id={tileDomId(props.domKey ?? props.game.id)}
      class="tile"
      classList={{
        "tile--selected": props.selected,
        "tile--open": props.open === true,
        "tile--missing": !props.game.installed,
      }}
      aria-selected={props.selected}
      aria-label={props.game.installed ? props.game.name : `${props.game.name} (not installed)`}
      onClick={() => props.onSelect(props.game.id)}
      onDblClick={() => props.onLaunch(props.game.id)}
    >
      <div class="tile__cover">
        <Cover game={props.game} />
        <div class="tile__source" title={sourceName(props.game.source)} aria-hidden="true">
          <SourceIcon source={props.game.source} />
          <span class="tile__source-name">{sourceName(props.game.source)}</span>
        </div>
        <div class="tile__quick">
          <QuickAction game={props.game} tabbable={props.selected} />
        </div>
        <div class="tile__badges tile__badges--top">
          <Show when={props.game.favorite}>
            <span class="badge badge--star" title="Favorite">
              <Star class="icon icon--filled" aria-hidden="true" />
            </span>
          </Show>
          <Show when={running()}>
            <span class="badge badge--running">Running</span>
          </Show>
        </div>
        <Show when={!props.game.installed}>
          <div class="tile__badges tile__badges--bottom">
            <span class="badge badge--missing" title="Not installed. Double-click to install.">
              <Download class="icon" aria-hidden="true" />
              <span>Not installed</span>
            </span>
          </div>
        </Show>
      </div>
      <div class="tile__title" title={props.game.name}>
        {props.game.name}
      </div>
    </div>
  );
}
