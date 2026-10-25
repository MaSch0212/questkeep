import Download from "lucide-solid/icons/download";
import Play from "lucide-solid/icons/play";
import Search from "lucide-solid/icons/search";
import { createSignal, For, onCleanup, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { installWithToast, launchWithToast } from "../lib/actions";
import * as api from "../lib/api";
import { rankMatches, splitMatch } from "../lib/commandBar";
import { primaryAction } from "../lib/primaryAction";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { GameIcon } from "./GameIcon";
import { sourceName, SourceIcon } from "./SourceIcon";

const DEBOUNCE_MS = 80;
const RECENT_COUNT = 5;

const optionId = (index: number) => `command-result-${index}`;

/** What the app's Ctrl+K shortcut needs. */
export interface CommandBarHandle {
  focus: () => void;
}

/**
 * "Find a game…" in the title bar (Ctrl+K). Finds any game, whichever view is open: arrows
 * select, Enter opens the game's page, Ctrl+Enter (or the button) plays or installs it.
 */
export function CommandBar(props: { ref?: (handle: CommandBarHandle) => void }) {
  const [open, setOpen] = createSignal(false);
  const [text, setText] = createSignal("");
  const [results, setResults] = createSignal<Game[]>([]);
  const [selected, setSelected] = createSignal(0);
  let input: HTMLInputElement | undefined;
  let root: HTMLDivElement | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;

  props.ref?.({
    focus: () => {
      input?.focus();
      show();
    },
  });

  async function load(value: string) {
    const current = ++seq;
    const search = value.trim();
    try {
      const games = search
        ? rankMatches(await api.listGames({ ...defaultQuery(), search, hidden: "all" }), search)
        : (
            await api.listGames({
              ...defaultQuery(),
              played: "played",
              sort: "lastPlayed",
              descending: true,
            })
          ).slice(0, RECENT_COUNT);
      if (current !== seq) return;
      setResults(games);
      setSelected(0);
    } catch {
      if (current === seq) setResults([]);
    }
  }

  function show() {
    if (open()) return;
    setOpen(true);
    void load(text());
  }

  function close() {
    clearTimeout(timer);
    setOpen(false);
    setText("");
    setResults([]);
    input?.blur();
  }

  function onPointerDown(event: PointerEvent) {
    if (open() && event.target instanceof Node && !root?.contains(event.target)) {
      setOpen(false);
      setText("");
    }
  }
  document.addEventListener("pointerdown", onPointerDown);
  onCleanup(() => {
    document.removeEventListener("pointerdown", onPointerDown);
    clearTimeout(timer);
  });

  function run(game: Game | undefined, act: boolean) {
    if (!game) return;
    close();
    if (!act) {
      store.openGamePage(game);
      return;
    }
    const action = primaryAction(game, store.isRunning(game.id));
    if (action === "install") void installWithToast(game.id);
    else if (action === "play") void launchWithToast(game.id);
    else store.openGamePage(game);
  }

  function onKeyDown(event: KeyboardEvent) {
    const count = results().length;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (count === 0) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      setSelected((i) => (i + step + count) % count);
    } else if (event.key === "Enter") {
      event.preventDefault();
      run(results()[selected()], event.ctrlKey || event.metaKey);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  }

  return (
    <div class="command" classList={{ "command--open": open() }} ref={(el) => (root = el)}>
      <div class="command__box">
        <Search class="icon command__search-icon" aria-hidden="true" />
        <input
          ref={(el) => (input = el)}
          class="command__input"
          type="text"
          role="combobox"
          aria-label="Find a game"
          aria-expanded={open()}
          aria-controls="command-results"
          aria-activedescendant={open() && results().length > 0 ? optionId(selected()) : undefined}
          aria-autocomplete="list"
          placeholder="Find a game…"
          spellcheck={false}
          autocomplete="off"
          value={text()}
          onFocus={show}
          onPointerDown={show}
          onInput={(event) => {
            const value = event.currentTarget.value;
            setText(value);
            setOpen(true);
            clearTimeout(timer);
            timer = setTimeout(() => void load(value), DEBOUNCE_MS);
          }}
          onKeyDown={onKeyDown}
        />
        <kbd class="command__kbd">Ctrl K</kbd>
      </div>
      <Show when={open()}>
        <div class="command__panel">
          <Show when={text().trim() === "" && results().length > 0}>
            <div class="command__heading">Recently played</div>
          </Show>
          <ul id="command-results" class="command__results" role="listbox" aria-label="Games">
            <For each={results()}>
              {(game, index) => {
                const action = () => primaryAction(game, store.isRunning(game.id));
                const parts = () => splitMatch(game.name, text());
                return (
                  <li
                    id={optionId(index())}
                    role="option"
                    aria-selected={index() === selected()}
                    class="command__result"
                    classList={{ "command__result--selected": index() === selected() }}
                    onPointerMove={() => setSelected(index())}
                    // Keeps focus in the input; the click decides what happens.
                    onPointerDown={(event) => event.preventDefault()}
                    onClick={(event) => {
                      const onButton =
                        event.target instanceof Element && event.target.closest("button");
                      run(game, onButton !== null && onButton !== false);
                    }}
                  >
                    <GameIcon game={game} />
                    <span class="command__name">
                      {parts().before}
                      <mark>{parts().match}</mark>
                      {parts().after}
                    </span>
                    <span class="command__meta">
                      <SourceIcon source={game.source} />
                      <span class="visually-hidden">{sourceName(game.source)}, </span>
                      {game.hidden ? "Hidden · " : ""}
                      {game.installed ? "Installed" : "Not installed"}
                    </span>
                    <Show when={action() === "play" || action() === "install"}>
                      <button
                        type="button"
                        class="command__act"
                        classList={{ "command__act--play": action() === "play" }}
                        tabIndex={-1}
                        aria-label={`${action() === "play" ? "Play" : "Install"} ${game.name}`}
                        title={`${action() === "play" ? "Play" : "Install"} (Ctrl+Enter)`}
                      >
                        <Show
                          when={action() === "play"}
                          fallback={<Download class="icon" aria-hidden="true" />}
                        >
                          <Play class="icon" aria-hidden="true" />
                        </Show>
                        {action() === "play" ? "Play" : "Install"}
                      </button>
                    </Show>
                  </li>
                );
              }}
            </For>
          </ul>
          <Show when={results().length === 0 && text().trim() !== ""}>
            <p class="command__empty">No game matches “{text().trim()}”.</p>
          </Show>
          <div class="command__hints" aria-hidden="true">
            <span>
              <kbd>↑</kbd> <kbd>↓</kbd> select
            </span>
            <span>
              <kbd>Enter</kbd> open page
            </span>
            <span>
              <kbd>Ctrl Enter</kbd> play or install
            </span>
            <span>
              <kbd>Esc</kbd> close
            </span>
          </div>
        </div>
      </Show>
    </div>
  );
}
