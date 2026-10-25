import ArrowDownWideNarrow from "lucide-solid/icons/arrow-down-wide-narrow";
import ArrowUpNarrowWide from "lucide-solid/icons/arrow-up-narrow-wide";
import BookmarkPlus from "lucide-solid/icons/bookmark-plus";
import Funnel from "lucide-solid/icons/funnel";
import LayoutGrid from "lucide-solid/icons/layout-grid";
import List from "lucide-solid/icons/list";
import PanelLeft from "lucide-solid/icons/panel-left";
import Undo2 from "lucide-solid/icons/undo-2";
import { createSignal, For, Show } from "solid-js";
import type { SortField } from "../bindings/SortField";
import { store } from "../lib/store";
import { activeFilterCount } from "../lib/views";
import type { ViewMode } from "../lib/viewMode";
import { FilterPanel } from "./FilterPanel";
import { KeywordFilter, type KeywordFilterHandle } from "./KeywordFilter";
import { Menu } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { SaveViewDialog } from "./SaveViewDialog";

const SORTS: { id: SortField; label: string }[] = [
  { id: "name", label: "Name" },
  { id: "lastPlayed", label: "Last played" },
  { id: "playtime", label: "Playtime" },
  { id: "dateAdded", label: "Date added" },
];

const LAYOUTS: { id: ViewMode; label: string; icon: typeof List }[] = [
  { id: "grid", label: "Grid view", icon: LayoutGrid },
  { id: "list", label: "List view", icon: List },
  { id: "details", label: "Details view", icon: PanelLeft },
];

type Open = "filters" | "sort" | "save" | null;

const filters = () => activeFilterCount(store.query());
const sortName = () => SORTS.find((s) => s.id === store.query().sort)?.label ?? "";
const sortLabel = () =>
  `Sort: ${sortName()}, ${store.query().descending ? "descending" : "ascending"}`;
const viewName = () => store.activeView()?.name ?? "view";
const changed = () => {
  const view = store.activeView();
  return view !== null && store.hasDraft(view.id);
};
const noun = (n: number) => (n === 1 ? "game" : "games");
/** "N games", or "N of M games" while the keyword filter narrows the view. */
const count = () => {
  const shown = store.games().length;
  const total = store.viewCounts()[store.activeViewId() ?? ""];
  if (store.keyword() !== null && total !== undefined) return { shown, total, noun: noun(total) };
  return { shown, total: null, noun: noun(shown) };
};

