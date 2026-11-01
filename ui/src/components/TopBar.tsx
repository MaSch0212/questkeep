import ArrowDownWideNarrow from "lucide-solid/icons/arrow-down-wide-narrow";
import ArrowUpNarrowWide from "lucide-solid/icons/arrow-up-narrow-wide";
import BookmarkPlus from "lucide-solid/icons/bookmark-plus";
import Funnel from "lucide-solid/icons/funnel";
import Group from "lucide-solid/icons/group";
import LayoutGrid from "lucide-solid/icons/layout-grid";
import List from "lucide-solid/icons/list";
import ListTree from "lucide-solid/icons/list-tree";
import PanelLeft from "lucide-solid/icons/panel-left";
import Undo2 from "lucide-solid/icons/undo-2";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { GroupBy } from "../bindings/GroupBy";
import type { SortField } from "../bindings/SortField";
import { translator } from "../i18n";
import { libraryCommon } from "../i18n/libraryCommon";
import { libraryTopBar } from "../i18n/libraryTopBar";
import { store } from "../lib/store";
import {
  GAME_GROUPS,
  groupLabel,
  GROUPS,
  inSentence,
  MORE_SORTS,
  NUMBER_GROUPS,
  sortLabel,
  SORTS,
} from "../lib/viewOptions";
import { activeFilterCount, shownName } from "../lib/views";
import type { ViewMode } from "../lib/viewMode";
import { FilterPanel } from "./FilterPanel";
import { KeywordFilter, type KeywordFilterHandle } from "./KeywordFilter";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { SaveViewDialog } from "./SaveViewDialog";
import { SearchPicker } from "./SearchPicker";

const tr = translator(libraryTopBar);
const trCommon = translator(libraryCommon);

const LAYOUTS: {
  id: ViewMode;
  key: "layoutGrid" | "layoutList" | "layoutDetails";
  icon: typeof List;
}[] = [
  { id: "grid", key: "layoutGrid", icon: LayoutGrid },
  { id: "list", key: "layoutList", icon: List },
  { id: "details", key: "layoutDetails", icon: PanelLeft },
];

const sortEntry = (sort: { id: SortField; label: string }): MenuEntry => ({
  label: sort.label,
  checked: store.query().sort === sort.id,
  keepOpen: true,
  onSelect: () => store.setSortField(sort.id),
});

const groupEntry = (group: { id: GroupBy; label: string }): MenuEntry => ({
  label: group.label,
  checked: store.groupBy() === group.id,
  onSelect: () => store.setGroupBy(group.id),
});

type Open = "filters" | "sort" | "group" | "jump" | "save" | null;

const filters = () => activeFilterCount(store.query());
const sortName = () => sortLabel(store.query().sort);
const sortButtonLabel = () =>
  tr(store.query().descending ? "sortDescending" : "sortAscending", { sort: sortName() });
