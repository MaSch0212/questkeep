import { For, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { store } from "../lib/store";
import { Cover } from "./Cover";
import { QuickAction } from "./QuickAction";
import { playCopy, storesOf } from "../lib/copies";
import { SourceIcon, sourceName, storeNames } from "./SourceIcon";
import CloudDownload from "lucide-solid/icons/cloud-download";
import EyeOff from "lucide-solid/icons/eye-off";
import Play from "lucide-solid/icons/play";
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
  /** A short date shown on the cover (the start page shelves). */
  date?: string;
}

export function tileDomId(id: string): string {
  return `tile-${id}`;
}

/**
 * A game card: a strip with the store and state icons above an uncropped cover. The title, the
 * other stores' logos and Play/Install show on hover or selection. Used by the grid and the start
 * page shelves.
 */
export function GameTile(props: GameTileProps) {
  const running = () => store.isRunning(props.game.id);
  // The store whose copy plays comes first; the others only show on hover.
  const primary = () => playCopy(props.game)?.source ?? props.game.source;
  const stores = () => {
    const first = primary();
    return [first, ...storesOf(props.game).filter((s) => s !== first)];
  };

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
      <div class="tile__body">
        <div class="tile__strip">
          <div class="tile__source" title={storeNames(props.game)} aria-hidden="true">
            <span class="tile__primary">
              <SourceIcon source={primary()} />
              <Show when={stores().length > 1}>
                <span class="tile__more" />
              </Show>
            </span>
            <span class="tile__source-name">{sourceName(primary())}</span>
            <Show when={stores().length > 1}>
              <span class="source-icons tile__all">
                <For each={stores()}>{(source) => <SourceIcon source={source} />}</For>
              </span>
            </Show>
          </div>
          <div class="tile__states">
            <Show when={props.game.favorite}>
              <span class="tile__state tile__state--star" title="Favorite">
                <Star class="icon icon--filled" aria-hidden="true" />
              </span>
            </Show>
            <Show when={props.game.hidden}>
              <span class="tile__state" title="Hidden">
                <EyeOff class="icon" aria-hidden="true" />
              </span>
            </Show>
            <Show when={running()}>
              <span class="tile__state tile__state--ring tile__state--running" title="Running">
                <Play class="icon icon--filled" aria-hidden="true" />
              </span>
            </Show>
            <Show when={!props.game.installed}>
              <span class="tile__state tile__state--ring" title="Not installed">
                <CloudDownload class="icon" aria-hidden="true" />
              </span>
            </Show>
          </div>
        </div>
        <div class="tile__cover">
          <Cover game={props.game} showName />
          <Show when={props.date}>
            <span class="tile__date">{props.date}</span>
          </Show>
          <div class="tile__title" title={props.game.name}>
            <span>{props.game.name}</span>
          </div>
          {/* A running game has nothing to start; the strip shows its state. */}
          <Show when={!running()}>
            <div class="tile__quick">
              <QuickAction game={props.game} tabbable={props.selected} />
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
