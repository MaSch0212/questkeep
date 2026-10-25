import GripVertical from "lucide-solid/icons/grip-vertical";
import Merge from "lucide-solid/icons/merge";
import Pencil from "lucide-solid/icons/pencil";
import Trash from "lucide-solid/icons/trash";
import Unlink from "lucide-solid/icons/unlink";
import { createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import type { Category } from "../../bindings/Category";
import { dropIndexAt, moveBefore, nudge } from "../../lib/categoryOrder";
import { store } from "../../lib/store";
import { Menu } from "../Menu";
import type { PopoverAnchor } from "../Popover";
import { SourceIcon, sourceName } from "../SourceIcon";

const sameName = (a: string, b: string) =>
  a.localeCompare(b, undefined, { sensitivity: "base" }) === 0;

/**
 * Orders, renames, merges and deletes categories; new ones are created on a game's page. Rows
 * move by dragging their handle or with Alt+Up/Down. Each row shows the store categories that
 * sync into it.
 */
export function CategoriesSection() {
  const [dragged, setDragged] = createSignal<string | null>(null);
  const [drop, setDrop] = createSignal<number | null>(null);
  const byId = createMemo(() => new Map(store.categories().map((c) => [c.id, c])));
  // Rows keyed by id, so a row (and its focus) survives the list reloading.
  const ids = createMemo(() => store.categories().map((c) => c.id));
  let list: HTMLUListElement | undefined;

  function startDrag(event: PointerEvent, id: string) {
    if (event.button !== 0) return;
    event.preventDefault();
    setDragged(id);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp, { once: true });
  }

  function onPointerMove(event: PointerEvent) {
    if (!list || !dragged()) return;
    const middles = [...list.querySelectorAll<HTMLElement>("[data-category-row]")].map((row) => {
      const rect = row.getBoundingClientRect();
      return rect.top + rect.height / 2;
    });
    setDrop(dropIndexAt(middles, event.clientY));
  }

  function onPointerUp() {
    window.removeEventListener("pointermove", onPointerMove);
    const id = dragged();
    const index = drop();
    setDragged(null);
    setDrop(null);
    if (id && index !== null) {
      const next = moveBefore(ids(), id, index);
      if (next.join() !== ids().join()) void store.reorderCategories(next);
    }
  }

  onCleanup(() => {
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
  });

  function onRowKeyDown(event: KeyboardEvent, id: string) {
    if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
    if (event.target instanceof HTMLInputElement) return;
    event.preventDefault();
    const next = nudge(ids(), id, event.key === "ArrowUp" ? -1 : 1);
    if (next.join() === ids().join()) return;
    // Moving the row's DOM node drops its focus; the node itself stays (rows are keyed by id).
    const focused = event.target instanceof HTMLElement ? event.target : null;
    void store.reorderCategories(next);
    focused?.focus();
  }

  return (
    <div class="form">
      <p class="muted">
        Categories group games in views (Group by → Category) and filter them, in this order: drag a
        row by its handle or press Alt+Up/Down to move it. Steam collections, GOG Galaxy tags and
        Lutris categories sync into the category of the same name; merge categories to sync several
        store categories into one. New categories are added on a game's page.
      </p>
      <Show when={ids().length > 0} fallback={<p class="muted">No categories yet.</p>}>
        <ul
          class="category-list"
          classList={{ "category-list--dragging": dragged() !== null }}
          ref={(el) => (list = el)}
        >
          <For each={ids()}>
            {(id, index) => (
              <Show when={byId().get(id)}>
                {(category) => (
                  <>
                    <Show when={drop() === index() && dragged() !== null}>
                      <li class="category-list__drop" aria-hidden="true" />
                    </Show>
                    <CategoryRow
                      category={category()}
                      dragged={dragged() === id}
                      onDragStart={(event) => startDrag(event, id)}
                      onKeyDown={(event) => onRowKeyDown(event, id)}
                    />
                  </>
                )}
              </Show>
            )}
          </For>
          <Show when={drop() === ids().length && dragged() !== null}>
            <li class="category-list__drop" aria-hidden="true" />
          </Show>
        </ul>
      </Show>
    </div>
  );
}

type Confirm = { kind: "delete" } | { kind: "merge"; into: Category };