const groupButtonLabel = () => {
  const group = store.groupBy();
  return group === "none"
    ? tr("groupBy")
    : tr("groupedBy", { group: inSentence(groupLabel(group)) });
};
const viewName = () => {
  const view = store.activeView();
  return view ? shownName(view) : tr("viewFallback");
};
const changed = () => {
  const view = store.activeView();
  return view !== null && store.hasDraft(view.id);
};
/** "N games", or "N of M games" while the keyword filter narrows the view. */
const count = () => {
  const shown = store.games().length;
  const total = store.viewCounts()[store.activeViewId() ?? ""];
  if (store.keyword() !== null && total !== undefined) return { shown, total };
  return { shown, total: null };
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

  /** Ctrl+G opens "Jump to group" while the library is grouped. */
  function onWindowKeyDown(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "g") return;
    if (store.page() !== "library" || store.groupBy() === "none") return;
    event.preventDefault();
    if (open() !== "jump") toggle("jump", "start");
  }

  onMount(() => {
    window.addEventListener("keydown", onWindowKeyDown);
    onCleanup(() => window.removeEventListener("keydown", onWindowKeyDown));
  });

  return (
    <header class="topbar">
      <KeywordFilter ref={props.keywordRef} />
      <button
        ref={(el) => (buttons.filters = el)}
        type="button"
        class="tool-btn"
        classList={{ "tool-btn--open": open() === "filters", "tool-btn--wide": filters() > 0 }}
        aria-label={filters() > 0 ? tr("filtersActive", { count: filters() }) : tr("filters")}
        title={tr("filters")}
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
        aria-label={sortButtonLabel()}
        title={sortButtonLabel()}
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
      <button
        ref={(el) => (buttons.group = el)}
        type="button"
        class="tool-btn"
        classList={{
          "tool-btn--open": open() === "group",
          "tool-btn--on": store.groupBy() !== "none",
        }}
        aria-label={groupButtonLabel()}
        title={groupButtonLabel()}
        aria-haspopup="menu"
        aria-expanded={open() === "group"}
        onClick={() => toggle("group", "start")}
      >
        <Group class="icon" aria-hidden="true" />
      </button>
      <Show when={store.groupBy() !== "none"}>
        <button
          ref={(el) => (buttons.jump = el)}
          type="button"
          class="tool-btn"
          classList={{ "tool-btn--open": open() === "jump" }}
          aria-label={tr("jumpToGroup")}
          title={tr("jumpTitle")}
          aria-haspopup="dialog"
          aria-expanded={open() === "jump"}
          onClick={() => toggle("jump", "start")}
        >
          <ListTree class="icon" aria-hidden="true" />
        </button>
      </Show>
      <Show when={store.isExplore()}>
        <button type="button" class="btn topbar__save" onClick={() => setSaving(true)}>
          <BookmarkPlus class="icon" aria-hidden="true" />
          {tr("saveAsView")}
        </button>
      </Show>
      <Show when={changed() && !store.isExplore()}>
        <div
          class="view-actions"
          role="group"
          aria-label={tr("unsavedChangesTo", { name: viewName() })}
        >
          <span class="view-actions__label">
            <span class="view-actions__dot" aria-hidden="true" />
            {tr("unsaved")}
          </span>
          <button
            type="button"
            class="view-actions__btn"
            aria-label={tr("resetTo", { name: viewName() })}
            title={tr("resetTitle")}
            onClick={() => store.resetView()}
          >
            <Undo2 class="icon" aria-hidden="true" />
          </button>
          <button
            ref={(el) => (buttons.save = el)}
            type="button"
            class="view-actions__btn"
            classList={{ "view-actions__btn--open": open() === "save" }}
            aria-label={tr("saveView")}
            title={tr("saveView")}
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
        <Show
          when={count().total !== null}
          fallback={trCommon("gamesCount", { count: count().shown })}
        >
          <strong>{count().shown}</strong> {tr("ofTotal", { total: count().total ?? 0 })}
        </Show>
      </span>
      <div class="view-toggle" role="group" aria-label={tr("layout")}>
        <For each={LAYOUTS}>
          {(layout) => (
            <button
              type="button"
              class="icon-btn icon-btn--bordered"
              classList={{ "icon-btn--active": store.viewMode() === layout.id }}
              aria-label={tr(layout.key)}
              aria-pressed={store.viewMode() === layout.id}
              title={tr(layout.key)}
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
      <Show when={open() === "jump" && anchor()}>
        {(at) => (
          <SearchPicker
            anchor={at()}
            label={tr("jumpToGroup")}
            placeholder={tr("typeToFind")}
            empty={tr("noGroupMatches")}
            items={store.groups().map((group) => ({
              id: group.key,
              label: group.label,
              hint: String(group.games.length),
            }))}
            onPick={(key) => store.jumpToGroup(key)}
            onClose={(returnFocus) => close(returnFocus)}
            footer={
              <div class="picker__footer">
                <button
                  type="button"
                  class="btn btn--small btn--ghost"
                  onClick={() => {
                    store.setAllGroupsCollapsed(true);
                    close(true);
                  }}
                >
                  {tr("collapseAll")}
                </button>
                <button
                  type="button"
                  class="btn btn--small btn--ghost"
                  onClick={() => {
                    store.setAllGroupsCollapsed(false);
                    close(true);
                  }}
                >
                  {tr("expandAll")}
                </button>
              </div>
            }
          />
        )}
      </Show>
      <Show when={open() === "group" && anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={tr("groupBy")}
            inside={() => [buttons.group]}
            onClose={close}
            entries={[
              { kind: "heading", label: tr("groupBy") },
              ...GROUPS.map(groupEntry),
              { kind: "separator" },
              {
                kind: "submenu",
                label: tr("aboutGame"),
                hint: GAME_GROUPS.find((g) => g.id === store.groupBy())?.label,
                entries: () => GAME_GROUPS.map(groupEntry),
              },
              {
                kind: "submenu",
                label: tr("timeNumbers"),
                hint: NUMBER_GROUPS.find((g) => g.id === store.groupBy())?.label,
                entries: () => NUMBER_GROUPS.map(groupEntry),
              },
            ]}
          />
        )}
      </Show>
      <Show when={open() === "sort" && anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={tr("sort")}
            inside={() => [buttons.sort]}
            onClose={close}
            entries={[
              ...SORTS.map(sortEntry),
              {
                kind: "submenu",
                label: tr("more"),
                hint: MORE_SORTS.find((s) => s.id === store.query().sort)?.label,
                entries: () => MORE_SORTS.map(sortEntry),
              },
              { kind: "separator" },
              {
                label: tr("ascending"),
                checked: !store.query().descending,
                keepOpen: true,
                onSelect: () => store.setDescending(false),
              },
              {
                label: tr("descending"),
                checked: store.query().descending,
                keepOpen: true,
                onSelect: () => store.setDescending(true),
              },
            ]}
          />
        )}
      </Show>
      <Show when={open() === "save" && anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={tr("saveView")}
            inside={() => [buttons.save]}
            onClose={close}
            entries={[
              {
                label: tr("updateView", { name: viewName() }),
                onSelect: () => void store.updateActiveView(),
              },
              { label: tr("saveAsNew"), onSelect: () => setSaving(true) },
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