/** The library toolbar: keyword filter, filters, sort, the open view's save actions, layout. */
export function TopBar(props: { keywordRef?: (handle: KeywordFilterHandle) => void }) {
  const [open, setOpen] = createSignal<Open>(null);
  const [anchor, setAnchor] = createSignal<PopoverAnchor | null>(null);
  const [saving, setSaving] = createSignal(false);
  const buttons: Partial<Record<Exclude<Open, null>, HTMLButtonElement>> = {};

  function toggle(which: Exclude<Open, null>, align: "start" | "end") {
    if (open() === which) {
      setOpen(null);
      return;
    }
    const rect = buttons[which]?.getBoundingClientRect();
    if (!rect) return;
    setAnchor({ rect, align });
    setOpen(which);
  }

  function close(returnFocus: boolean) {
    const which = open();
    setOpen(null);
    if (returnFocus && which) buttons[which]?.focus();
  }

  return (
    <header class="topbar">
      <KeywordFilter ref={props.keywordRef} />
      <button
        ref={(el) => (buttons.filters = el)}
        type="button"
        class="tool-btn"
        classList={{ "tool-btn--open": open() === "filters", "tool-btn--wide": filters() > 0 }}
        aria-label={filters() > 0 ? `Filters (${filters()} active)` : "Filters"}
        title="Filters"
        aria-haspopup="dialog"
        aria-expanded={open() === "filters"}
        onClick={() => toggle("filters", "start")}
      >
        <Funnel class="icon" aria-hidden="true" />
        <Show when={filters() > 0}>
          <span class="tool-btn__badge" aria-hidden="true">
            {filters()}
          </span>
        </Show>
      </button>
      <button
        ref={(el) => (buttons.sort = el)}
        type="button"
        class="tool-btn"
        classList={{ "tool-btn--open": open() === "sort" }}
        aria-label={sortLabel()}
        title={sortLabel()}
        aria-haspopup="menu"
        aria-expanded={open() === "sort"}
        onClick={() => toggle("sort", "start")}
      >
        <Show
          when={store.query().descending}
          fallback={<ArrowUpNarrowWide class="icon" aria-hidden="true" />}
        >
          <ArrowDownWideNarrow class="icon" aria-hidden="true" />
        </Show>
      </button>
      <Show when={changed()}>
        <div class="view-actions" role="group" aria-label={`Unsaved changes to ${viewName()}`}>
          <span class="view-actions__label">
            <span class="view-actions__dot" aria-hidden="true" />
            Unsaved
          </span>
          <button
            type="button"
            class="view-actions__btn"
            aria-label={`Reset to the saved ${viewName()}`}
            title="Reset to the saved view"
            onClick={() => store.resetView()}
          >
            <Undo2 class="icon" aria-hidden="true" />
          </button>
          <button
            ref={(el) => (buttons.save = el)}
            type="button"
            class="view-actions__btn"
            classList={{ "view-actions__btn--open": open() === "save" }}
            aria-label="Save view"
            title="Save view"
            aria-haspopup="menu"
            aria-expanded={open() === "save"}
            onClick={() => toggle("save", "start")}
          >
            <BookmarkPlus class="icon" aria-hidden="true" />
          </button>
        </div>
      </Show>
      <div class="topbar__spacer" />
      <span class="topbar__count" aria-live="polite">
        <Show when={count().total !== null} fallback={`${count().shown} ${count().noun}`}>
          <strong>{count().shown}</strong> of {count().total} {count().noun}
        </Show>
      </span>
      <div class="view-toggle" role="group" aria-label="Layout">
        <For each={LAYOUTS}>
          {(layout) => (
            <button
              type="button"
              class="icon-btn icon-btn--bordered"
              classList={{ "icon-btn--active": store.viewMode() === layout.id }}
              aria-label={layout.label}
              aria-pressed={store.viewMode() === layout.id}
              title={layout.label}
              onClick={() => store.setViewMode(layout.id)}
            >
              <layout.icon class="icon" aria-hidden="true" />
            </button>
          )}
        </For>
      </div>
      <Show when={open() === "filters" && anchor()}>
        {(at) => <FilterPanel anchor={at()} onClose={close} inside={() => [buttons.filters]} />}
      </Show>
      <Show when={open() === "sort" && anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label="Sort"
            inside={() => [buttons.sort]}
            onClose={close}
            entries={[
              {
                label: "Ascending",
                checked: !store.query().descending,
                keepOpen: true,
                onSelect: () => store.setDescending(false),
              },
              {
                label: "Descending",
                checked: store.query().descending,
                keepOpen: true,
                onSelect: () => store.setDescending(true),
              },
              { kind: "separator" },
              ...SORTS.map((sort) => ({
                label: sort.label,
                checked: store.query().sort === sort.id,
                keepOpen: true,
                onSelect: () => store.setSortField(sort.id),
              })),
            ]}
          />
        )}
      </Show>
      <Show when={open() === "save" && anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label="Save view"
            inside={() => [buttons.save]}
            onClose={close}
            entries={[
              {
                label: `Update “${viewName()}”`,
                onSelect: () => void store.updateActiveView(),
              },
              { label: "Save as new view…", onSelect: () => setSaving(true) },
            ]}
          />
        )}
      </Show>
      <Show when={saving()}>
        <SaveViewDialog onClose={() => setSaving(false)} />
      </Show>
    </header>
  );
}
