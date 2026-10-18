import { createSignal, For, onCleanup } from "solid-js";
import type { SortField } from "../bindings/SortField";
import { normalizeSearch } from "../lib/query";
import { store } from "../lib/store";

const SEARCH_DEBOUNCE_MS = 80;

const SORTS: { id: SortField; label: string }[] = [
  { id: "name", label: "Name" },
  { id: "lastPlayed", label: "Last played" },
  { id: "playtime", label: "Playtime" },
  { id: "dateAdded", label: "Date added" },
];

interface TopBarProps {
  inputRef: (element: HTMLInputElement) => void;
}

export function TopBar(props: TopBarProps) {
  const [text, setText] = createSignal("");
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));

  function onInput(value: string) {
    setText(value);
    clearTimeout(timer);
    timer = setTimeout(() => store.setSearch(normalizeSearch(value)), SEARCH_DEBOUNCE_MS);
  }

  function clear() {
    clearTimeout(timer);
    setText("");
    store.setSearch(null);
  }

  return (
    <header class="topbar">
      <input
        ref={props.inputRef}
        class="input topbar__search"
        type="search"
        placeholder="Search games  (Ctrl+F or /)"
        aria-label="Search games"
        value={text()}
        onInput={(event) => onInput(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape" && text() !== "") {
            event.preventDefault();
            event.stopPropagation();
            clear();
          }
        }}
      />
      <div class="topbar__spacer" />
      <span class="topbar__count" aria-live="polite">
        {store.games().length} {store.games().length === 1 ? "game" : "games"}
      </span>
      <select
        class="input topbar__sort"
        aria-label="Sort by"
        value={store.query().sort}
        onChange={(event) => store.setSort(event.currentTarget.value as SortField)}
      >
        <For each={SORTS}>{(sort) => <option value={sort.id}>{sort.label}</option>}</For>
      </select>
      <button
        type="button"
        class="icon-btn icon-btn--bordered"
        aria-label={store.query().descending ? "Sort descending" : "Sort ascending"}
        title={store.query().descending ? "Descending" : "Ascending"}
        onClick={() => store.toggleDirection()}
      >
        {store.query().descending ? "↓" : "↑"}
      </button>
    </header>
  );
}
