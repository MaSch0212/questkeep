import { translator } from "../i18n";
import { libraryList } from "../i18n/libraryList";
import { Index, Match, Show, Switch } from "solid-js";
import type { Game } from "../bindings/Game";
import { shownColumns, type ShownColumn } from "../lib/columns";
import { store } from "../lib/store";
import { GameIcon } from "./GameIcon";
import { QuickAction } from "./QuickAction";
import { storesOf } from "../lib/copies";
import { SourceIcons, sourceName, storeNames } from "./SourceIcon";
import Download from "lucide-solid/icons/download";
import Star from "lucide-solid/icons/star";

interface GameRowProps {
  game: Game;
  /** The view's columns after Name; the default ones if not given. */
  columns?: ShownColumn[];
  /** The keyboard cursor is on this row. */
  selected: boolean;
  /** The details panel is showing this game. */
  open?: boolean;
  onSelect: (id: string, event: MouseEvent) => void;
  /** Right-click: the context menu at the pointer. */
  onContextMenu?: (event: MouseEvent) => void;
  onLaunch: (id: string) => void;
  /** Unique within the list when the same game shows in several groups; the game id by default. */
  domKey?: string;
}

const tr = translator(libraryList);

const DEFAULT_SHOWN = shownColumns([]);

export function rowDomId(id: string): string {
  return `row-${id}`;
}

/** One fixed-height row of the list view. */
export function GameRow(props: GameRowProps) {
  const running = () => store.isRunning(props.game.id);
  const stores = () => storesOf(props.game);
  const names = {
    status: (id: string) => store.completion().statuses.find((s) => s.id === id)?.name,
    category: (id: string) => store.categories().find((c) => c.id === id)?.name,
  };

  return (
    <div
      role="option"
      id={rowDomId(props.domKey ?? props.game.id)}
      class="list-row"
      classList={{
        "list-row--selected": props.selected,
        "list-row--open": props.open === true,
        "list-row--missing": !props.game.installed,
      }}
      aria-selected={props.selected}
      aria-label={
        props.game.installed ? props.game.name : tr("notInstalledName", { name: props.game.name })
      }
      onClick={(event) => props.onSelect(props.game.id, event)}
      onContextMenu={(event) => {
        if (!props.onContextMenu) return;
        event.preventDefault();
        props.onContextMenu(event);
      }}
      onDblClick={() => props.onLaunch(props.game.id)}
    >
      <div class="list-row__thumb">
        <GameIcon game={props.game} />
      </div>
      <div class="list-row__name">
        <span class="list-row__text" data-tooltip={props.game.name}>
          {props.game.name}
        </span>
        <Show when={props.game.favorite}>
          <span
            class="list-row__star"
            role="img"
            aria-label={tr("favorite")}
            data-tooltip={tr("favorite")}
          >
            <Star class="icon icon--filled" aria-hidden="true" />
          </span>
        </Show>
        {/* In the wide name column, so it never covers the data columns. */}
        <div class="list-row__quick">
          <QuickAction game={props.game} tabbable={props.selected} />
        </div>
      </div>
      <Index each={props.columns ?? DEFAULT_SHOWN}>
        {(column) => (
          <Switch
            fallback={(() => {
              const text = () => column().def.text?.(props.game, names) ?? "";
              return (
                <div
                  class="list-row__cell"
                  classList={{ "list-row__cell--numeric": column().def.numeric === true }}
                  data-tooltip={text()}
                >
                  {text()}
                </div>
              );
            })()}
          >
            <Match when={column().def.id === "source"}>
              <div class="list-row__cell list-row__source" data-tooltip={storeNames(props.game)}>
                <SourceIcons game={props.game} />
                <Show
                  when={stores().length === 1}
                  fallback={<span class="visually-hidden">{storeNames(props.game)}</span>}
                >
                  <span class="list-row__text">{sourceName(props.game.source)}</span>
                </Show>
              </div>
            </Match>
            <Match when={column().def.id === "state"}>
              <div class="list-row__cell list-row__state">
                <Show
                  when={props.game.installed}
                  fallback={
                    <span class="state state--missing">
                      <Download class="icon" aria-hidden="true" />
                      <span>{tr("notInstalled")}</span>
                    </span>
                  }
                >
                  <span classList={{ "state state--running": running() }}>
                    {running() ? tr("running") : tr("installed")}
                  </span>
                </Show>
              </div>
            </Match>
          </Switch>
        )}
      </Index>
    </div>
  );
}
