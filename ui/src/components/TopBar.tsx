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
import Ellipsis from "lucide-solid/icons/ellipsis";
import { createEffect, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
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
import { readCssPx } from "../lib/layoutMode";
import { fitToolbar } from "../lib/toolbarFit";
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

type Open = "filters" | "sort" | "group" | "jump" | "save" | "more" | null;

/** Toolbar items that give way on a narrow window, in the order they do. */
type Item = "count" | "layout" | "save" | "jump" | "group" | "sort" | "filters";
const GIVE_WAY: readonly Item[] = ["count", "layout", "save", "jump", "group", "sort", "filters"];
/** The item a popup belongs to (it opens from the More button while the item is in the menu). */
const ITEM_OF: Record<Exclude<Open, null | "more">, Item> = {
  filters: "filters",
  sort: "sort",
  group: "group",
  jump: "jump",
  save: "save",
};

const sortEntries = (): MenuEntry[] => [
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
];

const groupEntries = (): MenuEntry[] => [
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
  { kind: "separator" },
  {
    label: tr("groupList"),
    checked: store.groupList(),
    disabled: store.groupBy() === "none",
    onSelect: () => store.setGroupList(!store.groupList()),
  },
];

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
/** "N games", or "N of M games" while the keyword filter or a picked group narrows the view. */
const count = () => {
  if (store.pickedGroup() !== null) {
    return { shown: store.groups()[0]?.games.length ?? 0, total: store.games().length };
  }
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
  // A narrow toolbar moves items into the More menu (see `fitToolbar`).
  const [hidden, setHidden] = createSignal<ReadonlySet<Item>>(new Set());
  const shows = (item: Item) => !hidden().has(item);
  const items: Partial<Record<Item, HTMLElement>> = {};
  const widths = new Map<Item, number>();
  let header: HTMLElement | undefined;
  /** Items that exist right now (the save actions and "Jump to group" come and go). */
  const present = (): Item[] =>
    GIVE_WAY.filter((item) => {
      if (item === "save") return store.isExplore() || changed();
      if (item === "jump") return store.groupBy() !== "none";
      return true;
    });

  function relayout() {
    // While the More menu is open its entries stay as they are.
    if (!header || open() === "more") return;
    const style = getComputedStyle(header);
    const tool = readCssPx(header, "--qk-tool-size", 32);
    for (const item of GIVE_WAY) {
      const width = items[item]?.getBoundingClientRect().width ?? 0;
      if (width > 0 && shows(item)) widths.set(item, width);
    }
    const keyword = header.querySelector<HTMLElement>(".keyword");
    const keywordWidth = keyword?.classList.contains("keyword--open")
      ? readCssPx(header, "--qk-keyword-min-width", 140)
      : (keyword?.getBoundingClientRect().width ?? tool);
    const available =
      header.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
    // Not laid out (yet), e.g. in tests: nothing gives way.
    if (available <= 0) return;
    const next = fitToolbar({
      available,
      gap: parseFloat(style.columnGap) || 0,
      // The filter box and the spacer always stay.
      fixed: [keywordWidth, 0],
      items: present().map((id) => ({
        id,
        width: widths.get(id) ?? tool,
        menu: id !== "count",
      })),
      moreWidth: tool,
    }) as Set<Item>;
    const current = hidden();
    if (next.size !== current.size || [...next].some((id) => !current.has(id))) setHidden(next);
  }

  onMount(() => {
    if (!header) return;
    const observer = new ResizeObserver(() => relayout());
    observer.observe(header);
    const keyword = header.querySelector(".keyword");
    if (keyword) observer.observe(keyword);
    onCleanup(() => observer.disconnect());
  });
  createEffect(on(present, () => queueMicrotask(relayout), { defer: true }));
  createEffect(on(open, (now) => now === null && queueMicrotask(relayout), { defer: true }));

  function toggle(which: Exclude<Open, null>, align: "start" | "end") {
    if (open() === which) {
      setOpen(null);
      return;
    }
    const inMenu = which !== "more" && hidden().has(ITEM_OF[which]);
    const rect = (inMenu ? buttons.more : buttons[which])?.getBoundingClientRect();
    if (!rect) return;
    setAnchor({ rect, align });
    setOpen(which);
  }

  /** The items that gave way, as entries of the More menu. */
  function moreEntries(): MenuEntry[] {
    const groups: MenuEntry[][] = [];
    if (hidden().has("filters")) {
      groups.push([
        {
          label: filters() > 0 ? tr("filtersActive", { count: filters() }) : tr("filters"),
          icon: () => <Funnel class="icon" aria-hidden="true" />,
          onSelect: () => toggle("filters", "end"),
        },
      ]);
    }
    const options: MenuEntry[] = [];
    if (hidden().has("sort")) {
      options.push({
        kind: "submenu",
        label: tr("sort"),
        hint: sortName(),
        entries: sortEntries,
      });
    }
    if (hidden().has("group")) {
      options.push({
        kind: "submenu",
        label: tr("groupBy"),
        hint: store.groupBy() === "none" ? undefined : groupLabel(store.groupBy()),
        entries: groupEntries,
      });
    }
    if (hidden().has("jump") && store.groupBy() !== "none") {
      options.push({ label: tr("jumpToGroup"), onSelect: () => toggle("jump", "end") });
    }
    if (options.length > 0) groups.push(options);
    if (hidden().has("save") && store.isExplore()) {
      groups.push([{ label: tr("saveAsView"), onSelect: () => setSaving(true) }]);
    } else if (hidden().has("save") && changed()) {
      groups.push([
        { label: tr("resetTitle"), onSelect: () => store.resetView() },
        {
          label: tr("updateView", { name: viewName() }),
          onSelect: () => void store.updateActiveView(),
        },
        { label: tr("saveAsNew"), onSelect: () => setSaving(true) },
      ]);
    }
    if (hidden().has("layout")) {
      groups.push([
        { kind: "heading", label: tr("layout") },
        ...LAYOUTS.map((layout): MenuEntry => ({
          label: tr(layout.key),
          checked: store.viewMode() === layout.id,
          onSelect: () => store.setViewMode(layout.id),
        })),
      ]);
    }
    const entries: MenuEntry[] = [];
    for (const group of groups) {
      if (entries.length > 0) entries.push({ kind: "separator" });
      entries.push(...group);
    }
    return entries;
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
    <header class="topbar" ref={(el) => (header = el)}>
      <KeywordFilter ref={props.keywordRef} />
      <Show when={shows("filters")}>
        <button
          ref={(el) => (buttons.filters = items.filters = el)}
          type="button"
          class="tool-btn"
          classList={{ "tool-btn--open": open() === "filters", "tool-btn--wide": filters() > 0 }}
          aria-label={filters() > 0 ? tr("filtersActive", { count: filters() }) : tr("filters")}
          data-tooltip={tr("filters")}
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
      </Show>
      <Show when={shows("sort")}>
        <button
          ref={(el) => (buttons.sort = items.sort = el)}
          type="button"
          class="tool-btn"
          classList={{ "tool-btn--open": open() === "sort" }}
          aria-label={sortButtonLabel()}
          data-tooltip={sortButtonLabel()}
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
      </Show>
      <Show when={shows("group")}>
        <button
          ref={(el) => (buttons.group = items.group = el)}
          type="button"
          class="tool-btn"
          classList={{
            "tool-btn--open": open() === "group",
            "tool-btn--on": store.groupBy() !== "none",
          }}
          aria-label={groupButtonLabel()}
          data-tooltip={groupButtonLabel()}
          aria-haspopup="menu"
          aria-expanded={open() === "group"}
          onClick={() => toggle("group", "start")}
        >
          <Group class="icon" aria-hidden="true" />
        </button>
      </Show>
      <Show when={store.groupBy() !== "none" && shows("jump")}>
        <button
          ref={(el) => (buttons.jump = items.jump = el)}
          type="button"
          class="tool-btn"
          classList={{ "tool-btn--open": open() === "jump" }}
          aria-label={tr("jumpToGroup")}
          data-tooltip={tr("jumpTitle")}
          aria-haspopup="dialog"
          aria-expanded={open() === "jump"}
          onClick={() => toggle("jump", "start")}
        >
          <ListTree class="icon" aria-hidden="true" />
        </button>
      </Show>
      <Show when={store.isExplore() && shows("save")}>
        <button
          ref={(el) => (items.save = el)}
          type="button"
          class="btn topbar__save"
          onClick={() => setSaving(true)}
        >
          <BookmarkPlus class="icon" aria-hidden="true" />
          {tr("saveAsView")}
        </button>
      </Show>
      <Show when={changed() && !store.isExplore() && shows("save")}>
        <div
          ref={(el) => (items.save = el)}
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
            data-tooltip={tr("resetTitle")}
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
            data-tooltip={tr("saveView")}
            aria-haspopup="menu"
            aria-expanded={open() === "save"}
            onClick={() => toggle("save", "start")}
          >
            <BookmarkPlus class="icon" aria-hidden="true" />
          </button>
        </div>
      </Show>
      <div class="topbar__spacer" />
      <span
        ref={(el) => (items.count = el)}
        class="topbar__count"
        classList={{ "visually-hidden": !shows("count") }}
        aria-live="polite"
      >
        <Show
          when={count().total !== null}
          fallback={trCommon("gamesCount", { count: count().shown })}
        >
          <strong>{count().shown}</strong> {tr("ofTotal", { total: count().total ?? 0 })}
        </Show>
      </span>
      <Show when={hidden().size > (hidden().has("count") ? 1 : 0)}>
        <button
          ref={(el) => (buttons.more = el)}
          type="button"
          class="tool-btn"
          classList={{ "tool-btn--open": open() === "more" }}
          aria-label={tr("moreTools")}
          data-tooltip={tr("moreTools")}
          aria-haspopup="menu"
          aria-expanded={open() === "more"}
          // Keeps the focus (and an open, empty filter box) where it is.
          onPointerDown={(event) => event.preventDefault()}
          onClick={() => toggle("more", "end")}
        >
          <Ellipsis class="icon" aria-hidden="true" />
        </button>
      </Show>
      <Show when={shows("layout")}>
        <div
          ref={(el) => (items.layout = el)}
          class="view-toggle"
          role="group"
          aria-label={tr("layout")}
        >
          <For each={LAYOUTS}>
            {(layout) => (
              <button
                type="button"
                class="icon-btn icon-btn--bordered"
                classList={{ "icon-btn--active": store.viewMode() === layout.id }}
                aria-label={tr(layout.key)}
                aria-pressed={store.viewMode() === layout.id}
                data-tooltip={tr(layout.key)}
                onClick={() => store.setViewMode(layout.id)}
              >
                <layout.icon class="icon" aria-hidden="true" />
              </button>
            )}
          </For>
        </div>
      </Show>
      <Show when={open() === "filters" && anchor()}>
        {(at) => (
          <FilterPanel
            anchor={at()}
            onClose={close}
            inside={() => [buttons.filters, buttons.more]}
          />
        )}
      </Show>
      <Show when={open() === "more" && anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={tr("moreTools")}
            inside={() => [buttons.more]}
            onClose={close}
            entries={moreEntries()}
          />
        )}
      </Show>
      <Show when={open() === "jump" && anchor()}>
        {(at) => (
          <SearchPicker
            anchor={at()}
            label={tr("jumpToGroup")}
            placeholder={tr("typeToFind")}
            empty={tr("noGroupMatches")}
            items={store.allGroups().map((group) => ({
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
            entries={[{ kind: "heading", label: tr("groupBy") }, ...groupEntries()]}
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
            entries={sortEntries()}
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
