import ChevronDown from "lucide-solid/icons/chevron-down";
import { createSignal, For, onMount, Show, type JSX } from "solid-js";
import type { GameQuery } from "../bindings/GameQuery";
import type { HiddenFilter } from "../bindings/HiddenFilter";
import type { KindFilter } from "../bindings/KindFilter";
import type { PlayedFilter } from "../bindings/PlayedFilter";
import type { TriState } from "../bindings/TriState";
import { translator } from "../i18n";
import { libraryCommon } from "../i18n/libraryCommon";
import { libraryFilterPanel } from "../i18n/libraryFilterPanel";
import { libraryFilters } from "../i18n/libraryFilters";
import { pickedNames } from "../lib/filters";
import { toggleSource } from "../lib/query";
import { store } from "../lib/store";
import { activeFilterCount, clearedFilters, NO_CATEGORY } from "../lib/views";
import { FilterTree } from "./FilterTree";
import { Popover, type PopoverAnchor } from "./Popover";
import { SearchPicker, type PickerItem } from "./SearchPicker";

const tr = translator(libraryFilterPanel);
const trCommon = translator(libraryCommon);
const trFilters = translator(libraryFilters);

/** A row of mutually exclusive choices (radio buttons styled as a segmented control). */
function Choice<T extends string>(props: {
  label: string;
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (value: T) => void;
}) {
  return (
    <>
      <span class="filters__label" id={`filter-${props.label}`}>
        {props.label}
      </span>
      <div class="segmented" role="radiogroup" aria-labelledby={`filter-${props.label}`}>
        <For each={props.options}>
          {([value, text]) => (
            <button
              type="button"
              role="radio"
              class="segmented__option"
              classList={{ "segmented__option--on": props.value === value }}
              aria-checked={props.value === value}
              onClick={() => props.onChange(value)}
            >
              {text}
            </button>
          )}
        </For>
      </div>
    </>
  );
}

/**
 * A row whose values are picked from a searchable list (stores, categories): one line however many
 * there are, the button names what is picked.
 */
function PickRow(props: {
  id: string;
  label: string;
  choose: string;
  placeholder: string;
  items: PickerItem[];
  onPick: (id: string) => void;
}) {
  const [anchor, setAnchor] = createSignal<PopoverAnchor | null>(null);
  let button: HTMLButtonElement | undefined;
  const picked = () => props.items.filter((item) => item.checked).map((item) => item.label);
  const text = () => {
    const { shown, more } = pickedNames(picked());
    if (shown === "") return tr("any");
    return more > 0 ? tr("andMore", { names: shown, count: more }) : shown;
  };

  return (
    <>
      <span class="filters__label" id={`filter-${props.id}`}>
        {props.label}
      </span>
      <button
        ref={(el) => (button = el)}
        type="button"
        class="filters__pick"
        classList={{ "filters__pick--on": picked().length > 0 }}
        aria-labelledby={`filter-${props.id} filter-${props.id}-value`}
        aria-haspopup="dialog"
        aria-expanded={anchor() !== null}
        onClick={() => {
          const rect = button?.getBoundingClientRect();
          setAnchor(anchor() || !rect ? null : { rect, align: "start" });
        }}
      >
        <span class="filters__pick-text" id={`filter-${props.id}-value`}>
          {text()}
        </span>
        <ChevronDown class="icon" aria-hidden="true" />
      </button>
      <Show when={anchor()}>
        {(at) => (
          <SearchPicker
            anchor={at()}
            label={props.choose}
            placeholder={props.placeholder}
            keepOpen
            items={props.items}
            onPick={props.onPick}
            onClose={(returnFocus) => {
              setAnchor(null);
              if (returnFocus) button?.focus();
            }}
          />
        )}
      </Show>
    </>
  );
}

const yesNo = (): readonly (readonly [TriState, string])[] => [
  ["any", tr("any")],
  ["yes", tr("yes")],
  ["no", tr("no")],
];

