import Search from "lucide-solid/icons/search";
import { createSignal, For, onCleanup, onMount } from "solid-js";
import type { Game } from "../bindings/Game";
import { translator } from "../i18n";
import { scriptsText } from "../i18n/scripts";
import * as api from "../lib/api";
import { rankMatches } from "../lib/commandBar";
import { defaultQuery } from "../lib/query";
import { GameIcon } from "./GameIcon";
import { Popover, type PopoverAnchor } from "./Popover";

const tr = translator(scriptsText);

const DEBOUNCE_MS = 120;
const RECENT_COUNT = 8;

/**
 * Picks one game of the library: typing searches by name (the first results by relevance), an
 * empty search lists the games played last. Arrows move, Enter chooses.
 */
export function GamePicker(props: {
  anchor: PopoverAnchor;
  label?: string;
  onPick: (game: Game) => void;
  onClose: (returnFocus: boolean) => void;
}) {
  const [text, setText] = createSignal("");
  const [results, setResults] = createSignal<Game[]>([]);
  const [active, setActive] = createSignal(0);
  let input: HTMLInputElement | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;

  async function load(value: string) {
    const mine = ++seq;
    const search = value.trim();
    try {
      const games =
        search === ""
          ? (
              await api.listGames({
                ...defaultQuery(),
                kind: "both",
                sort: "lastPlayed",
                descending: true,
              })
            ).slice(0, RECENT_COUNT)
          : rankMatches(
              await api.listGames({ ...defaultQuery(), search, hidden: "all", kind: "both" }),
              search,
            );
      if (mine !== seq) return;
      setResults(games);
      setActive(0);
    } catch {
      if (mine === seq) setResults([]);
    }
  }

  onMount(() => {
    queueMicrotask(() => input?.focus());
    void load("");
  });
  onCleanup(() => clearTimeout(timer));

  function pick(game: Game | undefined) {
    if (!game) return;
    // The owner may unmount this picker when it closes: it hears of the pick first.
    props.onPick(game);
    props.onClose(true);
  }

  function move(by: number) {
    const count = results().length;
    if (count > 0) setActive((i) => (i + by + count) % count);
  }

  return (
    <Popover
      anchor={props.anchor}
      class="picker"
      role="dialog"
      label={props.label ?? tr("pickGame")}
      onClose={props.onClose}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown") move(1);
        else if (event.key === "ArrowUp") move(-1);
        else if (event.key === "Enter") pick(results()[active()]);
        else return;
        event.preventDefault();
      }}
    >
      <label class="picker__search">
        <Search class="icon" aria-hidden="true" />
        <input
          ref={(el) => (input = el)}
          type="text"
          value={text()}
          placeholder={tr("searchGames")}
          aria-label={tr("searchGames")}
          onInput={(event) => {
            const value = event.currentTarget.value;
            setText(value);
            clearTimeout(timer);
            timer = setTimeout(() => void load(value), DEBOUNCE_MS);
          }}
        />
      </label>
      <div class="picker__list" role="listbox" aria-label={props.label ?? tr("pickGame")}>
        <For each={results()} fallback={<p class="picker__empty">{tr("noGames")}</p>}>
          {(game, index) => (
            <div
              role="option"
              class="picker__item"
              classList={{ "picker__item--active": active() === index() }}
              aria-selected={active() === index()}
              onPointerMove={() => setActive(index())}
              onPointerDown={(event) => event.preventDefault()}
              onClick={() => pick(game)}
            >
              <GameIcon game={game} />
              <span class="picker__label">{game.name}</span>
            </div>
          )}
        </For>
      </div>
    </Popover>
  );
}
