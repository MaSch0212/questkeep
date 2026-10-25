import Pencil from "lucide-solid/icons/pencil";
import Trash from "lucide-solid/icons/trash";
import { createSignal, For, Show } from "solid-js";
import type { Category } from "../../bindings/Category";
import { store } from "../../lib/store";
import { SourceIcon, sourceName } from "../SourceIcon";

/** Rename or delete categories; new ones are created on a game's page. */
export function CategoriesSection() {
  return (
    <div class="form">
      <p class="muted">
        Categories group games in views (Group by → Category) and filter them. Add them on a game's
        page. Categories from Steam collections, GOG Galaxy tags and Lutris come back with every
        sync; renaming one makes it yours.
      </p>
      <Show when={store.categories().length > 0} fallback={<p class="muted">No categories yet.</p>}>
        <ul class="category-list">
          <For each={store.categories()}>{(category) => <CategoryRow category={category} />}</For>
        </ul>
      </Show>
    </div>
  );
}

function CategoryRow(props: { category: Category }) {
  const [editing, setEditing] = createSignal(false);
  const [confirming, setConfirming] = createSignal(false);
  const games = () =>
    `${props.category.gameCount} ${props.category.gameCount === 1 ? "game" : "games"}`;

  async function rename(value: string) {
    setEditing(false);
    const name = value.trim();
    if (name !== "" && name !== props.category.name) {
      await store.renameCategory(props.category.id, name);
    }
  }

  return (
    <li class="category-list__row">
      <Show
        when={editing()}
        fallback={
          <>
            <span class="category-list__name">{props.category.name}</span>
            <Show when={props.category.source}>
              {(source) => (
                <span class="category-list__source" title={`Imported from ${sourceName(source())}`}>
                  <SourceIcon source={source()} />
                </span>
              )}
            </Show>
            <span class="category-list__count">{games()}</span>
            <button
              type="button"
              class="icon-btn icon-btn--small"
              aria-label={`Rename ${props.category.name}`}
              title="Rename"
              onClick={() => setEditing(true)}
            >
              <Pencil class="icon" aria-hidden="true" />
            </button>
            <button
              type="button"
              class="icon-btn icon-btn--small category-list__delete"
              aria-label={`Delete ${props.category.name}`}
              title="Delete"
              onClick={() => setConfirming(true)}
            >
              <Trash class="icon" aria-hidden="true" />
            </button>
          </>
        }
      >
        <input
          ref={(el) => queueMicrotask(() => el.select())}
          class="input category-list__input"
          aria-label="Category name"
          value={props.category.name}
          onBlur={(event) => void rename(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.currentTarget.blur();
            } else if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              setEditing(false);
            }
          }}
        />
      </Show>
      <Show when={confirming()}>
        <div class="category-list__confirm" role="alert">
          <span>
            Delete “{props.category.name}”? {games()} lose it; the games stay.
          </span>
          <button type="button" class="btn btn--small" onClick={() => setConfirming(false)}>
            Cancel
          </button>
          <button
            type="button"
            class="btn btn--small btn--danger-solid"
            onClick={() => {
              setConfirming(false);
              void store.deleteCategory(props.category.id);
            }}
          >
            Delete
          </button>
        </div>
      </Show>
    </li>
  );
}
