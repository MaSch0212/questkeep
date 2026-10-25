import Check from "lucide-solid/icons/check";
import Plus from "lucide-solid/icons/plus";
import Tag from "lucide-solid/icons/tag";
import X from "lucide-solid/icons/x";
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import type { Category } from "../bindings/Category";
import type { Game } from "../bindings/Game";
import { store } from "../lib/store";
import { Popover, type PopoverAnchor } from "./Popover";
import { sourceName } from "./SourceIcon";

const optionId = (index: number) => `category-option-${index}`;
const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

/** A game's categories as removable chips, plus a picker that finds or creates categories. */
export function CategoryEditor(props: { game: Game }) {
  const [picker, setPicker] = createSignal<PopoverAnchor | null>(null);
  let addButton: HTMLButtonElement | undefined;

  const assigned = createMemo(() =>
    store
      .categories()
      .filter((c) => props.game.categories.includes(c.id))
      .toSorted((a, b) => collator.compare(a.name, b.name)),
  );

  function save(ids: string[]) {
    void store.setGameCategories(props.game.id, ids);
  }

  function toggle(id: string) {
    const ids = props.game.categories;
    save(ids.includes(id) ? ids.filter((c) => c !== id) : [...ids, id]);
  }

  async function create(name: string) {
    const category = await store.createCategory(name);
    if (category && !props.game.categories.includes(category.id)) {
      save([...props.game.categories, category.id]);
    }
  }

  return (
    <section class="categories" aria-label="Categories">
      <ul class="categories__list">
        <For each={assigned()}>
          {(category) => (
            <li class="categories__chip" title={importedTitle(category)}>
              <Tag class="icon" aria-hidden="true" />
              {category.name}
              <button
                type="button"
                class="categories__remove"
                aria-label={`Remove ${category.name}`}
                title="Remove"
                onClick={() => toggle(category.id)}
              >
                <X class="icon" aria-hidden="true" />
              </button>
            </li>
          )}
        </For>
        <li>
          <button
            ref={(el) => (addButton = el)}
            type="button"
            class="categories__add"
            aria-haspopup="dialog"
            aria-expanded={picker() !== null}
            onClick={() => {
              const rect = addButton?.getBoundingClientRect();
              if (rect) setPicker(picker() ? null : { rect, align: "start" });
            }}
          >
            <Plus class="icon" aria-hidden="true" />
            {assigned().length === 0 ? "Add a category" : "Category"}
          </button>
        </li>
      </ul>
      <Show when={picker()}>
        {(anchor) => (
          <CategoryPicker
            anchor={anchor()}
            selected={props.game.categories}
            inside={() => [addButton]}
            onToggle={toggle}
            onCreate={(name) => void create(name)}
            onClose={(returnFocus) => {
              setPicker(null);
              if (returnFocus) addButton?.focus();
            }}
          />
        )}
      </Show>
    </section>
  );
}

function importedTitle(category: Category): string | undefined {
  return category.source ? `Imported from ${sourceName(category.source)}` : undefined;
}

/**
 * Type to find a category; arrows and Enter toggle the highlighted one, Enter on a new name
 * creates it. Stays open for several changes.
 */
function CategoryPicker(props: {
  anchor: PopoverAnchor;
  selected: readonly string[];
  inside: () => (Element | undefined)[];
  onToggle: (id: string) => void;
  onCreate: (name: string) => void;
  onClose: (returnFocus: boolean) => void;
}) {
  const [text, setText] = createSignal("");
  const [active, setActive] = createSignal(0);
  let input: HTMLInputElement | undefined;
  onMount(() => queueMicrotask(() => input?.focus()));

  const term = () => text().trim();
  const matches = createMemo(() => {
    const needle = term().toLocaleLowerCase();
    return store.categories().filter((c) => c.name.toLocaleLowerCase().includes(needle));
  });
  const canCreate = () =>
    term() !== "" && !store.categories().some((c) => collator.compare(c.name, term()) === 0);
  // Options: the matches, then "Create" when the name is new.
  const count = () => matches().length + (canCreate() ? 1 : 0);

  function choose(index: number) {
    const category = matches()[index];
    if (category) props.onToggle(category.id);
    else if (canCreate()) {
      props.onCreate(term());
      setText("");
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const n = count();
      if (n > 0) setActive((i) => (i + (event.key === "ArrowDown" ? 1 : n - 1)) % n);
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(Math.min(active(), count() - 1));
    }
  }

  return (
    <Popover
      anchor={props.anchor}
      class="category-picker"
      role="dialog"
      label="Categories"
      inside={props.inside}
      onClose={props.onClose}
    >
      <input
        ref={(el) => (input = el)}
        class="input category-picker__input"
        type="text"
        role="combobox"
        aria-label="Find or create a category"
        aria-expanded="true"
        aria-controls="category-options"
        aria-activedescendant={count() > 0 ? optionId(active()) : undefined}
        placeholder="Find or create a category…"
        spellcheck={false}
        autocomplete="off"
        value={text()}
        onInput={(event) => {
          setText(event.currentTarget.value);
          setActive(0);
        }}
        onKeyDown={onKeyDown}
      />
      <ul
        id="category-options"
        class="category-picker__list"
        role="listbox"
        aria-label="Categories"
      >
        <For each={matches()}>
          {(category, index) => {
            const on = () => props.selected.includes(category.id);
            return (
              <li
                id={optionId(index())}
                role="option"
                aria-selected={on()}
                class="category-picker__option"
                classList={{ "category-picker__option--active": index() === active() }}
                onPointerMove={() => setActive(index())}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => choose(index())}
              >
                <span class="category-picker__check">
                  <Show when={on()}>
                    <Check class="icon" aria-hidden="true" />
                  </Show>
                </span>
                <span class="category-picker__name">{category.name}</span>
                <span class="category-picker__count">{category.gameCount}</span>
              </li>
            );
          }}
        </For>
        <Show when={canCreate()}>
          <li
            id={optionId(matches().length)}
            role="option"
            aria-selected={false}
            class="category-picker__option"
            classList={{ "category-picker__option--active": active() === matches().length }}
            onPointerMove={() => setActive(matches().length)}
            onPointerDown={(event) => event.preventDefault()}
            onClick={() => choose(matches().length)}
          >
            <span class="category-picker__check">
              <Plus class="icon" aria-hidden="true" />
            </span>
            <span class="category-picker__name">Create “{term()}”</span>
          </li>
        </Show>
      </ul>
      <Show when={count() === 0}>
        <p class="category-picker__empty">Type a name to create your first category.</p>
      </Show>
    </Popover>
  );
}