function CategoryRow(props: {
  category: Category;
  dragged: boolean;
  onDragStart: (event: PointerEvent) => void;
  onKeyDown: (event: KeyboardEvent) => void;
}) {
  const [editing, setEditing] = createSignal(false);
  const [confirm, setConfirm] = createSignal<Confirm | null>(null);
  const [mergeMenu, setMergeMenu] = createSignal<PopoverAnchor | null>(null);
  let mergeButton: HTMLButtonElement | undefined;
  const games = () =>
    `${props.category.gameCount} ${props.category.gameCount === 1 ? "game" : "games"}`;
  const others = () => store.categories().filter((c) => c.id !== props.category.id);
  const mergeInto = () => {
    const current = confirm();
    return current?.kind === "merge" ? current.into : null;
  };

  async function rename(value: string) {
    setEditing(false);
    const name = value.trim();
    if (name !== "" && name !== props.category.name) {
      await store.renameCategory(props.category.id, name);
    }
  }

  return (
    <li
      class="category-list__row"
      classList={{ "category-list__row--dragged": props.dragged }}
      data-category-row
      onKeyDown={(event) => props.onKeyDown(event)}
    >
      <span
        class="category-list__handle"
        title="Drag to move (or Alt+Up/Down)"
        aria-hidden="true"
        onPointerDown={(event) => props.onDragStart(event)}
      >
        <GripVertical class="icon" />
      </span>
      <Show
        when={editing()}
        fallback={<span class="category-list__name">{props.category.name}</span>}
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
      <span class="category-list__sources">
        <For each={props.category.sources}>
          {(source) => {
            const own = () => sameName(source.name, props.category.name);
            return (
              <span
                class="category-list__source"
                title={`Synced with ${sourceName(source.source)}: “${source.name}”`}
              >
                <SourceIcon source={source.source} />
                <Show when={!own()}>
                  <span class="category-list__source-name">{source.name}</span>
                  <button
                    type="button"
                    class="category-list__separate"
                    aria-label={`Separate ${sourceName(source.source)}'s “${source.name}”`}
                    title="Give it its own category again"
                    onClick={() =>
                      void store.separateCategorySource(
                        props.category.id,
                        source.source,
                        source.name,
                      )
                    }
                  >
                    <Unlink class="icon" aria-hidden="true" />
                  </button>
                </Show>
              </span>
            );
          }}
        </For>
      </span>
      <span class="category-list__count">{games()}</span>
      <button
        ref={(el) => (mergeButton = el)}
        type="button"
        class="icon-btn icon-btn--small"
        aria-label={`Merge ${props.category.name} into…`}
        title="Merge into another category"
        aria-haspopup="menu"
        aria-expanded={mergeMenu() !== null}
        disabled={others().length === 0}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMergeMenu(mergeMenu() ? null : { rect, align: "end" });
        }}
      >
        <Merge class="icon" aria-hidden="true" />
      </button>
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
        onClick={() => setConfirm({ kind: "delete" })}
      >
        <Trash class="icon" aria-hidden="true" />
      </button>
      <Show when={mergeMenu()}>
        {(anchor) => (
          <Menu
            anchor={anchor()}
            label={`Merge ${props.category.name} into`}
            inside={() => [mergeButton]}
            onClose={(returnFocus) => {
              setMergeMenu(null);
              if (returnFocus) mergeButton?.focus();
            }}
            entries={[
              { kind: "heading", label: "Merge into" },
              ...others().map((into) => ({
                label: into.name,
                hint: String(into.gameCount),
                onSelect: () => setConfirm({ kind: "merge", into }),
              })),
            ]}
          />
        )}
      </Show>
      <Show when={confirm()?.kind === "delete"}>
        <div class="category-list__confirm" role="alert">
          <span>
            Delete “{props.category.name}”? {games()} lose it; the games stay.
            <Show when={props.category.sources.length > 0}>
              {" "}
              Synced store categories bring it back on the next sync.
            </Show>
          </span>
          <button type="button" class="btn btn--small" onClick={() => setConfirm(null)}>
            Cancel
          </button>
          <button
            type="button"
            class="btn btn--small btn--danger-solid"
            onClick={() => {
              setConfirm(null);
              void store.deleteCategory(props.category.id);
            }}
          >
            Delete
          </button>
        </div>
      </Show>
      <Show when={mergeInto()}>
        {(into) => (
          <div class="category-list__confirm category-list__confirm--merge" role="alert">
            <span>
              Merge “{props.category.name}” into “{into().name}”? Its games and store categories
              move over, and “{props.category.name}” is removed.
            </span>
            <button type="button" class="btn btn--small" onClick={() => setConfirm(null)}>
              Cancel
            </button>
            <button
              type="button"
              class="btn btn--small btn--primary"
              onClick={() => {
                const target = into().id;
                setConfirm(null);
                void store.mergeCategories(props.category.id, target);
              }}
            >
              Merge
            </button>
          </div>
        )}
      </Show>
    </li>
  );
}
