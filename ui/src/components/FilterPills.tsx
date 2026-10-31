import X from "lucide-solid/icons/x";
import { createEffect, For, Show } from "solid-js";
import type { GameQuery } from "../bindings/GameQuery";
import type { ListField } from "../bindings/ListField";
import { emptyGroup, removeAt, treePills, type PillPart } from "../lib/filters";
import { store } from "../lib/store";
import { clearedFilters, NO_CATEGORY } from "../lib/views";

interface QuickPill {
  parts: PillPart[];
  clear: Partial<GameQuery>;
}

/** The quick filters that are set, each as a pill. */
function quickPills(query: GameQuery): QuickPill[] {
  const pills: QuickPill[] = [];
  if (query.sources.length > 0) {
    const names = query.sources.map((id) => store.valueName("source", id));
    pills.push({ parts: [{ label: "Store", value: names.join(", ") }], clear: { sources: [] } });
  }
  if (query.categories.length > 0) {
    const names = query.categories.map((id) =>
      id === NO_CATEGORY ? "No category" : store.valueName("category", id),
    );
    pills.push({
      parts: [{ label: "Category", value: names.join(", ") }],
      clear: { categories: [] },
    });
  }
  if (query.installed !== "any") {
    pills.push({
      parts: [{ label: query.installed === "yes" ? "Installed" : "Not installed", value: "" }],
      clear: { installed: "any" },
    });
  }
  if (query.favorite !== "any") {
    pills.push({
      parts: [{ label: query.favorite === "yes" ? "Favorite" : "Not favorite", value: "" }],
      clear: { favorite: "any" },
    });
  }
  if (query.played !== "any") {
    pills.push({
      parts: [{ label: query.played === "played" ? "Played" : "Never played", value: "" }],
      clear: { played: "any" },
    });
  }
  if (query.hidden !== "visible") {
    pills.push({
      parts: [{ label: query.hidden === "all" ? "Hidden included" : "Only hidden", value: "" }],
      clear: { hidden: "visible" },
    });
  }
  return pills;
}

function PillText(props: { parts: PillPart[]; joiner: string; grouped: boolean }) {
  return (
    <span class="pill__text">
      <Show when={props.grouped}>( </Show>
      <For each={props.parts}>
        {(part, index) => (
          <>
            <Show when={index() > 0}>
              <i class="pill__joiner"> {props.joiner} </i>
            </Show>
            {part.label}
            <Show when={part.value}>
              : <b>{part.value}</b>
            </Show>
          </>
        )}
      </For>
      <Show when={props.grouped}> )</Show>
    </span>
  );
}

const describe = (parts: PillPart[]) =>
  parts.map((p) => (p.value ? `${p.label}: ${p.value}` : p.label)).join(", ");

const quick = () => quickPills(store.query());
const tree = () => treePills(store.query().filter, store.valueName);

/** The open view's active filters under the toolbar, each removable, then "Clear all". */
export function FilterPills() {
  // Names of list values come from the loaded value lists.
  createEffect(() => {
    const fields = new Set<ListField>();
    const walk = (items: GameQuery["filter"]["items"]) => {
      for (const node of items) {
        if (node.kind === "group") walk(node.items);
        else if (node.kind === "list" && node.values.length > 0) fields.add(node.field);
      }
    };
    walk(store.query().filter.items);
    for (const field of fields) {
      if (store.valueLists()[field] === undefined) void store.loadValues(field);
    }
  });

  return (
    <Show when={quick().length + tree().length > 0}>
      <div class="filter-pills" role="list" aria-label="Active filters">
        <For each={quick()}>
          {(pill) => (
            <span class="pill" role="listitem">
              <PillText parts={pill.parts} joiner="and" grouped={false} />
              <button
                type="button"
                class="pill__remove"
                aria-label={`Remove filter ${describe(pill.parts)}`}
                onClick={() => store.changeQuery(pill.clear)}
              >
                <X class="icon" aria-hidden="true" />
              </button>
            </span>
          )}
        </For>
        <For each={tree()}>
          {(pill) => (
            <span class="pill" role="listitem">
              <PillText parts={pill.parts} joiner={pill.joiner} grouped={pill.grouped} />
              <button
                type="button"
                class="pill__remove"
                aria-label={`Remove filter ${describe(pill.parts)}`}
                onClick={() =>
                  store.changeQuery({ filter: removeAt(store.query().filter, [pill.index]) })
                }
              >
                <X class="icon" aria-hidden="true" />
              </button>
            </span>
          )}
        </For>
        <button
          type="button"
          class="pill pill--clear"
          onClick={() => {
            const cleared = clearedFilters(store.query());
            store.changeQuery({
              sources: cleared.sources,
              categories: cleared.categories,
              installed: cleared.installed,
              favorite: cleared.favorite,
              played: cleared.played,
              hidden: cleared.hidden,
              filter: emptyGroup(),
            });
          }}
        >
          Clear all
        </button>
      </div>
    </Show>
  );
}
