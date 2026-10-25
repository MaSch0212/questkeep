import Search from "lucide-solid/icons/search";
import X from "lucide-solid/icons/x";
import { createEffect, createSignal, onCleanup } from "solid-js";
import { normalizeSearch } from "../lib/query";
import { store } from "../lib/store";

const DEBOUNCE_MS = 80;

/** What the library page can do with the keyword field (App's keyboard shortcuts). */
export interface KeywordFilterHandle {
  /** Opens the field, optionally starting it with a typed character. */
  open: (initial?: string) => void;
}

/**
 * Narrows the open view by name. An icon until used; opens into a field on click, Ctrl+F, `/`
 * or typing over the games. While it filters it stays open; Escape or × clears and closes it.
 * Never saved with a view, and cleared when another view opens.
 */
export function KeywordFilter(props: { ref?: (handle: KeywordFilterHandle) => void }) {
  const [expanded, setExpanded] = createSignal(false);
  const [text, setText] = createSignal(store.keyword() ?? "");
  let input: HTMLInputElement | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));

  // The store clears the keyword when another view opens; the field follows.
  createEffect(() => {
    if (store.keyword() === null && text() !== "" && !timer) {
      setText("");
      if (document.activeElement !== input) setExpanded(false);
    }
  });

  function apply(value: string) {
    clearTimeout(timer);
    // Armed before the text changes, so the effect above doesn't take it for a cleared keyword.
    timer = setTimeout(() => {
      timer = undefined;
      store.setKeyword(normalizeSearch(value));
    }, DEBOUNCE_MS);
    setText(value);
  }

  function open(initial?: string) {
    setExpanded(true);
    if (initial !== undefined) apply(text() + initial);
    queueMicrotask(() => {
      input?.focus();
      if (initial === undefined) input?.select();
    });
  }

  function clear() {
    clearTimeout(timer);
    timer = undefined;
    setText("");
    store.setKeyword(null);
    setExpanded(false);
  }

  props.ref?.({ open });

  const active = () => text().trim() !== "";

  return (
    <div
      class="keyword"
      classList={{ "keyword--open": expanded() || active(), "keyword--active": active() }}
    >
      <button
        type="button"
        class="keyword__icon"
        aria-label="Filter this view by name (Ctrl+F)"
        title="Filter this view (Ctrl+F)"
        aria-expanded={expanded() || active()}
        // Keeps focus in the field, so clicking the icon of an open, empty field closes it.
        onPointerDown={(event) => event.preventDefault()}
        onClick={() => (expanded() && !active() ? clear() : open())}
      >
        <Search class="icon" aria-hidden="true" />
      </button>
      <input
        ref={(el) => (input = el)}
        class="keyword__input"
        type="text"
        placeholder="Filter this view…"
        aria-label="Filter this view by name"
        spellcheck={false}
        autocomplete="off"
        tabIndex={expanded() || active() ? 0 : -1}
        value={text()}
        onInput={(event) => apply(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            clear();
          }
        }}
        // Collapses only when empty, so a narrowed list never goes unnoticed.
        onBlur={() => {
          if (!active()) setExpanded(false);
        }}
      />
      <button
        type="button"
        class="keyword__clear"
        aria-label="Clear the filter"
        title="Clear (Esc)"
        tabIndex={expanded() || active() ? 0 : -1}
        onClick={clear}
      >
        <X class="icon" aria-hidden="true" />
      </button>
    </div>
  );
}
