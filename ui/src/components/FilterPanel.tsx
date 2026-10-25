import { For, onMount, type JSX } from "solid-js";
import type { GameQuery } from "../bindings/GameQuery";
import type { HiddenFilter } from "../bindings/HiddenFilter";
import type { PlayedFilter } from "../bindings/PlayedFilter";
import type { TriState } from "../bindings/TriState";
import { toggleSource } from "../lib/query";
import { store } from "../lib/store";
import { clearedFilters } from "../lib/views";
import { Popover, type PopoverAnchor } from "./Popover";
import { SourceIcon } from "./SourceIcon";

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

const YES_NO: readonly (readonly [TriState, string])[] = [
  ["any", "Any"],
  ["yes", "Yes"],
  ["no", "No"],
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
      label="Filters"
      ref={(el) => (panel = el)}
      inside={props.inside}
      onClose={props.onClose}
    >
      <h2 class="filters__title">Filters</h2>
      <div class="filters__grid">
        <span class="filters__label" id="filter-stores">
          Store
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
        <Choice
          label="Installed"
          value={query().installed}
          options={YES_NO}
          onChange={(installed) => set({ installed })}
        />
        <Choice
          label="Favorite"
          value={query().favorite}
          options={YES_NO}
          onChange={(favorite) => set({ favorite })}
        />
        <Choice<PlayedFilter>
          label="Played"
          value={query().played}
          options={[
            ["any", "Any"],
            ["played", "Played"],
            ["never", "Never"],
          ]}
          onChange={(played) => set({ played })}
        />
        <Choice<HiddenFilter>
          label="Hidden games"
          value={query().hidden}
          options={[
            ["visible", "Leave out"],
            ["all", "Include"],
            ["hidden", "Only"],
          ]}
          onChange={(hidden) => set({ hidden })}
        />
      </div>
      <div class="filters__footer">
        <button
          type="button"
          class="btn btn--ghost"
          onClick={() => {
            const cleared = clearedFilters(query());
            set({
              sources: cleared.sources,
              favorite: cleared.favorite,
              installed: cleared.installed,
              played: cleared.played,
              hidden: cleared.hidden,
            });
          }}
        >
          Clear filters
        </button>
        <button type="button" class="btn" onClick={() => props.onClose(true)}>
          Done
        </button>
      </div>
    </Popover>
  );
}