const query = () => store.query();
const set = (patch: Partial<GameQuery>) => store.changeQuery(patch);

/** The open view's filters. Changes apply at once and are kept as the view's draft. */
export function FilterPanel(props: {
  anchor: PopoverAnchor;
  onClose: (returnFocus: boolean) => void;
  inside: () => (Element | undefined)[];
}): JSX.Element {
  let panel: HTMLDivElement | undefined;

  onMount(() => queueMicrotask(() => panel?.querySelector<HTMLElement>("button")?.focus()));

  return (
    <Popover
      anchor={props.anchor}
      class="filters"
      role="dialog"
      label={tr("title")}
      ref={(el) => (panel = el)}
      inside={props.inside}
      onClose={props.onClose}
    >
      <div class="filters__head">
        <h2 class="filters__title">{tr("title")}</h2>
        <span class="filters__summary">
          {tr("summary", {
            active: activeFilterCount(query()),
            games: trCommon("gamesCount", { count: store.games().length }),
          })}
        </span>
      </div>
      <div class="filters__body">
        <div class="filters__grid">
          <PickRow
            id="stores"
            label={trFilters("source")}
            choose={tr("chooseStores")}
            placeholder={tr("searchStores")}
            items={store.sources().map((source) => ({
              id: source.id,
              label: source.name,
              checked: query().sources.includes(source.id),
            }))}
            onPick={(id) => set({ sources: toggleSource(query().sources, id) })}
          />
          <Show
            when={store.categories().length > 0}
            fallback={
              <>
                <span class="filters__label">{trFilters("category")}</span>
                <p class="filters__hint">{tr("noCategories")}</p>
              </>
            }
          >
            <PickRow
              id="categories"
              label={trFilters("category")}
              choose={tr("chooseCategories")}
              placeholder={tr("searchCategories")}
              items={[
                ...store.categories().map((category) => ({
                  id: category.id,
                  label: category.name,
                  checked: query().categories.includes(category.id),
                })),
                {
                  id: NO_CATEGORY,
                  label: trCommon("noCategory"),
                  special: true,
                  checked: query().categories.includes(NO_CATEGORY),
                },
              ]}
              onPick={(id) => set({ categories: toggleSource(query().categories, id) })}
            />
          </Show>
          <Choice<KindFilter>
            label={tr("type")}
            value={query().kind}
            options={[
              ["games", tr("games")],
              ["apps", tr("apps")],
              ["both", tr("both")],
            ]}
            onChange={(kind) => set({ kind })}
          />
          <Choice
            label={tr("installed")}
            value={query().installed}
            options={yesNo()}
            onChange={(installed) => set({ installed })}
          />
          <Choice
            label={tr("favorite")}
            value={query().favorite}
            options={yesNo()}
            onChange={(favorite) => set({ favorite })}
          />
          <Choice<PlayedFilter>
            label={tr("played")}
            value={query().played}
            options={[
              ["any", tr("any")],
              ["played", tr("played")],
              ["never", tr("never")],
            ]}
            onChange={(played) => set({ played })}
          />
          <Choice<HiddenFilter>
            label={tr("hiddenGames")}
            value={query().hidden}
            options={[
              ["visible", tr("leaveOut")],
              ["all", tr("include")],
              ["hidden", tr("only")],
            ]}
            onChange={(hidden) => set({ hidden })}
          />
        </div>
        <FilterTree tree={query().filter} onChange={(filter) => set({ filter })} />
      </div>
      <div class="filters__footer">
        <button
          type="button"
          class="btn btn--ghost"
          onClick={() => {
            const {
              sort: _sort,
              descending: _descending,
              search: _search,
              ...cleared
            } = clearedFilters(query());
            set(cleared);
          }}
        >
          {tr("clearFilters")}
        </button>
        <button type="button" class="btn" onClick={() => props.onClose(true)}>
          {tr("done")}
        </button>
      </div>
    </Popover>
  );
}
