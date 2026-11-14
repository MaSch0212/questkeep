import GripVertical from "lucide-solid/icons/grip-vertical";
import Merge from "lucide-solid/icons/merge";
import Pencil from "lucide-solid/icons/pencil";
import Trash from "lucide-solid/icons/trash";
import Unlink from "lucide-solid/icons/unlink";
import WandSparkles from "lucide-solid/icons/wand-sparkles";
import { createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import type { Category } from "../../bindings/Category";
import { translator } from "../../i18n";
import { settingsLibrary } from "../../i18n/settingsLibrary";
import { dropIndexAt, moveBefore, nudge } from "../../lib/categoryOrder";
import { store } from "../../lib/store";
import { CategoryFilterDialog } from "../CategoryFilterDialog";
import { Menu } from "../Menu";
import type { PopoverAnchor } from "../Popover";
import { SourceIcon, sourceName } from "../SourceIcon";

const tr = translator(settingsLibrary);

const sameName = (a: string, b: string) =>
  a.localeCompare(b, undefined, { sensitivity: "base" }) === 0;

/**
 * Adds, orders, renames, merges and deletes categories (new ones can also be created on a game's
 * page). Rows
 * move by dragging their handle or with Alt+Up/Down. Each row shows the store categories that
 * sync into it.
 */
export function CategoriesSection() {
  const [dragged, setDragged] = createSignal<string | null>(null);
  const [drop, setDrop] = createSignal<number | null>(null);
  const [adding, setAdding] = createSignal("");
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

  async function add(event: SubmitEvent) {
    event.preventDefault();
    const name = adding().trim();
    if (name === "") return;
    if (store.categories().some((c) => sameName(c.name, name))) {
      store.pushToast("error", tr("categoryExists", { name }));
      return;
    }
    if (await store.createCategory(name)) setAdding("");
  }

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
      <p class="muted">{tr("categoriesIntro")}</p>
      <Show when={ids().length > 0} fallback={<p class="muted">{tr("noCategories")}</p>}>
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
      <form class="field__row status-add" onSubmit={(event) => void add(event)}>
        <input
          class="input"
          placeholder={tr("newCategoryPlaceholder")}
          aria-label={tr("newCategoryAria")}
          value={adding()}
          onInput={(event) => setAdding(event.currentTarget.value)}
        />
        <button type="submit" class="btn" disabled={adding().trim() === ""}>
          {tr("addCategory")}
        </button>
      </form>
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
  const [filtering, setFiltering] = createSignal(false);
  let mergeButton: HTMLButtonElement | undefined;
  const games = () => tr("games", { count: props.category.gameCount });
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
        data-tooltip={tr("dragTitle")}
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
          aria-label={tr("categoryName")}
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
                data-tooltip={tr("syncedWith", {
                  store: sourceName(source.source),
                  name: source.name,
                })}
              >
                <SourceIcon source={source.source} />
                <Show when={!own()}>
                  <span class="category-list__source-name">{source.name}</span>
                  <button
                    type="button"
                    class="category-list__separate"
                    aria-label={tr("separate", {
                      store: sourceName(source.source),
                      name: source.name,
                    })}
                    data-tooltip={tr("separateTitle")}
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
      <Show when={props.category.autoFilter}>
        <span class="category-list__auto" data-tooltip={tr("autoBadgeTitle")}>
          <WandSparkles class="icon" aria-hidden="true" />
          {tr("autoBadge")}
        </span>
      </Show>
      <span class="category-list__count">{games()}</span>
      <button
        type="button"
        class="icon-btn icon-btn--small"
        classList={{ "icon-btn--on": props.category.autoFilter !== null }}
        aria-label={tr("autoAria", { name: props.category.name })}
        data-tooltip={tr("autoButtonTitle")}
        aria-haspopup="dialog"
        onClick={() => setFiltering(true)}
      >
        <WandSparkles class="icon" aria-hidden="true" />
      </button>
      <button
        ref={(el) => (mergeButton = el)}
        type="button"
        class="icon-btn icon-btn--small"
        aria-label={tr("mergeAria", { name: props.category.name })}
        data-tooltip={tr("mergeTitle")}
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
        aria-label={tr("rename", { name: props.category.name })}
        data-tooltip={tr("renameTitle")}
        onClick={() => setEditing(true)}
      >
        <Pencil class="icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="icon-btn icon-btn--small category-list__delete"
        aria-label={tr("delete", { name: props.category.name })}
        data-tooltip={tr("deleteTitle")}
        onClick={() => setConfirm({ kind: "delete" })}
      >
        <Trash class="icon" aria-hidden="true" />
      </button>
      <Show when={mergeMenu()}>
        {(anchor) => (
          <Menu
            anchor={anchor()}
            label={tr("mergeMenu", { name: props.category.name })}
            inside={() => [mergeButton]}
            onClose={(returnFocus) => {
              setMergeMenu(null);
              if (returnFocus) mergeButton?.focus();
            }}
            entries={[
              { kind: "heading", label: tr("mergeHeading") },
              ...others().map((into) => ({
                label: into.name,
                hint: String(into.gameCount),
                onSelect: () => setConfirm({ kind: "merge", into }),
              })),
            ]}
          />
        )}
      </Show>
      <Show when={filtering()}>
        <CategoryFilterDialog category={props.category} onClose={() => setFiltering(false)} />
      </Show>
      <Show when={confirm()?.kind === "delete"}>
        <div class="category-list__confirm" role="alert">
          <span>
            {tr("deleteConfirm", { name: props.category.name, count: props.category.gameCount })}
            <Show when={props.category.sources.length > 0}> {tr("deleteSynced")}</Show>
          </span>
          <button type="button" class="btn btn--small" onClick={() => setConfirm(null)}>
            {tr("cancel")}
          </button>
          <button
            type="button"
            class="btn btn--small btn--danger-solid"
            onClick={() => {
              setConfirm(null);
              void store.deleteCategory(props.category.id);
            }}
          >
            {tr("deleteButton")}
          </button>
        </div>
      </Show>
      <Show when={mergeInto()}>
        {(into) => (
          <div class="category-list__confirm category-list__confirm--merge" role="alert">
            <span>{tr("mergeConfirm", { name: props.category.name, into: into().name })}</span>
            <button type="button" class="btn btn--small" onClick={() => setConfirm(null)}>
              {tr("cancel")}
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
              {tr("mergeButton")}
            </button>
          </div>
        )}
      </Show>
    </li>
  );
}
