import { createSignal, For, onCleanup, Show } from "solid-js";
import type { SortField } from "../bindings/SortField";
import { normalizeSearch } from "../lib/query";
import { store } from "../lib/store";
import ArrowDown from "lucide-solid/icons/arrow-down";
import ArrowUp from "lucide-solid/icons/arrow-up";
import LayoutGrid from "lucide-solid/icons/layout-grid";
import List from "lucide-solid/icons/list";

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
      <div class="view-toggle" role="group" aria-label="Library view">
        <button
          type="button"
          class="icon-btn icon-btn--bordered"
          classList={{ "icon-btn--active": store.viewMode() === "grid" }}
          aria-label="Grid view"
          aria-pressed={store.viewMode() === "grid"}
          title="Grid view"
          onClick={() => store.setViewMode("grid")}
        >
          <LayoutGrid class="icon" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="icon-btn icon-btn--bordered"
          classList={{ "icon-btn--active": store.viewMode() === "list" }}
          aria-label="List view"
          aria-pressed={store.viewMode() === "list"}
          title="List view"
          onClick={() => store.setViewMode("list")}
        >
          <List class="icon" aria-hidden="true" />
        </button>
      </div>
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
        <Show
          when={store.query().descending}
          fallback={<ArrowUp class="icon" aria-hidden="true" />}
        >
          <ArrowDown class="icon" aria-hidden="true" />
        </Show>
      </button>
    </header>
  );
}
