import { For, onMount, Show, type JSX } from "solid-js";
import type { GameQuery } from "../bindings/GameQuery";
import type { HiddenFilter } from "../bindings/HiddenFilter";
import type { KindFilter } from "../bindings/KindFilter";
import type { PlayedFilter } from "../bindings/PlayedFilter";
import type { TriState } from "../bindings/TriState";
import { translator } from "../i18n";
import { libraryCommon } from "../i18n/libraryCommon";
import { libraryFilterPanel } from "../i18n/libraryFilterPanel";
import { libraryFilters } from "../i18n/libraryFilters";
import { toggleSource } from "../lib/query";
import { store } from "../lib/store";
import { activeFilterCount, clearedFilters, NO_CATEGORY } from "../lib/views";
import { FilterTree } from "./FilterTree";
import { Popover, type PopoverAnchor } from "./Popover";
import { SourceIcon } from "./SourceIcon";

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
          <span class="filters__label" id="filter-stores">
            {trFilters("source")}
          </span>
          <div class="filters__stores" role="group" aria-labelledby="filter-stores">
            <For each={store.sources()}>
              {(source) => {
                const on = () => query().sources.includes(source.id);
                return (
                  <button
                    type="button"
                    class="chip"
                    classList={{ "chip--on": on() }}
                    aria-pressed={on()}
                    onClick={() => set({ sources: toggleSource(query().sources, source.id) })}
                  >
                    <SourceIcon source={source.id} />
                    {source.name}
                  </button>
                );
              }}
            </For>
          </div>
          <span class="filters__label" id="filter-categories">
            {trFilters("category")}
          </span>
          <Show
            when={store.categories().length > 0}
            fallback={<p class="filters__hint">{tr("noCategories")}</p>}
          >
            <div class="filters__stores" role="group" aria-labelledby="filter-categories">
              <For
                each={[...store.categories(), { id: NO_CATEGORY, name: trCommon("noCategory") }]}
              >
                {(category) => {
                  const on = () => query().categories.includes(category.id);
                  return (
                    <button
                      type="button"
                      class="chip"
                      classList={{ "chip--on": on(), "chip--none": category.id === NO_CATEGORY }}
                      aria-pressed={on()}
                      onClick={() =>
                        set({ categories: toggleSource(query().categories, category.id) })
                      }
                    >
                      {category.name}
                    </button>
                  );
                }}
              </For>
            </div>
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
