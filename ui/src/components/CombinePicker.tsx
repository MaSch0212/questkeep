import { createResource, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { translator } from "../i18n";
import { shellCombine } from "../i18n/shellCombine";
import * as api from "../lib/api";
import { defaultQuery } from "../lib/query";
import { GameIcon } from "./GameIcon";
import { Popover, type PopoverAnchor } from "./Popover";
import { SourceIcons, storeNames } from "./SourceIcon";

const tr = translator(shellCombine);

const SEARCH_DELAY_MS = 150;
const MAX_RESULTS = 30;
const optionId = (index: number) => `combine-option-${index}`;

/**
 * Finds another game to combine with `game` (the same game in another store). Starts with
 * games of the same name; arrows and Enter pick one.
 */
export function CombinePicker(props: {
  game: Game;
  anchor: PopoverAnchor;
  inside: () => (Element | undefined)[];
  onPick: (other: Game) => void;
  onClose: (returnFocus: boolean) => void;
}) {
  const [text, setText] = createSignal(props.game.name);
  const [search, setSearch] = createSignal(props.game.name);
  const [active, setActive] = createSignal(0);
  let input: HTMLInputElement | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  onMount(() =>
    queueMicrotask(() => {
      input?.focus();
      input?.select();
    }),
  );

  const [results] = createResource(search, async (term) => {
    const games = await api.listGames({ ...defaultQuery(), search: term.trim(), hidden: "all" });
    return games.filter((g) => g.id !== props.game.id).slice(0, MAX_RESULTS);
  });
  const options = () => results.latest ?? [];

  function pick(index: number) {
    const game = options()[index];
    if (game) props.onPick(game);
  }

  function onKeyDown(event: KeyboardEvent) {
    const count = options().length;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (count > 0) setActive((i) => (i + (event.key === "ArrowDown" ? 1 : count - 1)) % count);
    } else if (event.key === "Enter") {
      event.preventDefault();
      pick(Math.min(active(), count - 1));
    }
  }

  return (
    <Popover
      anchor={props.anchor}
      class="combine-picker"
      role="dialog"
      label={tr("pickerLabel")}
      inside={props.inside}
      onClose={props.onClose}
    >
      <p class="combine-picker__hint">{tr("pickerHint")}</p>
      <input
        ref={(el) => (input = el)}
        class="input"
        type="text"
        role="combobox"
        aria-label={tr("findGame")}
        aria-expanded="true"
        aria-controls="combine-options"
        aria-activedescendant={options().length > 0 ? optionId(active()) : undefined}
        placeholder={tr("findGameEllipsis")}
        spellcheck={false}
        autocomplete="off"
        value={text()}
        onInput={(event) => {
          const value = event.currentTarget.value;
          setText(value);
          setActive(0);
          clearTimeout(timer);
          timer = setTimeout(() => setSearch(value), SEARCH_DELAY_MS);
        }}
        onKeyDown={onKeyDown}
      />
      <ul id="combine-options" class="combine-picker__list" role="listbox" aria-label={tr("games")}>
        <For each={options()}>
          {(game, index) => (
            <li
              id={optionId(index())}
              role="option"
              aria-selected={index() === active()}
              aria-label={`${game.name} (${storeNames(game)})`}
              class="combine-picker__option"
              classList={{ "combine-picker__option--active": index() === active() }}
              onPointerMove={() => setActive(index())}
              onPointerDown={(event) => event.preventDefault()}
              onClick={() => pick(index())}
            >
              <span class="list-row__thumb">
                <GameIcon game={game} />
              </span>
              <span class="combine-picker__name">{game.name}</span>
              <SourceIcons game={game} />
            </li>
          )}
        </For>
      </ul>
      <Show when={!results.loading && options().length === 0}>
        <p class="combine-picker__empty">{tr("noOther")}</p>
      </Show>
    </Popover>
  );
}
