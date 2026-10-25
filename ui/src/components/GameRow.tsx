import { Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { formatPlaytime, formatRelativeDate, releaseYear } from "../lib/format";
import { store } from "../lib/store";
import { Cover } from "./Cover";
import { QuickAction } from "./QuickAction";
import { SourceIcon, sourceName } from "./SourceIcon";
import Download from "lucide-solid/icons/download";
import Star from "lucide-solid/icons/star";

interface GameRowProps {
  game: Game;
  /** The keyboard cursor is on this row. */
  selected: boolean;
  /** The details panel is showing this game. */
  open?: boolean;
  onSelect: (id: string) => void;
  onLaunch: (id: string) => void;
}

export function rowDomId(id: string): string {
  return `row-${id}`;
}

/** One fixed-height row of the list view. */
export function GameRow(props: GameRowProps) {
  const running = () => store.isRunning(props.game.id);
  const source = () => sourceName(props.game.source);

  return (
    <div
      role="option"
      id={rowDomId(props.game.id)}
      class="list-row"
      classList={{
        "list-row--selected": props.selected,
        "list-row--open": props.open === true,
        "list-row--missing": !props.game.installed,
      }}
      aria-selected={props.selected}
      aria-label={props.game.installed ? props.game.name : `${props.game.name} (not installed)`}
      onClick={() => props.onSelect(props.game.id)}
      onDblClick={() => props.onLaunch(props.game.id)}
    >
      <div class="list-row__thumb">
        <Cover game={props.game} />
      </div>
      <div class="list-row__name" title={props.game.name}>
        <span class="list-row__text">{props.game.name}</span>
        <Show when={props.game.favorite}>
          <span class="list-row__star" title="Favorite">
            <Star class="icon icon--filled" aria-hidden="true" />
          </span>
        </Show>
        {/* In the wide name column, so it never covers the data columns. */}
        <div class="list-row__quick">
          <QuickAction game={props.game} tabbable={props.selected} />
        </div>
      </div>
      <div class="list-row__cell list-row__source" title={source()}>
        <SourceIcon source={props.game.source} />
        <span class="list-row__text">{source()}</span>
      </div>
      <div class="list-row__cell list-row__state">
        <Show
          when={props.game.installed}
          fallback={
            <span class="state state--missing">
              <Download class="icon" aria-hidden="true" />
              <span>Not installed</span>
            </span>
          }
        >
          <span classList={{ "state state--running": running() }}>
            {running() ? "Running" : "Installed"}
          </span>
        </Show>
      </div>
      <div class="list-row__cell">{formatPlaytime(props.game.playtimeSeconds)}</div>
      <div class="list-row__cell">{formatRelativeDate(props.game.lastPlayed)}</div>
      <div class="list-row__cell list-row__year">{releaseYear(props.game.releaseDate) ?? ""}</div>
    </div>
  );
}
